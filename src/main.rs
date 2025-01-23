#![no_std]
#![no_main]
#![allow(stable_features, unknown_lints, async_fn_in_trait)]

mod button_handler;
mod keyboard_handler;
mod rgb_led;

use crate::rgb_led::full_red;
use crate::rgb_led::off;
use crate::rgb_led::single;
use crate::rgb_led::LedRing;
use button_handler::ButtonHandler;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join3;
use embassy_futures::join::join_array;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::peripherals::PIO0;
use embassy_rp::peripherals::USB;
use embassy_rp::pio::Instance;
use embassy_rp::pio::Pio;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Ticker;
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use keyboard_handler::KeyboardHandler;
use smart_leds::colors::BLUE;
use smart_leds::colors::GREEN;
use smart_leds::colors::ORANGE;
use smart_leds::colors::YELLOW;
use usbd_hid::descriptor::MouseReport;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

enum Command {
    Kill,
    Lock,
}

static COMMAND: Signal<ThreadModeRawMutex, Command> = Signal::new();
static ENABLE_WIGGLE: AtomicBool = AtomicBool::new(false);

static KILL_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);
static WIGGLE_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);
static LOCK_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);

static KILL_BUTTON_SIGNAL: Signal<ThreadModeRawMutex, bool> = Signal::new();
static WIGGLE_BUTTON_SIGNAL: Signal<ThreadModeRawMutex, bool> = Signal::new();
static LOCK_BUTTON_SIGNAL: Signal<ThreadModeRawMutex, bool> = Signal::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let driver = Driver::new(peripherals.USB, Irqs);

    let mut config = embassy_usb::Config::new(0xc0de, 0xaffe);
    config.manufacturer = Some("Fx137");
    config.product = Some("KILL EM ALL");
    config.serial_number = Some("6942");

    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut keyboard_state = State::new();
    let mut mouse_state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    let keyboard_configuration = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };

    let mouse_configuration = embassy_usb::class::hid::Config {
        report_descriptor: MouseReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };

    let Pio {
        mut common, sm0, ..
    } = Pio::new(peripherals.PIO0, Irqs);

    let lock_button = Input::new(peripherals.PIN_6, Pull::Down);
    let wiggle_button = Input::new(peripherals.PIN_10, Pull::Down);
    let kill_button = Input::new(peripherals.PIN_26, Pull::Down);

    let keyboard_writer =
        HidWriter::<_, 8>::new(&mut builder, &mut keyboard_state, keyboard_configuration);

    let mut keyboard_handler = KeyboardHandler::new(keyboard_writer);

    let mut mouse_writer =
        HidWriter::<_, 8>::new(&mut builder, &mut mouse_state, mouse_configuration);

    let mut usb = builder.build();
    let usb_future = usb.run();

    let mut wiggle_handler = ButtonHandler::new(&WIGGLE_BUTTON_SIGNAL, wiggle_button);
    let mut lock_handler = ButtonHandler::new(&LOCK_BUTTON_SIGNAL, lock_button);
    let mut kill_handler = ButtonHandler::new(&KILL_BUTTON_SIGNAL, kill_button);

    let mouse_command_handler = async {
        let mut y: i8 = 5;

        loop {
            let enable = ENABLE_WIGGLE.load(Ordering::Relaxed);

            Timer::after(Duration::from_millis(500)).await;
            if enable {
                y = -y;
                let report = MouseReport {
                    buttons: 0,
                    x: 0,
                    y,
                    wheel: 0,
                    pan: 0,
                };
                match mouse_writer.write_serialize(&report).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }
            }
        }
    };

    let keyboard_handler_future = async {
        loop {
            let command = COMMAND.wait().await;

            match command {
                Command::Kill => keyboard_handler.handle_kill().await,
                Command::Lock => keyboard_handler.handle_lock().await,
            };
        }
    };

    let led_future = async {
        let mut led_ring = LedRing::new(&mut common, sm0, peripherals.DMA_CH0, peripherals.PIN_20);

        loop {
            if KILL_BUTTON_PRESSED.load(Ordering::Relaxed)
                && start_lights(&mut led_ring).await.is_some()
            {
                COMMAND.signal(Command::Kill);
                Timer::after_secs(1).await;

                while KILL_BUTTON_PRESSED.load(Ordering::Relaxed) {
                    Timer::after_millis(20).await;
                }
            }
            Timer::after_millis(20).await;
        }
    };

    join3(usb_future, led_future, keyboard_handler_future).await;
    join_array([
        wiggle_handler.handle_normally_open(),
        lock_handler.handle_normally_open(),
        kill_handler.handle_normally_open(),
    ])
    .await;
}

async fn start_lights<'a, P: Instance, const S: usize>(
    led_ring: &mut LedRing<'a, P, S>,
) -> Option<()> {
    let mut ticker_fast = Ticker::every(Duration::from_millis(50));

    for color in [GREEN, BLUE, YELLOW, ORANGE] {
        for _ in 0..2 {
            for j in 0..led_ring.size {
                led_ring.write(&single(j, color)).await;
                ticker_fast.next().await;
                update_led_on_button_off(led_ring).await?;
            }
        }
    }

    for _ in 0..3 {
        led_ring.write(&full_red()).await;

        for _ in 0..10 {
            ticker_fast.next().await;
            update_led_on_button_off(led_ring).await?;
        }

        led_ring.write(&off()).await;

        for _ in 0..10 {
            ticker_fast.next().await;
            update_led_on_button_off(led_ring).await?;
        }
    }
    Some(())
}

async fn update_led_on_button_off<'a, P: Instance, const S: usize>(
    led_ring: &mut LedRing<'a, P, S>,
) -> Option<()> {
    match KILL_BUTTON_PRESSED.load(Ordering::Relaxed) {
        true => Some(()),
        false => {
            led_ring.write(&off()).await;
            None
        }
    }
}
