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
#[inline(always)]
pub(crate) fn enable_irq(irq: pac::Interrupt) {
    // SAFETY: NVIC_IPR is a fixed system register and each interrupt has
    // its own byte in it; unmasking is sound once the handler's statics are
    // in place, which every caller arranges before it calls this.
    unsafe {
        let nvic_ipr = 0xE000_E400 as *mut u8;
        let irqn = irq as u8;
        nvic_ipr.add(irqn as usize).write_volatile(0x10);
        cortex_m::peripheral::NVIC::unmask(irq);
    }
}
