#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
#[allow(dead_code)]
mod hal;
#[cfg(not(any(test, feature = "sim")))]
mod packagemanager;
#[cfg(not(any(test, feature = "sim")))]
mod pdb;
mod system;
mod task_priority;

#[cfg(not(any(test, feature = "sim")))]
use cortex_m::asm;
#[cfg(not(any(test, feature = "sim")))]
use cortex_m_rt::{entry, exception, ExceptionFrame};
#[cfg(not(any(test, feature = "sim")))]
use defmt_rtt as _;
#[cfg(not(any(test, feature = "sim")))]
use freertos_rust::*;
#[cfg(not(any(test, feature = "sim")))]
use panic_probe as _;

#[cfg(not(any(test, feature = "sim")))]
#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;

#[cfg(not(any(test, feature = "sim")))]
#[entry]
fn main() -> ! {
    hal::boot::clock_init();

    // The APK lives exclusively in the persistent PAPK flash region.
    // probe-rs writes it there on every firmware flash (via the .papk_flash_init
    // ELF section); pdb install updates it over UART.
    let boot_apk: &'static [u8] =
        unsafe { hal::flash::read_flash_papk() }.expect("PAPK flash region invalid");

    hal::boot::start_tasks(boot_apk)
}

#[cfg(feature = "sim")]
fn main() {
    app::run_jvm();
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[exception]
unsafe fn DefaultHandler(_irqn: i16) {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(_ef: &ExceptionFrame) -> ! {
    // On RP2040 (Cortex-M0+) bkpt halts cleanly with a debugger attached.
    // On RP2350 (Cortex-M33) bkpt without a debugger causes a re-entrant
    // fault → lockup, so we skip it.
    #[cfg(not(feature = "chip-rp2350"))]
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationMallocFailedHook() {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationStackOverflowHook(_pxTask: FreeRtosTaskHandle, _pcTaskName: FreeRtosCharPtr) {
    asm::bkpt();
}
