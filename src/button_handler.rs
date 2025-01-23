use embassy_rp::{
    gpio::{Input, Pin, Pull},
    Peripheral,
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

pub struct ButtonHandler<'a> {
    signal: &'a Signal<ThreadModeRawMutex, bool>,
    input: Input<'a>,
}

impl<'a> ButtonHandler<'a> {
    pub fn new(
        signal: &'a Signal<ThreadModeRawMutex, bool>,
        pin: impl Peripheral<P = impl Pin> + 'a,
    ) -> Self {
        let input = Input::new(pin, Pull::Up);
        Self { signal, input }
    }

    pub async fn handle_normally_open(&mut self) -> ! {
        loop {
            self.input.wait_for_rising_edge().await;
            Timer::after(Duration::from_millis(10)).await;
            self.signal.signal(true);

            self.input.wait_for_low().await;
            self.signal.signal(false);
        }
    }
}
