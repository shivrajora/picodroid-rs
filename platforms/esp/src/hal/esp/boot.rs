// SPDX-License-Identifier: GPL-3.0-only
//! ESP32-S3 boot — Milestone 1 stub.
//!
//! clock_init is a no-op; real ClockControl::max() wiring is Milestone 2.
//! start_tasks bypasses FreeRTOS entirely (Milestone 3 adds Xtensa RTOS).
//!
//! Panic handler is provided by esp-backtrace.
//! Critical-section: no-op impl for single-threaded M1; replace with
//! esp-hal's interrupt-disabling impl in Milestone 2.

struct EspCriticalSection;
critical_section::set_impl!(EspCriticalSection);

unsafe impl critical_section::Impl for EspCriticalSection {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        0
    }
    unsafe fn release(_token: critical_section::RawRestoreState) {}
}

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
