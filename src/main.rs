#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
#[cfg(all(not(any(test, feature = "sim")), feature = "board-waveshare-pico-28"))]
mod boards;
#[cfg(feature = "display-test")]
mod display_test;
#[allow(dead_code)]
mod drivers;
#[allow(dead_code)]
mod hal;
#[cfg(not(test))]
mod lifecycle;
#[cfg(not(test))]
mod lvgl_ffi;
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

    // Display test: skip JVM, run LVGL test UI directly on a FreeRTOS task.
    #[cfg(feature = "display-test")]
    {
        Task::new()
            .name("disp")
            .stack_size(8192)
            .priority(TaskPriority(task_priority::PRIORITY_JVM_NORM))
            .start(move |_| display_test::run())
            .unwrap();

        FreeRtosUtils::start_scheduler()
    }

    // Normal path: boot the JVM with the installed APK.
    #[cfg(not(feature = "display-test"))]
    {
        let boot_apk: &'static [u8] =
            unsafe { hal::flash::read_flash_papk() }.expect("PAPK flash region invalid");

        hal::boot::start_tasks(boot_apk)
    }
}

#[cfg(all(feature = "sim", not(feature = "display-test")))]
fn main() {
    app::run_jvm();
}

#[cfg(all(feature = "sim", feature = "display-test"))]
fn main() {
    display_test::run();
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
    #[allow(clippy::empty_loop)]
    loop {}
}
