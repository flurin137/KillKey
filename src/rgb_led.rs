use embassy_rp::dma::{AnyChannel, Channel};
use embassy_rp::pio::{Common, Config, Instance, PioPin, StateMachine};
use embassy_rp::pio::{FifoJoin, ShiftConfig, ShiftDirection};
use embassy_rp::{clocks, into_ref, Peripheral, PeripheralRef};
use embassy_time::Timer;
use fixed::types::U24F8;
use fixed_macro::fixed;
use smart_leds::colors::RED;
use smart_leds::RGB8;

pub struct LedRing<'d, P: Instance, const S: usize, const N: usize> {
    dma: PeripheralRef<'d, AnyChannel>,
    sm: StateMachine<'d, P, S>,
}

const T1: u8 = 2;
// start bit
const T2: u8 = 5;
// data bit
const T3: u8 = 3;
// stop bit
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;

impl<'d, P: Instance, const S: usize, const N: usize> LedRing<'d, P, S, N> {
    pub fn new(
        pio: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: impl Peripheral<P = impl Channel> + 'd,
        pin: impl PioPin,
    ) -> Self {
        into_ref!(dma);

        // prepare the PIO program
        let side_set = pio::SideSet::new(false, 1, false);
        let program = create_program(side_set);
        let mut config = Config::default();

        // Pin config
        let out_pin = pio.make_pio_pin(pin);
        config.set_out_pins(&[&out_pin]);
        config.set_set_pins(&[&out_pin]);

        config.use_program(&pio.load_program(&program), &[&out_pin]);

        let clock_freq = U24F8::from_num(clocks::clk_sys_freq() / 1000);
        let ws2812_freq = fixed!(800: U24F8);
        let bit_freq = ws2812_freq * CYCLES_PER_BIT;
        config.clock_divider = clock_freq / bit_freq;

        // FIFO config
        config.fifo_join = FifoJoin::TxOnly;
        config.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 24,
            direction: ShiftDirection::Left,
        };

        sm.set_config(&config);
        sm.set_enable(true);

        Self {
            dma: dma.map_into(),
            sm,
        }
    }

    pub async fn write(&mut self, colors: &[RGB8; N]) {
        // Precompute the word bytes from the colors
        let mut words = [0u32; N];
        for i in 0..N {
            let word = (u32::from(colors[i].g) << 24)
                | (u32::from(colors[i].r) << 16)
                | (u32::from(colors[i].b) << 8);
            words[i] = word;
        }

        // DMA transfer
        self.sm.tx().dma_push(self.dma.reborrow(), &words).await;

        Timer::after_micros(55).await;
    }
}

fn create_program(side_set: pio::SideSet) -> pio::Program<32> {
    let mut assembler: pio::Assembler<32> = pio::Assembler::new_with_side_set(side_set);

    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();
    let mut do_zero = assembler.label();
    assembler.set_with_side_set(pio::SetDestination::PINDIRS, 1, 0);
    assembler.bind(&mut wrap_target);
    // Do stop bit
    assembler.out_with_delay_and_side_set(pio::OutDestination::X, 1, T3 - 1, 0);
    // Do start bit
    assembler.jmp_with_delay_and_side_set(pio::JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
    // Do data bit = 1
    assembler.jmp_with_delay_and_side_set(pio::JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
    assembler.bind(&mut do_zero);
    // Do data bit = 0
    assembler.nop_with_delay_and_side_set(T2 - 1, 0);
    assembler.bind(&mut wrap_source);

    let program = assembler.assemble_with_wrap(wrap_source, wrap_target);
    program
}

pub fn full_red() -> [RGB8; 16] {
    [RED; 16]
}

pub fn off() -> [RGB8; 16] {
    [RGB8::default(); 16]
}

pub fn single(index: usize, color: RGB8) -> [RGB8; 16] {
    let mut data = [RGB8::default(); 16];

    data[index] = color;

    data
}
