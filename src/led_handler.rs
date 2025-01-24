use crate::rgb_led::full_red;
use crate::rgb_led::off;
use crate::rgb_led::single;
use crate::rgb_led::LedRing;
use embassy_rp::dma::Channel;
use embassy_rp::pio::Common;
use embassy_rp::pio::Instance;
use embassy_rp::pio::PioPin;
use embassy_rp::pio::StateMachine;
use embassy_rp::Peripheral;
use embassy_time::Duration;
use embassy_time::Ticker;
use smart_leds::colors::BLUE;
use smart_leds::colors::GREEN;
use smart_leds::colors::ORANGE;
use smart_leds::colors::YELLOW;

pub struct LedHandler<'a, P: Instance, const S: usize> {
    led_ring: LedRing<'a, P, S>,
}

impl<'a, P: Instance, const S: usize> LedHandler<'a, P, S> {
    pub fn new(
        pio: &mut Common<'a, P>,
        sm: StateMachine<'a, P, S>,
        dma: impl Peripheral<P = impl Channel> + 'a,
        pin: impl PioPin,
    ) -> Self {
        let led_ring = LedRing::new(pio, sm, dma, pin);
        Self { led_ring }
    }

    pub async fn start_lights(&mut self, abort: &impl Fn() -> bool) -> Option<()> {
        let mut ticker_fast = Ticker::every(Duration::from_millis(50));

        for color in [GREEN, BLUE, YELLOW, ORANGE] {
            for _ in 0..2 {
                for j in 0..self.led_ring.size {
                    self.led_ring.write(&single(j, color)).await;
                    ticker_fast.next().await;
                    self.disable_light_if_aborted(abort).await?;
                }
            }
        }

        for _ in 0..3 {
            self.led_ring.write(&full_red()).await;

            for _ in 0..10 {
                ticker_fast.next().await;
                self.disable_light_if_aborted(abort).await?;
            }

            self.led_ring.write(&off()).await;

            for _ in 0..10 {
                ticker_fast.next().await;
                self.disable_light_if_aborted(abort).await?;
            }
        }
        Some(())
    }

    async fn disable_light_if_aborted(&mut self, abort: &impl Fn() -> bool) -> Option<()> {
        if abort() {
            self.led_ring.write(&off()).await;
            return None;
        }

        Some(())
    }
}
