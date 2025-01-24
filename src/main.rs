#![no_std]
#![no_main]
#![allow(stable_features, unknown_lints, async_fn_in_trait)]

mod button_handler;
mod keyboard_handler;
mod led_handler;
mod mouse_handler;
mod rgb_led;

use button_handler::Button;
use button_handler::ButtonHandler;
use button_handler::Event;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join5;
use embassy_futures::join::join_array;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::PIO0;
use embassy_rp::peripherals::USB;
use embassy_rp::pio::Pio;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{HidWriter, State};
use embassy_usb::Builder;
use keyboard_handler::KeyboardHandler;
use led_handler::LedHandler;
use mouse_handler::MouseHandler;
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

static ENABLE_WIGGLE: AtomicBool = AtomicBool::new(false);
static KILL_BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);

static KEYBOARD_COMMAND: Signal<ThreadModeRawMutex, Command> = Signal::new();
static BUTTONS_SIGNAL: Signal<ThreadModeRawMutex, (Button, Event)> = Signal::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let driver = Driver::new(peripherals.USB, Irqs);

    let mut config = embassy_usb::Config::new(0xc0de, 0xaffe);
    config.manufacturer = Some("Fx137");
    config.product = Some("KILL EM ALL");
    config.serial_number = Some("6942");

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

    let keyboard_writer =
        HidWriter::<_, 8>::new(&mut builder, &mut keyboard_state, keyboard_configuration);

    let mouse_writer = HidWriter::<_, 8>::new(&mut builder, &mut mouse_state, mouse_configuration);

    let mut keyboard_handler = KeyboardHandler::new(keyboard_writer);
    let mut mouse_handler = MouseHandler::new(mouse_writer);

    let mut led_handler =
        LedHandler::new(&mut common, sm0, peripherals.DMA_CH0, peripherals.PIN_20);

    let mut usb = builder.build();
    let usb_future = usb.run();

    let mut wiggle_handler =
        ButtonHandler::new(&BUTTONS_SIGNAL, peripherals.PIN_10, Button::Wiggle);
    let mut lock_handler = ButtonHandler::new(&BUTTONS_SIGNAL, peripherals.PIN_6, Button::Lock);
    let mut kill_handler = ButtonHandler::new(&BUTTONS_SIGNAL, peripherals.PIN_26, Button::Kill);

    let signal_handler_future = async {
        loop {
            let pressed = BUTTONS_SIGNAL.wait().await;

            match pressed {
                (Button::Kill, Event::Pressed) => {
                    KILL_BUTTON_PRESSED.store(true, Ordering::Relaxed)
                }
                (Button::Kill, Event::Released) => {
                    KILL_BUTTON_PRESSED.store(false, Ordering::Relaxed)
                }
                (Button::Lock, Event::Pressed) => KEYBOARD_COMMAND.signal(Command::Lock),
                (Button::Lock, Event::Released) => {}
                (Button::Wiggle, Event::Pressed) => ENABLE_WIGGLE.store(true, Ordering::Relaxed),
                (Button::Wiggle, Event::Released) => ENABLE_WIGGLE.store(false, Ordering::Relaxed),
            }
        }
    };

    let mouse_handler_future = async {
        loop {
            let enable = ENABLE_WIGGLE.load(Ordering::Relaxed);

            Timer::after(Duration::from_millis(500)).await;
            if enable {
                mouse_handler.update_position().await
            }
        }
    };

    let keyboard_handler_future = async {
        loop {
            let command = KEYBOARD_COMMAND.wait().await;

            match command {
                Command::Kill => keyboard_handler.handle_kill().await,
                Command::Lock => keyboard_handler.handle_lock().await,
            };
        }
    };

    let kill_handler_future = async {
        let abort = || !KILL_BUTTON_PRESSED.load(Ordering::Relaxed);

        loop {
            if KILL_BUTTON_PRESSED.load(Ordering::Relaxed)
                && led_handler.start_lights(&abort).await.is_some()
            {
                KEYBOARD_COMMAND.signal(Command::Kill);
                Timer::after_secs(1).await;

                while KILL_BUTTON_PRESSED.load(Ordering::Relaxed) {
                    Timer::after_millis(20).await;
                }
            }
            Timer::after_millis(20).await;
        }
    };

    join(
        join5(
            kill_handler_future,
            keyboard_handler_future,
            mouse_handler_future,
            signal_handler_future,
            usb_future,
        ),
        join_array([
            wiggle_handler.handle_normally_open(),
            lock_handler.handle_normally_open(),
            kill_handler.handle_normally_open(),
        ]),
    )
    .await;
}
