use core::cell::UnsafeCell;
use freertos_rust::{Duration, InterruptContext, Semaphore};

// ── Output ───────────────────────────────────────────────────────────────────

pub fn set_direction(pin: u8, direction: i32) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    ensure_io_unreset(&p);

    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(5) });

    p.PADS_BANK0.gpio(pin as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        w.ie().clear_bit().od().clear_bit()
    });

    p.SIO
        .gpio_oe_set()
        .write(|w| unsafe { w.bits(1u32 << pin) });

    if direction == 1 {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}

pub fn set_value(pin: u8, high: bool) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    if high {
        p.SIO
            .gpio_out_set()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    } else {
        p.SIO
            .gpio_out_clr()
            .write(|w| unsafe { w.bits(1u32 << pin) });
    }
}

// ── Input ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum Pull {
    None,
    Up,
    Down,
}

pub fn set_input(pin: u8, pull: Pull) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    ensure_io_unreset(&p);

    p.IO_BANK0
        .gpio(pin as usize)
        .gpio_ctrl()
        .write(|w| unsafe { w.funcsel().bits(5) }); // SIO

    p.PADS_BANK0.gpio(pin as usize).write(|w| {
        #[cfg(feature = "chip-rp2350")]
        let w = w.iso().clear_bit();
        let w = w.ie().set_bit().od().clear_bit();
        match pull {
            Pull::Up => w.pue().set_bit().pde().clear_bit(),
            Pull::Down => w.pue().clear_bit().pde().set_bit(),
            Pull::None => w.pue().clear_bit().pde().clear_bit(),
        }
    });

    // Disable output driver
    p.SIO
        .gpio_oe_clr()
        .write(|w| unsafe { w.bits(1u32 << pin) });
}

pub fn read(pin: u8) -> bool {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    (p.SIO.gpio_in().read().bits() >> pin) & 1 != 0
}

// ── Edge interrupt ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum EdgeTrigger {
    Rising,
    Falling,
    Both,
}

pub fn enable_edge_irq(pin: u8, edge: EdgeTrigger) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let reg_idx = pin as usize / 8;
    let bit_pos = (pin as usize % 8) * 4;

    // Bits within the 4-bit group: [level_low, level_high, edge_low, edge_high]
    let edge_low_bit = 1u32 << (bit_pos + 2);
    let edge_high_bit = 1u32 << (bit_pos + 3);

    let mask = match edge {
        EdgeTrigger::Falling => edge_low_bit,
        EdgeTrigger::Rising => edge_high_bit,
        EdgeTrigger::Both => edge_low_bit | edge_high_bit,
    };

    p.IO_BANK0
        .proc0_inte(reg_idx)
        .modify(|r, w| unsafe { w.bits(r.bits() | mask) });
}

pub fn disable_edge_irq(pin: u8) {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    let reg_idx = pin as usize / 8;
    let bit_pos = (pin as usize % 8) * 4;
    let clear_mask = !(0xFu32 << bit_pos);

    p.IO_BANK0
        .proc0_inte(reg_idx)
        .modify(|r, w| unsafe { w.bits(r.bits() & clear_mask) });
}

pub fn init_gpio_irq() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    // Allocate the wake semaphore the first time we install GPIO IRQs.
    // Subsequent calls leave it in place (binary semaphore, signal-latching).
    unsafe {
        if (*BUTTON_WAKE_SEM.0.get()).is_none() {
            let sem = Semaphore::new_binary().expect("BUTTON_WAKE_SEM alloc");
            *BUTTON_WAKE_SEM.0.get() = Some(sem);
        }
    }

    unsafe {
        let nvic_ipr = 0xE000_E400 as *mut u8;
        let irqn = pac::Interrupt::IO_IRQ_BANK0 as u8;
        nvic_ipr.add(irqn as usize).write_volatile(0x10);
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::IO_IRQ_BANK0);
    }
}

