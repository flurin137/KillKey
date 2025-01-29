use defmt::{info, warn};
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::hid::HidWriter;
use usbd_hid::descriptor::{KeyboardReport, KeyboardUsage};

pub struct KeyboardHandler<'a> {
    keyboard_writer: HidWriter<'a, Driver<'a, USB>, 8>,
}

impl<'a> KeyboardHandler<'a> {
    pub fn new(keyboard_writer: HidWriter<'a, Driver<'a, USB>, 8>) -> Self {
        Self { keyboard_writer }
    }

    pub async fn handle_lock(&mut self) {
        self.handle_keys([KeyboardUsage::KeyboardLl as u8, 0, 0, 0, 0, 0], 0x08)
            .await
    }

    pub async fn handle_kill(&mut self) {
        self.handle_keys([KeyboardUsage::KeyboardF17 as u8, 0, 0, 0, 0, 0], 0x03)
            .await
    }

    async fn handle_keys(&mut self, key_codes: [u8; 6], modifiers: u8) {
        let report = KeyboardReport {
            keycodes: key_codes,
            leds: 0,
            modifier: modifiers,
            reserved: 0,
        };

        match self.keyboard_writer.write_serialize(&report).await {
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

        match self.keyboard_writer.write_serialize(&report).await {
            Ok(()) => {
                info!("Hid repport written");
            }
            Err(e) => warn!("Failed to send report: {:?}", e),
        }
    }
}
