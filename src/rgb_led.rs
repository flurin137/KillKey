use embassy_rp::gpio::{Level, Output, Pin};
use embassy_time::Timer;

const LONG_TIME: u64 = 800;
const SHORT_TIME: u64 = 450;

pub struct RGB {
    red: u8,
    green: u8,
    blue: u8,
}

impl RGB {
    pub fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

pub struct RgbLED<'d, T: Pin> {
    output: Output<'d, T>,
}

impl<'d, T: Pin> RgbLED<'d, T> {
    pub fn new(pin: T) -> Self {
        let output = Output::new(pin, Level::Low);
        Self { output }
    }

    async fn write_byte(&mut self, mut data: u8) {
        for _ in 0..8 {
            if (data & 0x80) != 0 {
                self.output.set_high();
                Timer::after_nanos(SHORT_TIME).await;
                self.output.set_low();
                Timer::after_nanos(LONG_TIME).await;
            } else {
                self.output.set_high();
                Timer::after_nanos(LONG_TIME).await;
                self.output.set_low();
                Timer::after_nanos(SHORT_TIME).await;
            }

            data <<= 1;
        }
    }

    pub async fn write_colors(&mut self, colors: &[RGB]) {
        for color in colors {
            self.write_byte(color.green).await;
            self.write_byte(color.red).await;
            self.write_byte(color.blue).await;
        }
    }
}
