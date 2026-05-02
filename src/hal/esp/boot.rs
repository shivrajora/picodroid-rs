// SPDX-License-Identifier: GPL-3.0-only
//! ESP32-S3 boot — Milestone 1 stub.
//!
//! clock_init is a no-op; real ClockControl::max() wiring is Milestone 2.
//! start_tasks bypasses FreeRTOS entirely (Milestone 3 adds Xtensa RTOS).
//!
//! # defmt transport
//! defmt-rtt (used on RP) is ARM-only. We provide a no-op global logger here
//! so defmt symbols link. Real defmt-over-JTAG lands in a later milestone.

// No-op defmt global logger so defmt symbols link on Xtensa.
#[defmt::global_logger]
struct EspDefmtLogger;

unsafe impl defmt::Logger for EspDefmtLogger {
    fn acquire() {}
    unsafe fn flush() {}
    unsafe fn release() {}
    unsafe fn write(_bytes: &[u8]) {}
}

defmt::timestamp!("{=u64}", 0u64);

pub fn clock_init() {}

pub fn start_tasks(_boot_apk: &'static [u8]) -> ! {
    crate::app::run_jvm();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    #[allow(clippy::empty_loop)]
    loop {}
}
