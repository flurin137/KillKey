#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]
#![allow(stable_features, unknown_lints, async_fn_in_trait)]

use core::sync::atomic::{AtomicBool, Ordering};

use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{AnyPin, Input, Level, Output, Pin, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{HidWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use embassy_usb::Builder;
use usbd_hid::descriptor::{MouseReport, SerializedDescriptor};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

static ENABLE_WIGGLE: AtomicBool = AtomicBool::new(false);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let peripherals = embassy_rp::init(Default::default());
    let driver = Driver::new(peripherals.USB, Irqs);

    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Fx137");
    config.product = Some("KILL EM ALL");
    config.serial_number = Some("4269");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

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
        report_descriptor: MouseReport::desc(),
        request_handler: Some(&request_handler),
        poll_ms: 60,
        max_packet_size: 8,
    };

    unwrap!(spawner.spawn(io_task(
        peripherals.PIN_13.degrade(),
        peripherals.PIN_9.degrade()
    )));

    let mut writer = HidWriter::<_, 5>::new(&mut builder, &mut state, config);

    let mut usb = builder.build();

    let usb_future = usb.run();

    let hid_future = async {
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
                match writer.write_serialize(&report).await {
                    Ok(()) => {}
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }
            }
        }
    };

    // Run everything concurrently.
    // If we had made everything `'static` above instead, we could do this using separate tasks instead.
    join(usb_future, hid_future).await;
}

#[embassy_executor::task]
async fn io_task(button_pin: AnyPin, led_pin: AnyPin) {
    let mut button = Input::new(button_pin, Pull::Up);
    let mut led = Output::new(led_pin, Level::Low);

    let mut value = false;

    loop {
        button.wait_for_falling_edge().await;
        value = !value;

        let level = match value {
            true => Level::High,
            false => Level::Low,
        };
        led.set_level(level);
        ENABLE_WIGGLE.store(value, Ordering::Relaxed);

        Timer::after_millis(10).await;
        button.wait_for_high().await;
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
