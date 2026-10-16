// SPDX-License-Identifier: GPL-3.0-only
//! The two things every driver here does to reach the hardware, written
//! once: take the peripheral handles, and enable an interrupt at a priority
//! the kernel knows about.
//!
//! The drivers are free functions called from any task and from interrupt
//! handlers, so none of them can own its peripheral; each takes the handles
//! afresh. What keeps that sound is per driver, not per call: a register
//! block is touched by one driver, and a driver that shares a register
//! between task and interrupt context says how at the access.

#![deny(clippy::undocumented_unsafe_blocks, unsafe_op_in_unsafe_fn)]

#[cfg(feature = "chip-rp2350")]
pub(crate) use rp235x_hal::pac;
#[cfg(feature = "chip-rp2040")]
pub(crate) use rp_pico::hal::pac;

/// The chip's peripherals. The handles are zero-sized names for fixed
/// addresses: taking them reads and writes nothing but the PAC's
/// "peripherals taken" flag, which only `Peripherals::take()` at boot
/// consults.
#[inline(always)]
pub(crate) fn steal() -> pac::Peripherals {
    // SAFETY: the handles alias the ones every other driver holds. Register
    // cells are volatile and `Sync`-free; exclusion between contexts is the
    // drivers' business (see the module docs), not the handles'.
    unsafe { pac::Peripherals::steal() }
}

/// Give `irq` priority 0x10 and unmask it on the calling core.
///
/// 0x10 is `configMAX_SYSCALL_INTERRUPT_PRIORITY`: the kernel's critical
/// sections mask the interrupt, so its handler may call `FromISR`
/// functions. The NVIC is banked per core; this configures the caller's.
///
/// The priority goes through `NVIC::set_priority`, which stores the byte
/// on ARMv7-M and ARMv8-M and read-modify-writes the containing IPR word
/// on ARMv6-M, the only access that architecture defines for those
/// registers. The read-modify-write runs with interrupts masked so a task
/// switch cannot land between the read and the write and lose a
/// neighbouring interrupt's byte.
#[inline(always)]
pub(crate) fn enable_irq(irq: pac::Interrupt) {
    cortex_m::interrupt::free(|_| {
        // SAFETY: the NVIC handle aliases nothing Rust owns: no code takes
        // `cortex_m::Peripherals`, and the handle is used for one priority
        // store under the interrupt mask. The priority is the one the
        // kernel's BASEPRI critical sections (RP2350) mask up to, so a
        // handler at it cannot run inside one; the RP2040 port masks with
        // PRIMASK and no priority-based section exists. Unmasking is sound
        // once the handler's statics are in place, which every caller
        // arranges before it calls this.
        unsafe {
            let mut nvic = cortex_m::Peripherals::steal().NVIC;
            nvic.set_priority(irq, 0x10);
            cortex_m::peripheral::NVIC::unmask(irq);
        }
    });
}

/// Pin-mask writes to the single-cycle IO block: drive the pins in `mask`
/// high or low, or turn their output drivers on or off. Each register is a
/// write-only alias that changes exactly the pins whose bits are set and
/// has no other effect — no FIFO, no interrupt, no DMA request.
///
/// They are safe for the reason the PAC's own `funcsel().sio()` is: a pad
/// level is not memory. SIO drives only pads whose function select is SIO,
/// and none of those carries anything Rust's memory depends on: the flash
/// sits on the QSPI pads, and the PSRAM's chip select is a bank-0 pad set
/// to XIP_CS1, which SIO does not drive.
/// Which pads an app may drive is a policy question PeripheralManager
/// answers (it refuses the board's own pads); a wrong level there is a
/// misbehaving peripheral, not undefined behaviour. The PAC types these
/// registers as raw `bits` only because its generator gives every
/// whole-register write that shape; on the RP2040 bits 30 and 31 name no
/// pad and are ignored.
#[inline(always)]
pub(crate) fn sio_out_set(sio: &pac::SIO, mask: u32) {
    // SAFETY: GPIO_OUT_SET only raises the output level of the pads in
    // `mask` (see above); any `u32` is a valid value.
    sio.gpio_out_set().write(|w| unsafe { w.bits(mask) });
}

/// Drive the pins in `mask` low. See [`sio_out_set`].
#[inline(always)]
pub(crate) fn sio_out_clr(sio: &pac::SIO, mask: u32) {
    // SAFETY: GPIO_OUT_CLR only lowers the output level of the pads in
    // `mask` (see `sio_out_set`); any `u32` is a valid value.
    sio.gpio_out_clr().write(|w| unsafe { w.bits(mask) });
}

/// Turn on the output drivers of the pins in `mask`. See [`sio_out_set`].
#[inline(always)]
pub(crate) fn sio_oe_set(sio: &pac::SIO, mask: u32) {
    // SAFETY: GPIO_OE_SET only enables the output drivers of the pads in
    // `mask` (see `sio_out_set`); any `u32` is a valid value.
    sio.gpio_oe_set().write(|w| unsafe { w.bits(mask) });
}

/// Turn off the output drivers of the pins in `mask`. See [`sio_out_set`].
#[inline(always)]
pub(crate) fn sio_oe_clr(sio: &pac::SIO, mask: u32) {
    // SAFETY: GPIO_OE_CLR only disables the output drivers of the pads in
    // `mask` (see `sio_out_set`); any `u32` is a valid value.
    sio.gpio_oe_clr().write(|w| unsafe { w.bits(mask) });
}
