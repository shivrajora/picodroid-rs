// SPDX-License-Identifier: GPL-3.0-only
//! ESP32-S3 boot — Milestone 1 stub.
//!
//! clock_init is a no-op; real ClockControl::max() wiring is Milestone 2.
//! start_tasks bypasses FreeRTOS entirely (Milestone 3 adds Xtensa RTOS).
//!
//! Panic handler and defmt transport are provided by esp-backtrace (Stage 5+).
//! Critical-section impl: esp-hal provides one via its internal HAL init.

// No-op defmt global logger so defmt symbols link on Xtensa.
// esp-backtrace handles the panic handler; we only need the logger stub here.
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
