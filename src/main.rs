#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]
#![allow(stable_features, unknown_lints, async_fn_in_trait)]

mod rgb_led;

use crate::rgb_led::RgbLEDs;
use crate::rgb_led::RGB;
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join4;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{HidWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use embassy_usb::Builder;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

static KILL: Signal<ThreadModeRawMutex, ()> = Signal::new();

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

    let mut button = Input::new(peripherals.PIN_19, Pull::Up);

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

            info!("Button Pressed");

            Timer::after(Duration::from_millis(5000)).await;

            KILL.signal(());
        }
    };

    let led_fut = async {
        //const NUM_LEDS: usize = 16;
        let mut led = RgbLEDs::new(peripherals.PIN_20);

        loop {
            Timer::after_secs(1).await;
            

            led.write_colors(&[
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
                RGB::new(50, 50, 0),
                RGB::new(0, 0, 20),
            ])
            .await;

            Timer::after_secs(1).await;

            led.write_colors(&[
                RGB::new(0, 0, 0),
                RGB::new(0, 0, 0),
                RGB::new(0, 0, 0),
                RGB::new(0, 0, 0),
            ])
            .await;
        }
    };

    join(button_fut, led_fut).await;
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
