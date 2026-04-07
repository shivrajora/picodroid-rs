use core::cell::UnsafeCell;

use freertos_rust::{Duration, InterruptContext, Semaphore};
use pico_jvm::array_heap::ArrayHeap;

// CLK_PERI defaults to system clock: 125 MHz on RP2040, 150 MHz on RP2350
#[cfg(feature = "chip-rp2040")]
const PCLK_HZ: u32 = 125_000_000;
#[cfg(feature = "chip-rp2350-hal")]
const PCLK_HZ: u32 = 150_000_000;

// IC_CON bit masks
const IC_CON_MASTER_MODE: u32 = 1 << 0;
const IC_CON_SPEED_STD: u32 = 1 << 1; // SPEED=01 (standard, 100 kHz)
const IC_CON_SPEED_FAST: u32 = 1 << 2; // SPEED=10 (fast, 400 kHz)
const IC_CON_RESTART_EN: u32 = 1 << 5;
const IC_CON_SLAVE_DISABLE: u32 = 1 << 6;

// IC_DATA_CMD bit masks
const IC_DATA_CMD_READ: u32 = 1 << 8;
const IC_DATA_CMD_STOP: u32 = 1 << 9;

// IC_INTR_MASK bit positions
const INTR_RX_FULL: u32 = 1 << 2;
const INTR_TX_EMPTY: u32 = 1 << 4;
const INTR_TX_ABRT: u32 = 1 << 6;
const INTR_STOP_DET: u32 = 1 << 9;

// Maximum bytes per interrupt-driven I2C transaction.
const MAX_XFER_LEN: usize = 64;

// ── Transfer state shared between task and ISR ───────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum I2cOp {
    Idle,
    Write,
    Read,
}

struct I2cXferState {
    op: I2cOp,
    buf: [u8; MAX_XFER_LEN],
    len: usize,
    tx_idx: usize, // write: next byte to push; read: next read cmd to issue
    rx_idx: usize, // read: next byte to store
    result: i32,   // set by ISR: len on success, -1 on abort
}

impl I2cXferState {
    const fn new() -> Self {
        Self {
            op: I2cOp::Idle,
            buf: [0; MAX_XFER_LEN],
            len: 0,
            tx_idx: 0,
            rx_idx: 0,
            result: 0,
        }
    }
}

// ── Per-peripheral statics ───────────────────────────────────────────────────

struct XferCell(UnsafeCell<I2cXferState>);
unsafe impl Sync for XferCell {}

struct SemCell(UnsafeCell<Option<Semaphore>>);
unsafe impl Sync for SemCell {}

static I2C0_STATE: XferCell = XferCell(UnsafeCell::new(I2cXferState::new()));
static I2C1_STATE: XferCell = XferCell(UnsafeCell::new(I2cXferState::new()));

static I2C0_DONE: SemCell = SemCell(UnsafeCell::new(None));
static I2C1_DONE: SemCell = SemCell(UnsafeCell::new(None));

static I2C0_LOCK: SemCell = SemCell(UnsafeCell::new(None));
static I2C1_LOCK: SemCell = SemCell(UnsafeCell::new(None));

fn i2c_state(id: u8) -> &'static mut I2cXferState {
    unsafe {
        &mut *match id {
            0 => I2C0_STATE.0.get(),
            _ => I2C1_STATE.0.get(),
        }
    }
}

fn i2c_done(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*I2C0_DONE.0.get()).as_ref(),
            _ => (*I2C1_DONE.0.get()).as_ref(),
        }
        .expect("I2C done semaphore not initialised")
    }
}

fn i2c_lock(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*I2C0_LOCK.0.get()).as_ref(),
            _ => (*I2C1_LOCK.0.get()).as_ref(),
        }
        .expect("I2C lock semaphore not initialised")
    }
}

// ── ISR ──────────────────────────────────────────────────────────────────────

