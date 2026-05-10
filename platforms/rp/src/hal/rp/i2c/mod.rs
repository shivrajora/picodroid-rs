// SPDX-License-Identifier: GPL-3.0-only
//! RP2040 / RP2350 I2C HAL.
//!
//! Driver design (matches the rp235x-hal `non_blocking` controller):
//!
//! 1. The task pushes bytes (or read commands) into the TX FIFO synchronously
//!    until either the FIFO is full or all bytes have been queued.
//! 2. To wait for the next progress condition (FIFO room, an RX byte, an
//!    empty FIFO, a STOP) the task arms exactly the relevant interrupts in
//!    `IC_INTR_MASK` and blocks on a per-bus binary "wake" semaphore.
//! 3. The ISR's only job is to write `IC_INTR_MASK = 0` (which de-asserts the
//!    peripheral IRQ line) and `give_from_isr` the wake semaphore. It does
//!    not touch transfer state, does not clear individual interrupt flags,
//!    and never decides whether the transfer succeeded.
//! 4. After the task wakes it polls `IC_RAW_INTR_STAT` to see what actually
//!    happened. If it was an abort, `IC_TX_ABRT_SOURCE` is read+cleared and
//!    the transfer fails; otherwise the wait loop re-checks the condition.
//!
//! This avoids the level-vs-edge race that broke the previous bespoke
//! IRQ-driven flow (where a state machine in the ISR depended on
//! re-pending after an in-ISR mask change). The CPU never busy-spins on
//! peripheral state — every wait point yields to FreeRTOS via the wake
//! semaphore.
pub mod protocol;

use core::cell::UnsafeCell;

use freertos_rust::{Duration, InterruptContext, Semaphore};
use pico_jvm::array_heap::ArrayHeap;

use protocol::{
    fs_spklen, ic_con_for_speed, scl_counts, sda_tx_hold_count, FIFO_DEPTH, IC_DATA_CMD_READ,
    IC_DATA_CMD_STOP, INTR_RX_FULL, INTR_STOP_DET, INTR_TX_ABRT, INTR_TX_EMPTY, MAX_XFER_LEN,
};

use super::clock::PCLK_HZ;

// ── Per-peripheral statics ───────────────────────────────────────────────────

struct SemCell(UnsafeCell<Option<Semaphore>>);
unsafe impl Sync for SemCell {}

static I2C0_LOCK: SemCell = SemCell(UnsafeCell::new(None));
static I2C1_LOCK: SemCell = SemCell(UnsafeCell::new(None));

static I2C0_WAKE: SemCell = SemCell(UnsafeCell::new(None));
static I2C1_WAKE: SemCell = SemCell(UnsafeCell::new(None));

fn i2c_lock(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*I2C0_LOCK.0.get()).as_ref(),
            _ => (*I2C1_LOCK.0.get()).as_ref(),
        }
        .expect("I2C lock semaphore not initialised")
    }
}

fn i2c_wake(id: u8) -> &'static Semaphore {
    unsafe {
        match id {
            0 => (*I2C0_WAKE.0.get()).as_ref(),
            _ => (*I2C1_WAKE.0.get()).as_ref(),
        }
        .expect("I2C wake semaphore not initialised")
    }
}

// ── ISR ──────────────────────────────────────────────────────────────────────

/// One-line ISR body shared by both bus IRQs. Mask all interrupts on the bus
/// (which de-asserts the peripheral's IRQ line — pending-bit consumption is
/// implicit because the level signal is gone) and signal the waiter.
macro_rules! i2c_isr_body {
    ($i2c:expr, $wake_static:expr) => {{
        $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });
        let mut ctx = InterruptContext::new();
        if let Some(sem) = unsafe { (*$wake_static.0.get()).as_ref() } {
            sem.give_from_isr(&mut ctx);
        }
    }};
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn I2C0_IRQ() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    i2c_isr_body!(&p.I2C0, I2C0_WAKE);
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn I2C1_IRQ() {
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };
    i2c_isr_body!(&p.I2C1, I2C1_WAKE);
}

// ── Speed configuration ─────────────────────────────────────────────────────

