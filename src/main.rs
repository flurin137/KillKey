#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]
#![allow(stable_features, unknown_lints, async_fn_in_trait)]

mod rgb_led;

use crate::rgb_led::full_red;
use crate::rgb_led::off;
use crate::rgb_led::single;
use crate::rgb_led::LedRing;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join4;
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
use embassy_usb::class::hid::{HidWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use embassy_usb::Builder;
use smart_leds::colors::BLUE;
use smart_leds::colors::GREEN;
use smart_leds::colors::ORANGE;
use smart_leds::colors::YELLOW;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

static KILL: Signal<ThreadModeRawMutex, ()> = Signal::new();
static BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let driver = Driver::new(peripherals.USB, Irqs);

    let mut config = embassy_usb::Config::new(0xc0de, 0xaffe);
    config.manufacturer = Some("Fx137");
    config.product = Some("KILL EM ALL");
    config.serial_number = Some("4269");

    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    let request_handler = MyRequestHandler {};

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut device_descriptor,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );

    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: Some(&request_handler),
        poll_ms: 60,
        max_packet_size: 64,
    };

    let Pio {
        mut common, sm0, ..
    } = Pio::new(peripherals.PIO0, Irqs);

    let mut button = Input::new(peripherals.PIN_26, Pull::Down);

    let mut writer = HidWriter::<_, 8>::new(&mut builder, &mut state, config);

    let mut usb = builder.build();

    let usb_future = usb.run();

    let hid_future = async {
        loop {
            KILL.wait().await;

            let report = KeyboardReport {
                keycodes: [0x6c, 0, 0, 0, 0, 0],
                leds: 0,
                modifier: 0x03,
                reserved: 0,
            };

            match writer.write_serialize(&report).await {
                Ok(()) => {
                    info!("Hid repport written");
                }
                Err(e) => warn!("Failed to send report: {:?}", e),
            }

            let report = KeyboardReport {
                keycodes: [0, 0, 0, 0, 0, 0],
                leds: 0,
                modifier: 0,
                reserved: 0,
            };

            match writer.write_serialize(&report).await {
                Ok(()) => {
                    info!("Hid repport written");
                }
                Err(e) => warn!("Failed to send report: {:?}", e),
            }
        }
    };

    let button_fut = async {
        loop {
            button.wait_for_falling_edge().await;
            Timer::after(Duration::from_millis(10)).await;

            BUTTON_PRESSED.store(true, Ordering::Relaxed);

            button.wait_for_high().await;
            BUTTON_PRESSED.store(false, Ordering::Relaxed);
        }
    };

    let led_fut = async {
        let mut led_ring = LedRing::new(&mut common, sm0, peripherals.DMA_CH0, peripherals.PIN_20);

        loop {
            if BUTTON_PRESSED.load(Ordering::Relaxed) && start_lights(&mut led_ring).await.is_some()
            {
                KILL.signal(());
                Timer::after_secs(1).await;

                while BUTTON_PRESSED.load(Ordering::Relaxed) {
                    Timer::after_millis(20).await;
                }
            }
            Timer::after_millis(20).await;
        }
    };

    join4(button_fut, led_fut, usb_future, hid_future).await;
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

async fn update_led_on_button_off<'a, P: Instance, const S: usize>(led_ring: &mut LedRing<'a, P, S>) -> Option<()> {
    match BUTTON_PRESSED.load(Ordering::Relaxed) {
        true => Some(()),
        false => {
            led_ring.write(&off()).await;
            None
        }
    }
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&self, _: ReportId, _buf: &mut [u8]) -> Option<usize> {
        None
    }

    fn set_report(&self, _: ReportId, _: &[u8]) -> OutResponse {
        OutResponse::Accepted
    }

    fn set_idle_ms(&self, _: Option<ReportId>, _: u32) {}

    fn get_idle_ms(&self, _: Option<ReportId>) -> Option<u32> {
        None
    }
}