macro_rules! i2c_isr_body {
    ($i2c:expr, $state_static:expr, $done_static:expr) => {{
        let state = unsafe { &mut *$state_static.0.get() };
        let mut ctx = InterruptContext::new();

        let stat = $i2c.ic_intr_stat().read();

        // TX_ABRT: abort detected — terminate immediately
        if stat.r_tx_abrt().bit_is_set() {
            let _ = $i2c.ic_clr_tx_abrt().read();
            $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
            state.result = -1;
            state.op = I2cOp::Idle;
            if let Some(sem) = unsafe { (*$done_static.0.get()).as_ref() } {
                sem.give_from_isr(&mut ctx);
            }
            return;
        }

        match state.op {
            I2cOp::Write => {
                // TX_EMPTY: FIFO has space, feed more bytes
                if stat.r_tx_empty().bit_is_set() {
                    while state.tx_idx < state.len {
                        if $i2c.ic_status().read().tfnf().bit_is_clear() {
                            break;
                        }
                        let byte = state.buf[state.tx_idx];
                        let stop = if state.tx_idx == state.len - 1 {
                            IC_DATA_CMD_STOP
                        } else {
                            0
                        };
                        $i2c.ic_data_cmd()
                            .write(|w| unsafe { w.bits(byte as u32 | stop) });
                        state.tx_idx += 1;
                    }
                    if state.tx_idx >= state.len {
                        // All bytes queued; wait for STOP_DET only
                        $i2c.ic_intr_mask()
                            .write(|w| unsafe { w.bits(INTR_STOP_DET | INTR_TX_ABRT) });
                    }
                }
                // STOP_DET: transaction complete
                if stat.r_stop_det().bit_is_set() {
                    let _ = $i2c.ic_clr_stop_det().read();
                    $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
                    if $i2c.ic_raw_intr_stat().read().tx_abrt().bit_is_set() {
                        let _ = $i2c.ic_clr_tx_abrt().read();
                        state.result = -1;
                    } else {
                        state.result = state.len as i32;
                    }
                    state.op = I2cOp::Idle;
                    if let Some(sem) = unsafe { (*$done_static.0.get()).as_ref() } {
                        sem.give_from_isr(&mut ctx);
                    }
                }
            }
            I2cOp::Read => {
                // RX_FULL: data available — drain RX FIFO, issue more read cmds
                if stat.r_rx_full().bit_is_set() {
                    while $i2c.ic_status().read().rfne().bit_is_set() && state.rx_idx < state.len {
                        state.buf[state.rx_idx] = $i2c.ic_data_cmd().read().dat().bits();
                        state.rx_idx += 1;
                    }
                    while state.tx_idx < state.len {
                        if $i2c.ic_status().read().tfnf().bit_is_clear() {
                            break;
                        }
                        let stop = if state.tx_idx == state.len - 1 {
                            IC_DATA_CMD_STOP
                        } else {
                            0
                        };
                        $i2c.ic_data_cmd()
                            .write(|w| unsafe { w.bits(IC_DATA_CMD_READ | stop) });
                        state.tx_idx += 1;
                    }
                }
                // STOP_DET: all done
                if stat.r_stop_det().bit_is_set() {
                    let _ = $i2c.ic_clr_stop_det().read();
                    // Drain any remaining RX bytes
                    while $i2c.ic_status().read().rfne().bit_is_set() && state.rx_idx < state.len {
                        state.buf[state.rx_idx] = $i2c.ic_data_cmd().read().dat().bits();
                        state.rx_idx += 1;
                    }
                    $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
                    if $i2c.ic_raw_intr_stat().read().tx_abrt().bit_is_set() {
                        let _ = $i2c.ic_clr_tx_abrt().read();
                        state.result = -1;
                    } else {
                        state.result = state.rx_idx as i32;
                    }
                    state.op = I2cOp::Idle;
                    if let Some(sem) = unsafe { (*$done_static.0.get()).as_ref() } {
                        sem.give_from_isr(&mut ctx);
                    }
                }
            }
            I2cOp::Idle => {
                // Spurious interrupt; mask everything
                $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
            }
        }
        // ctx drops here → freertos_rs_isr_yield if a higher-priority task woke
    }};
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn I2C0_IRQ() {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    i2c_isr_body!(&p.I2C0, I2C0_STATE, I2C0_DONE);
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn I2C1_IRQ() {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    i2c_isr_body!(&p.I2C1, I2C1_STATE, I2C1_DONE);
}

// ── Speed configuration ─────────────────────────────────────────────────────

fn scl_counts(speed_hz: u32) -> (u16, u16) {
    let period = PCLK_HZ / speed_hz.max(1);
    let lcnt = (period * 3 / 5) as u16;
    let hcnt = (period - lcnt as u32) as u16;
    (hcnt, lcnt)
}

fn ic_con_for_speed(speed_hz: u32) -> u32 {
    let speed_bits = if speed_hz <= 100_000 {
        IC_CON_SPEED_STD
    } else {
        IC_CON_SPEED_FAST
    };
    IC_CON_MASTER_MODE | speed_bits | IC_CON_RESTART_EN | IC_CON_SLAVE_DISABLE
}

macro_rules! apply_speed {
    ($i2c:expr, $speed_hz:expr) => {{
        // Disable controller before reconfiguring
        $i2c.ic_enable().write(|w| unsafe { w.bits(0) });
        while $i2c.ic_enable_status().read().ic_en().bit_is_set() {}

        // IC_CON: master mode, speed, restart enabled, slave disabled
        let con = ic_con_for_speed($speed_hz);
        $i2c.ic_con().write(|w| unsafe { w.bits(con) });

        // SCL counts — program both SS and FS registers so set_speed() can
        // switch between modes without re-init.
        let (hcnt, lcnt) = scl_counts($speed_hz);
        $i2c.ic_ss_scl_hcnt()
            .write(|w| unsafe { w.ic_ss_scl_hcnt().bits(hcnt) });
        $i2c.ic_ss_scl_lcnt()
            .write(|w| unsafe { w.ic_ss_scl_lcnt().bits(lcnt) });
        $i2c.ic_fs_scl_hcnt()
            .write(|w| unsafe { w.ic_fs_scl_hcnt().bits(hcnt) });
        $i2c.ic_fs_scl_lcnt()
            .write(|w| unsafe { w.ic_fs_scl_lcnt().bits(lcnt) });

        // Re-enable
        $i2c.ic_enable().write(|w| unsafe { w.bits(1) });
    }};
}

fn reconfigure(i2c_id: u8, speed_hz: u32) {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    match i2c_id {
        0 => apply_speed!(&p.I2C0, speed_hz),
        _ => apply_speed!(&p.I2C1, speed_hz),
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Configure GPIO pins for I2C function and start the controller at 100 kHz.
pub fn init(i2c_id: u8) {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent)
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release the appropriate I2C block from reset
    match i2c_id {
        0 => {
            p.RESETS.reset().modify(|_, w| w.i2c0().clear_bit());
            while p.RESETS.reset_done().read().i2c0().bit_is_clear() {}
        }
        _ => {
            p.RESETS.reset().modify(|_, w| w.i2c1().clear_bit());
            while p.RESETS.reset_done().read().i2c1().bit_is_clear() {}
        }
    }

    // Route GPIO pins to I2C function (function select 3).
    // Default pin assignments:
    //   I2C0 → SDA=GP4, SCL=GP5
    //   I2C1 → SDA=GP2, SCL=GP3
    let (sda_pin, scl_pin): (usize, usize) = match i2c_id {
        0 => (4, 5),
        _ => (2, 3),
    };
    for pin in [sda_pin, scl_pin] {
        p.IO_BANK0
            .gpio(pin)
            .gpio_ctrl()
            .write(|w| unsafe { w.funcsel().bits(3) }); // 3 = I2C
                                                        // Enable input, pull-up (open-drain bus), Schmitt trigger
        p.PADS_BANK0.gpio(pin).write(|w| {
            #[cfg(feature = "chip-rp2350-hal")]
            let w = w.iso().clear_bit();
            w.ie()
                .set_bit()
                .od()
                .clear_bit()
                .pue()
                .set_bit()
                .schmitt()
                .set_bit()
        });
    }

    // Apply default configuration: 100 kHz standard speed
    reconfigure(i2c_id, 100_000);

    // Allocate completion semaphore (binary, starts empty)
    let done = Semaphore::new_binary().expect("i2c done sem alloc");
    // Allocate lock semaphore (binary, starts given = unlocked)
    let lock = Semaphore::new_binary().expect("i2c lock sem alloc");
    lock.give();

    match i2c_id {
        0 => unsafe {
            *I2C0_DONE.0.get() = Some(done);
            *I2C0_LOCK.0.get() = Some(lock);
        },
        _ => unsafe {
            *I2C1_DONE.0.get() = Some(done);
            *I2C1_LOCK.0.get() = Some(lock);
        },
    }

    // Mask all I2C interrupts (ISR enables them per-transaction)
    macro_rules! setup_irq {
        ($i2c:expr, $irq:expr) => {{
            $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
            unsafe {
                let nvic_ipr = 0xE000_E400 as *mut u8;
                let irqn = $irq as u8;
                nvic_ipr.add(irqn as usize).write_volatile(0x10);
                cortex_m::peripheral::NVIC::unmask($irq);
            }
        }};
    }
    match i2c_id {
        0 => setup_irq!(&p.I2C0, pac::Interrupt::I2C0_IRQ),
        _ => setup_irq!(&p.I2C1, pac::Interrupt::I2C1_IRQ),
    }
}

pub fn set_speed(i2c_id: u8, hz: u32) {
    let lock = i2c_lock(i2c_id);
    let _ = lock.take(Duration::infinite());
    reconfigure(i2c_id, hz);
    lock.give();
}

/// Interrupt-driven write. Returns len on success, -1 on NACK/abort.
pub fn write(i2c_id: u8, address: u32, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    let lock = i2c_lock(i2c_id);
    let _ = lock.take(Duration::infinite());

    let p = unsafe { pac::Peripherals::steal() };

    macro_rules! do_write {
        ($i2c:expr) => {{
            $i2c.ic_tar()
                .write(|w| unsafe { w.ic_tar().bits(address as u16) });

            if len == 0 {
                // Zero-byte probe: single address+STOP, not worth ISR overhead
                while $i2c.ic_status().read().tfnf().bit_is_clear() {}
                $i2c.ic_data_cmd()
                    .write(|w| unsafe { w.bits(IC_DATA_CMD_STOP) });
                while $i2c.ic_status().read().tfe().bit_is_clear() {}
                while $i2c.ic_status().read().mst_activity().bit_is_set() {}
                let result = if $i2c.ic_raw_intr_stat().read().tx_abrt().bit_is_set() {
                    let _ = $i2c.ic_clr_tx_abrt().read();
                    -1i32
                } else {
                    0i32
                };
                lock.give();
                return result;
            }

            if len > MAX_XFER_LEN {
                lock.give();
                return -1;
            }

            let state = i2c_state(i2c_id);
            for i in 0..len {
                state.buf[i] = arrays.load(data_idx, i).unwrap_or(0) as u8;
            }
            state.op = I2cOp::Write;
            state.len = len;
            state.tx_idx = 0;
            state.rx_idx = 0;
            state.result = 0;

            // TX FIFO threshold = 0 (TX_EMPTY fires when FIFO is empty)
            $i2c.ic_tx_tl().write(|w| unsafe { w.bits(0) });
            let _ = $i2c.ic_clr_intr().read();
            $i2c.ic_intr_mask()
                .write(|w| unsafe { w.bits(INTR_TX_EMPTY | INTR_TX_ABRT | INTR_STOP_DET) });
        }};
    }

    match i2c_id {
        0 => do_write!(&p.I2C0),
        _ => do_write!(&p.I2C1),
    }

    let done = i2c_done(i2c_id);
    if done.take(Duration::ms(1000)).is_err() {
        // Timeout: mask all interrupts and abort
        match i2c_id {
            0 => p.I2C0.ic_intr_mask().write(|w| unsafe { w.bits(0) }),
            _ => p.I2C1.ic_intr_mask().write(|w| unsafe { w.bits(0) }),
        }
        let state = i2c_state(i2c_id);
        state.op = I2cOp::Idle;
        state.result = -1;
    }

    let result = i2c_state(i2c_id).result;
    lock.give();
    result
}

/// Interrupt-driven read. Returns len on success, -1 on NACK/abort.
pub fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    #[cfg(feature = "chip-rp2350-hal")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;

    if len == 0 {
        return 0;
    }

    let lock = i2c_lock(i2c_id);
    let _ = lock.take(Duration::infinite());

    if len > MAX_XFER_LEN {
        lock.give();
        return -1;
    }

    let p = unsafe { pac::Peripherals::steal() };

    macro_rules! do_read {
        ($i2c:expr) => {{
            $i2c.ic_tar()
                .write(|w| unsafe { w.ic_tar().bits(address as u16) });

            let state = i2c_state(i2c_id);
            state.op = I2cOp::Read;
            state.len = len;
            state.tx_idx = 0;
            state.rx_idx = 0;
            state.result = 0;

            // RX FIFO threshold = 0 (RX_FULL fires when ≥1 byte available)
            $i2c.ic_rx_tl().write(|w| unsafe { w.bits(0) });

            // Seed TX FIFO with read commands (up to FIFO depth = 16)
            let seed = len.min(16);
            for i in 0..seed {
                let stop = if i == len - 1 { IC_DATA_CMD_STOP } else { 0 };
                $i2c.ic_data_cmd()
                    .write(|w| unsafe { w.bits(IC_DATA_CMD_READ | stop) });
            }
            state.tx_idx = seed;

            let _ = $i2c.ic_clr_intr().read();
            $i2c.ic_intr_mask()
                .write(|w| unsafe { w.bits(INTR_RX_FULL | INTR_TX_ABRT | INTR_STOP_DET) });
        }};
    }

    match i2c_id {
        0 => do_read!(&p.I2C0),
        _ => do_read!(&p.I2C1),
    }

    let done = i2c_done(i2c_id);
    if done.take(Duration::ms(1000)).is_err() {
        match i2c_id {
            0 => p.I2C0.ic_intr_mask().write(|w| unsafe { w.bits(0) }),
            _ => p.I2C1.ic_intr_mask().write(|w| unsafe { w.bits(0) }),
        }
        let state = i2c_state(i2c_id);
        state.op = I2cOp::Idle;
        state.result = -1;
    }

    let state = i2c_state(i2c_id);
    let result = state.result;
    if result > 0 {
        for i in 0..(result as usize).min(len) {
            arrays.store(buf_idx, i, state.buf[i] as i32);
        }
    }

    lock.give();
    result
}