macro_rules! apply_speed {
    ($i2c:expr, $speed_hz:expr) => {{
        // Disable controller before reconfiguring (poll IC_ENABLE_STATUS to
        // confirm the FSM has actually parked — IC_ENABLE.disable is async).
        $i2c.ic_enable().write(|w| unsafe { w.bits(0) });
        while $i2c.ic_enable_status().read().ic_en().bit_is_set() {}

        // IC_CON: master mode, fast speed, restart enabled, slave disabled,
        // tx_empty_ctrl on, rx-FIFO clock stretching on.
        let con = ic_con_for_speed($speed_hz);
        $i2c.ic_con().write(|w| unsafe { w.bits(con) });

        // SCL counts — program both SS and FS registers so set_speed() can
        // switch between modes without re-init.
        let (hcnt, lcnt) = scl_counts(PCLK_HZ, $speed_hz);
        $i2c.ic_ss_scl_hcnt()
            .write(|w| unsafe { w.ic_ss_scl_hcnt().bits(hcnt) });
        $i2c.ic_ss_scl_lcnt()
            .write(|w| unsafe { w.ic_ss_scl_lcnt().bits(lcnt) });
        $i2c.ic_fs_scl_hcnt()
            .write(|w| unsafe { w.ic_fs_scl_hcnt().bits(hcnt) });
        $i2c.ic_fs_scl_lcnt()
            .write(|w| unsafe { w.ic_fs_scl_lcnt().bits(lcnt) });

        // SDA hold time: the DW IP resets IC_SDA_HOLD to 1 cycle (~7 ns at
        // 150 MHz); without programming this every byte NACKs because slaves
        // can't latch our edges. pico-sdk's `i2c_init` formula.
        $i2c.ic_sda_hold().write(|w| unsafe {
            w.ic_sda_tx_hold()
                .bits(sda_tx_hold_count(PCLK_HZ, $speed_hz))
        });

        // Fast-mode spike-suppression length (reset value 7 is too short).
        $i2c.ic_fs_spklen()
            .write(|w| unsafe { w.ic_fs_spklen().bits(fs_spklen(lcnt)) });

        // Default thresholds — the wait helpers re-program these per-step.
        $i2c.ic_tx_tl().write(|w| unsafe { w.tx_tl().bits(0) });
        $i2c.ic_rx_tl().write(|w| unsafe { w.rx_tl().bits(0) });

        // Mask all interrupts — they are armed only inside the wait helpers.
        $i2c.ic_intr_mask().write(|w| unsafe { w.bits(0) });

        // Re-enable.
        $i2c.ic_enable().write(|w| unsafe { w.bits(1) });
    }};
}

