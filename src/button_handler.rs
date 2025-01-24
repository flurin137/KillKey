use defmt::{info, Format};
use embassy_rp::{
    gpio::{Input, Pin, Pull},
    Peripheral,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

#[derive(Clone, Copy, Format)]
pub enum Button {
    Kill,
    Lock,
    Wiggle,
}

#[derive(Clone, Copy, Format)]
pub enum Event {
    Pressed,
    Released,
}

pub struct ButtonHandler<'a> {
    signal: &'a Signal<ThreadModeRawMutex, (Button, Event)>,
    input: Input<'a>,
    button: Button,
}

impl<'a> ButtonHandler<'a> {
    pub fn new(
        signal: &'a Signal<ThreadModeRawMutex, (Button, Event)>,
        pin: impl Peripheral<P = impl Pin> + 'a,
        button: Button,
    ) -> Self {
        let input = Input::new(pin, Pull::Up);
        Self {
            signal,
            input,
            button,
        }
    }

    pub async fn handle_normally_open(&mut self) -> ! {
        loop {
            self.input.wait_for_falling_edge().await;
            Timer::after(Duration::from_millis(10)).await;
            self.signal.signal((self.button, Event::Pressed));

            info!("Button {} pressed", self.button);

            self.input.wait_for_high().await;
            self.signal.signal((self.button, Event::Released));
        }
    }

    pub async fn handle_normally_closed(&mut self) -> ! {
        loop {
            self.input.wait_for_rising_edge().await;
            Timer::after(Duration::from_millis(10)).await;
            self.signal.signal((self.button, Event::Pressed));

            info!("Button {} pressed", self.button);

            self.input.wait_for_low().await;
            self.signal.signal((self.button, Event::Released));
        }
    }
}
