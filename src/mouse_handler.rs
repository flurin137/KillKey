use defmt::warn;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::hid::HidWriter;
use usbd_hid::descriptor::MouseReport;

pub struct MouseHandler<'a> {
    mouse_writer: HidWriter<'a, Driver<'a, USB>, 8>,
    current_move: i8,
}

impl<'a> MouseHandler<'a> {
    pub fn new(mouse_writer: HidWriter<'a, Driver<'a, USB>, 8>) -> Self {
        Self {
            mouse_writer,
            current_move: 5,
        }
    }

    pub async fn update_position(&mut self) {
        self.current_move = -self.current_move;

        let report = MouseReport {
            buttons: 0,
            x: 0,
            y: self.current_move,
            wheel: 0,
            pan: 0,
        };

        match self.mouse_writer.write_serialize(&report).await {
            Ok(()) => {}
            Err(e) => warn!("Failed to send report: {:?}", e),
        }
    }
}