// ── GPIO ISR ─────────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn IO_IRQ_BANK0() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    #[cfg(feature = "chip-rp2040")]
    const NUM_REGS: usize = 4;
    #[cfg(feature = "chip-rp2350")]
    const NUM_REGS: usize = 6;

    for reg_idx in 0..NUM_REGS {
        let ints = p.IO_BANK0.proc0_ints(reg_idx).read().bits();
        if ints == 0 {
            continue;
        }
        for bit_group in 0..8u32 {
            let pin = (reg_idx * 8 + bit_group as usize) as u8;
            let shift = bit_group * 4;
            let edge_low = (ints >> (shift + 2)) & 1 != 0;
            let edge_high = (ints >> (shift + 3)) & 1 != 0;
            if edge_low {
                enqueue_gpio_event(pin, false);
            }
            if edge_high {
                enqueue_gpio_event(pin, true);
            }
        }
        // Clear by writing 1s to the raw interrupt register
        p.IO_BANK0.intr(reg_idx).write(|w| unsafe { w.bits(ints) });
    }
}

// ── Event queue (ISR-safe ring buffer) ───────────────────────────────────────

#[derive(Clone, Copy)]
pub struct GpioEvent {
    pub pin: u8,
    pub rising: bool,
}

const GPIO_QUEUE_SIZE: usize = 16;
static mut GPIO_QUEUE: [GpioEvent; GPIO_QUEUE_SIZE] = [GpioEvent {
    pin: 0,
    rising: false,
}; GPIO_QUEUE_SIZE];
static mut GPIO_QUEUE_HEAD: usize = 0;
static mut GPIO_QUEUE_TAIL: usize = 0;

fn enqueue_gpio_event(pin: u8, rising: bool) {
    unsafe {
        let next = (GPIO_QUEUE_HEAD + 1) % GPIO_QUEUE_SIZE;
        if next != GPIO_QUEUE_TAIL {
            GPIO_QUEUE[GPIO_QUEUE_HEAD] = GpioEvent { pin, rising };
            GPIO_QUEUE_HEAD = next;
            // Wake any task blocked in `wait_for_button_event()`. Latches if
            // nothing is currently waiting (binary semaphore).
            if let Some(sem) = (*BUTTON_WAKE_SEM.0.get()).as_ref() {
                let mut ctx = InterruptContext::new();
                sem.give_from_isr(&mut ctx);
            }
        }
    }
}

pub fn drain_gpio_event() -> Option<GpioEvent> {
    unsafe {
        if GPIO_QUEUE_TAIL == GPIO_QUEUE_HEAD {
            return None;
        }
        let ev = GPIO_QUEUE[GPIO_QUEUE_TAIL];
        GPIO_QUEUE_TAIL = (GPIO_QUEUE_TAIL + 1) % GPIO_QUEUE_SIZE;
        Some(ev)
    }
}

pub fn has_pending_event() -> bool {
    unsafe { GPIO_QUEUE_TAIL != GPIO_QUEUE_HEAD }
}

// ── Wake semaphore (signalled from IO_IRQ_BANK0 ISR) ─────────────────────────

struct SemCell(UnsafeCell<Option<Semaphore>>);
unsafe impl Sync for SemCell {}

static BUTTON_WAKE_SEM: SemCell = SemCell(UnsafeCell::new(None));

/// Block the calling task until the next GPIO edge IRQ enqueues an event.
/// Returns immediately if a signal was latched while the task wasn't waiting.
/// No-op if `init_gpio_irq()` has not been called yet.
pub fn wait_for_button_event() {
    unsafe {
        if let Some(sem) = (*BUTTON_WAKE_SEM.0.get()).as_ref() {
            let _ = sem.take(Duration::infinite());
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[cfg(feature = "chip-rp2350")]
fn ensure_io_unreset(p: &rp235x_hal::pac::Peripherals) {
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}
}

#[cfg(feature = "chip-rp2040")]
fn ensure_io_unreset(p: &rp_pico::hal::pac::Peripherals) {
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}
}