fn reconfigure(i2c_id: u8, speed_hz: u32) {
    #[cfg(feature = "chip-rp2350")]
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

fn is_initialised(i2c_id: u8) -> bool {
    unsafe {
        match i2c_id {
            0 => (*I2C0_LOCK.0.get()).is_some(),
            _ => (*I2C1_LOCK.0.get()).is_some(),
        }
    }
}

/// Configure GPIO pins for I2C function and start the controller at 100 kHz.
/// Idempotent: subsequent calls for the same `i2c_id` return immediately.
pub fn init(i2c_id: u8) {
    if is_initialised(i2c_id) {
        return;
    }

    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let p = unsafe { pac::Peripherals::steal() };

    // Ensure IO_BANK0 and PADS_BANK0 are out of reset (idempotent).
    p.RESETS
        .reset()
        .modify(|_, w| w.io_bank0().clear_bit().pads_bank0().clear_bit());
    while p.RESETS.reset_done().read().io_bank0().bit_is_clear() {}
    while p.RESETS.reset_done().read().pads_bank0().bit_is_clear() {}

    // Release the appropriate I2C block from reset.
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

    // Default funcsel-3 pin assignments (boards using non-default pads aren't
    // supported here yet; add an `init_with_pins` if/when a board needs it).
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
                                                        // I2C is open-drain. PADS_BANK0 reset value has PDE=1; using
                                                        // `.write()` (not `.modify()`) sticks every unset field at
                                                        // reset, so PDE silently stays enabled and the bus settles
                                                        // mid-rail. Explicitly clear PDE here.
        p.PADS_BANK0.gpio(pin).write(|w| {
            #[cfg(feature = "chip-rp2350")]
            let w = w.iso().clear_bit();
            w.ie()
                .set_bit()
                .od()
                .clear_bit()
                .pue()
                .set_bit()
                .pde()
                .clear_bit()
                .schmitt()
                .set_bit()
        });
    }

    // Apply default configuration: 100 kHz, fast-mode timing.
    reconfigure(i2c_id, 100_000);

    // Per-bus mutex (binary semaphore released → unlocked).
    let lock = Semaphore::new_binary().expect("i2c lock sem alloc");
    lock.give();
    // Per-bus ISR-to-task wake (binary semaphore released → none pending).
    let wake = Semaphore::new_binary().expect("i2c wake sem alloc");
    match i2c_id {
        0 => unsafe {
            *I2C0_LOCK.0.get() = Some(lock);
            *I2C0_WAKE.0.get() = Some(wake);
        },
        _ => unsafe {
            *I2C1_LOCK.0.get() = Some(lock);
            *I2C1_WAKE.0.get() = Some(wake);
        },
    }

    // Unmask the IRQ in the NVIC at a kernel-aware priority (numerically equal
    // to configMAX_SYSCALL_INTERRUPT_PRIORITY = 0x10, so it is masked during
    // FreeRTOS critical sections and may safely call `_from_isr` APIs).
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

// ── IRQ-driven transfer primitives ───────────────────────────────────────────

/// Outcome of the wait helpers below.
enum WaitResult {
    /// The level condition was reached.
    Ready,
    /// The slave NACK'd / the controller raised TX_ABRT. The abort source has
    /// already been read+cleared.
    Abort,
    /// We waited longer than `timeout_ms` without ISR or condition firing —
    /// suggests a controller hang. Caller should propagate as an error.
    Timeout,
}

/// Worst-case wait per step. At 100 kHz a 16-byte FIFO drain is ~1.6 ms; we
/// pick a generous bound so a stuck slave or clock-stretch glitch surfaces as
/// `Timeout` instead of hanging the calling task forever.
const WAIT_TIMEOUT_MS: u32 = 200;

/// Read+clear `IC_TX_ABRT_SOURCE`. Returns `true` if the controller reported
/// an abort since the last call.
fn check_abort<I: I2cPeriph>(i2c: &I) -> bool {
    let abrt = i2c.read_abrt_source();
    if abrt != 0 {
        i2c.clear_tx_abrt();
    }
    abrt != 0
}

/// Trait used so the wait/transfer helpers are generic over `pac::I2C0` and
/// `pac::I2C1` without resorting to PAC-typed macros.
trait I2cPeriph {
    fn ic_intr_mask_write(&self, mask: u32);
    fn ic_tx_tl_write(&self, tl: u8);
    fn ic_rx_tl_write(&self, tl: u8);
    fn ic_data_cmd_write(&self, cmd: u32);
    fn ic_data_cmd_read_byte(&self) -> u8;
    fn ic_enable_write(&self, on: bool);
    fn ic_enable_status_busy(&self) -> bool;
    fn ic_tar_write(&self, addr: u16);
    fn raw_intr_stat(&self) -> u32;
    fn read_abrt_source(&self) -> u32;
    fn clear_tx_abrt(&self);
    fn clear_stop_det(&self);
    fn clear_intr(&self);
    fn tx_fifo_used(&self) -> u8;
    fn rx_fifo_used(&self) -> u8;
    fn mst_active(&self) -> bool;
}

macro_rules! impl_i2c_periph {
    ($t:ty) => {
        impl I2cPeriph for $t {
            #[inline]
            fn ic_intr_mask_write(&self, mask: u32) {
                self.ic_intr_mask().write(|w| unsafe { w.bits(mask) });
            }
            #[inline]
            fn ic_tx_tl_write(&self, tl: u8) {
                self.ic_tx_tl().write(|w| unsafe { w.tx_tl().bits(tl) });
            }
            #[inline]
            fn ic_rx_tl_write(&self, tl: u8) {
                self.ic_rx_tl().write(|w| unsafe { w.rx_tl().bits(tl) });
            }
            #[inline]
            fn ic_data_cmd_write(&self, cmd: u32) {
                self.ic_data_cmd().write(|w| unsafe { w.bits(cmd) });
            }
            #[inline]
            fn ic_data_cmd_read_byte(&self) -> u8 {
                self.ic_data_cmd().read().dat().bits()
            }
            #[inline]
            fn ic_enable_write(&self, on: bool) {
                self.ic_enable().write(|w| unsafe { w.bits(on as u32) });
            }
            #[inline]
            fn ic_enable_status_busy(&self) -> bool {
                self.ic_enable_status().read().ic_en().bit_is_set()
            }
            #[inline]
            fn ic_tar_write(&self, addr: u16) {
                self.ic_tar().write(|w| unsafe { w.ic_tar().bits(addr) });
            }
            #[inline]
            fn raw_intr_stat(&self) -> u32 {
                self.ic_raw_intr_stat().read().bits()
            }
            #[inline]
            fn read_abrt_source(&self) -> u32 {
                self.ic_tx_abrt_source().read().bits()
            }
            #[inline]
            fn clear_tx_abrt(&self) {
                let _ = self.ic_clr_tx_abrt().read();
            }
            #[inline]
            fn clear_stop_det(&self) {
                let _ = self.ic_clr_stop_det().read();
            }
            #[inline]
            fn clear_intr(&self) {
                let _ = self.ic_clr_intr().read();
            }
            #[inline]
            fn tx_fifo_used(&self) -> u8 {
                self.ic_txflr().read().txflr().bits()
            }
            #[inline]
            fn rx_fifo_used(&self) -> u8 {
                self.ic_rxflr().read().rxflr().bits()
            }
            #[inline]
            fn mst_active(&self) -> bool {
                self.ic_status().read().mst_activity().bit_is_set()
            }
        }
    };
}

#[cfg(feature = "chip-rp2350")]
impl_i2c_periph!(rp235x_hal::pac::I2C0);
#[cfg(feature = "chip-rp2350")]
impl_i2c_periph!(rp235x_hal::pac::I2C1);
#[cfg(feature = "chip-rp2040")]
impl_i2c_periph!(rp_pico::hal::pac::I2C0);
#[cfg(feature = "chip-rp2040")]
impl_i2c_periph!(rp_pico::hal::pac::I2C1);

/// Generic level-triggered wait loop:
///   1. Drain any stale wake.
///   2. Synchronously poll the condition once. If it's already true, return.
///   3. Set FIFO thresholds.
///   4. Arm `IC_INTR_MASK` with the requested wake mask plus TX_ABRT (so an
///      abort short-circuits any wait).
///   5. Block on the wake semaphore. The ISR will clear the mask and signal
///      the semaphore on the first interrupt.
///   6. Loop: re-check abort and the condition, re-arm if neither is set yet.
///
/// The peripheral IRQ line is level-driven — if the condition becomes true
/// between step 2 and step 4, step 4's mask write triggers the ISR
/// immediately, so step 5 returns without sleeping.
fn wait_for<I, F>(
    i2c_id: u8,
    i2c: &I,
    mask: u32,
    tx_tl: Option<u8>,
    rx_tl: Option<u8>,
    mut ready: F,
) -> WaitResult
where
    I: I2cPeriph,
    F: FnMut(&I) -> bool,
{
    let wake = i2c_wake(i2c_id);
    if let Some(tl) = tx_tl {
        i2c.ic_tx_tl_write(tl);
    }
    if let Some(tl) = rx_tl {
        i2c.ic_rx_tl_write(tl);
    }
    loop {
        if check_abort(i2c) {
            i2c.ic_intr_mask_write(0);
            return WaitResult::Abort;
        }
        if ready(i2c) {
            i2c.ic_intr_mask_write(0);
            return WaitResult::Ready;
        }
        // Drain any wake left over from a prior step.
        let _ = wake.take(Duration::zero());
        // Arm the mask. If `ready` becomes true between the check above and
        // here, the level signal is already high and the ISR fires
        // immediately — that's fine, the wake then returns instantly.
        i2c.ic_intr_mask_write(mask | INTR_TX_ABRT);
        if wake.take(Duration::ms(WAIT_TIMEOUT_MS)).is_err() {
            i2c.ic_intr_mask_write(0);
            return WaitResult::Timeout;
        }
    }
}

#[inline]
fn tx_not_full<I: I2cPeriph>(i2c: &I) -> bool {
    i2c.tx_fifo_used() < FIFO_DEPTH
}

#[inline]
fn raw_has(i2c: &impl I2cPeriph, bit: u32) -> bool {
    i2c.raw_intr_stat() & bit != 0
}

fn write_internal<I: I2cPeriph>(i2c_id: u8, i2c: &I, address: u8, data: &[u8]) -> i32 {
    // Set IC_TAR (must be done with the controller disabled).
    i2c.ic_enable_write(false);
    while i2c.ic_enable_status_busy() {}
    i2c.ic_tar_write(address as u16);
    i2c.ic_enable_write(true);

    // Drop any sticky abort/stop bits left from a prior failed transfer so
    // they don't immediately short-circuit the very first wait below.
    i2c.clear_intr();

    let len = data.len();
    if len == 0 {
        // Bare address probe: write an empty data byte with STOP set.
        i2c.ic_data_cmd_write(IC_DATA_CMD_STOP);
        // Wait for the master FSM to release the bus.
        match wait_for(i2c_id, i2c, INTR_STOP_DET, Some(0), Some(0), |i| {
            raw_has(i, INTR_STOP_DET) && !i.mst_active()
        }) {
            WaitResult::Ready => {
                i2c.clear_stop_det();
                0
            }
            WaitResult::Abort => -1,
            WaitResult::Timeout => -1,
        }
    } else {
        for (i, &byte) in data.iter().enumerate() {
            // Wait for FIFO room. tx_tl=FIFO_DEPTH-1 → TX_EMPTY fires when
            // FIFO has at least 1 free slot.
            if !tx_not_full(i2c) {
                match wait_for(
                    i2c_id,
                    i2c,
                    INTR_TX_EMPTY,
                    Some(FIFO_DEPTH - 1),
                    None,
                    tx_not_full,
                ) {
                    WaitResult::Ready => {}
                    WaitResult::Abort | WaitResult::Timeout => return -1,
                }
            }
            let last = i == len - 1;
            let cmd = byte as u32 | if last { IC_DATA_CMD_STOP } else { 0 };
            i2c.ic_data_cmd_write(cmd);
        }

        // Wait for the FIFO to drain (all bytes shifted out) and then for
        // the STOP condition to land on the bus.
        match wait_for(i2c_id, i2c, INTR_STOP_DET, Some(0), None, |i| {
            raw_has(i, INTR_STOP_DET) && !i.mst_active()
        }) {
            WaitResult::Ready => {
                i2c.clear_stop_det();
                len as i32
            }
            WaitResult::Abort | WaitResult::Timeout => -1,
        }
    }
}

fn read_internal<I: I2cPeriph>(i2c_id: u8, i2c: &I, address: u8, buf: &mut [u8]) -> i32 {
    i2c.ic_enable_write(false);
    while i2c.ic_enable_status_busy() {}
    i2c.ic_tar_write(address as u16);
    i2c.ic_enable_write(true);
    i2c.clear_intr();

    let len = buf.len();
    let last_idx = len - 1;
    let mut tx_idx = 0usize; // Next read-cmd to push.
    let mut rx_idx = 0usize; // Next byte to receive.

    while rx_idx < len {
        // Push as many read commands as the FIFO will accept.
        while tx_idx < len && tx_not_full(i2c) {
            let cmd = IC_DATA_CMD_READ
                | if tx_idx == last_idx {
                    IC_DATA_CMD_STOP
                } else {
                    0
                };
            i2c.ic_data_cmd_write(cmd);
            tx_idx += 1;
        }

        // Wait for at least one received byte (or an abort).
        match wait_for(i2c_id, i2c, INTR_RX_FULL, None, Some(0), |i| {
            i.rx_fifo_used() > 0
        }) {
            WaitResult::Ready => {}
            WaitResult::Abort | WaitResult::Timeout => return -1,
        }

        // Drain whatever the controller has handed us.
        while rx_idx < len && i2c.rx_fifo_used() > 0 {
            buf[rx_idx] = i2c.ic_data_cmd_read_byte();
            rx_idx += 1;
        }
    }

    // Final wait for STOP so we leave the bus in a clean state.
    match wait_for(i2c_id, i2c, INTR_STOP_DET, None, None, |i| {
        raw_has(i, INTR_STOP_DET) && !i.mst_active()
    }) {
        WaitResult::Ready => {
            i2c.clear_stop_det();
            rx_idx as i32
        }
        WaitResult::Abort | WaitResult::Timeout => -1,
    }
}

// ── Public byte-slice API ────────────────────────────────────────────────────

/// IRQ-driven write of a byte slice. Returns `len` on success, `-1` on
/// NACK/abort/timeout. Blocks the calling FreeRTOS task on a semaphore at
/// every wait point — never busy-spins on peripheral state.
pub fn write_slice(i2c_id: u8, address: u8, data: &[u8]) -> i32 {
    if data.len() > MAX_XFER_LEN {
        return -1;
    }
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let lock = i2c_lock(i2c_id);
    let _ = lock.take(Duration::infinite());
    let p = unsafe { pac::Peripherals::steal() };
    let result = match i2c_id {
        0 => write_internal(i2c_id, &p.I2C0, address, data),
        _ => write_internal(i2c_id, &p.I2C1, address, data),
    };
    lock.give();
    result
}

/// IRQ-driven read into a byte slice. Returns `len` on success, `-1` on
/// NACK/abort/timeout.
pub fn read_slice(i2c_id: u8, address: u8, buf: &mut [u8]) -> i32 {
    if buf.is_empty() {
        return 0;
    }
    if buf.len() > MAX_XFER_LEN {
        return -1;
    }
    #[cfg(feature = "chip-rp2350")]
    use rp235x_hal::pac;
    #[cfg(feature = "chip-rp2040")]
    use rp_pico::hal::pac;
    let lock = i2c_lock(i2c_id);
    let _ = lock.take(Duration::infinite());
    let p = unsafe { pac::Peripherals::steal() };
    let result = match i2c_id {
        0 => read_internal(i2c_id, &p.I2C0, address, buf),
        _ => read_internal(i2c_id, &p.I2C1, address, buf),
    };
    lock.give();
    result
}

// ── Java I2cDevice entrypoints ───────────────────────────────────────────────

/// Java `I2cDevice.write` shim — copies the source array into a stack buffer
/// and reuses `write_slice`. Returns `len` on success, `-1` on NACK/abort.
pub fn write(i2c_id: u8, address: u32, data_idx: u16, len: usize, arrays: &ArrayHeap) -> i32 {
    if len > MAX_XFER_LEN {
        return -1;
    }
    let mut buf = [0u8; MAX_XFER_LEN];
    for (i, slot) in buf.iter_mut().enumerate().take(len) {
        *slot = arrays.load(data_idx, i).unwrap_or(0) as u8;
    }
    write_slice(i2c_id, address as u8, &buf[..len])
}

/// Java `I2cDevice.read` shim — reads into a stack buffer via `read_slice`,
/// then copies into the destination array. Returns `len` on success, `-1` on
/// NACK/abort.
pub fn read(i2c_id: u8, address: u32, buf_idx: u16, len: usize, arrays: &mut ArrayHeap) -> i32 {
    if len == 0 {
        return 0;
    }
    if len > MAX_XFER_LEN {
        return -1;
    }
    let mut buf = [0u8; MAX_XFER_LEN];
    let result = read_slice(i2c_id, address as u8, &mut buf[..len]);
    if result > 0 {
        let n = (result as usize).min(len);
        for (i, &byte) in buf.iter().enumerate().take(n) {
            arrays.store(buf_idx, i, byte as i32);
        }
    }
    result
}
