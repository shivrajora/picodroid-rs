#![no_std]
#![no_main]

extern crate alloc;

mod framework;
mod system;

use bsp::entry;
use cortex_m::asm;
use cortex_m_rt::{exception, ExceptionFrame};
use defmt_rtt as _;
use freertos_rust::*;
use panic_probe as _;

#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;

use rp_pico as bsp;

#[entry]
fn main() -> ! {
    Task::new()
        .name("jvm")
        .stack_size(2048)
        .start(move |_| {
            framework::run_jvm();
        })
        .unwrap();
    FreeRtosUtils::start_scheduler();
}

#[allow(non_snake_case)]
#[exception]
unsafe fn DefaultHandler(_irqn: i16) {
    asm::bkpt();
    loop {}
}

#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(_ef: &ExceptionFrame) -> ! {
    asm::bkpt();
    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationMallocFailedHook() {
    asm::bkpt();
    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationStackOverflowHook(_pxTask: FreeRtosTaskHandle, _pcTaskName: FreeRtosCharPtr) {
    asm::bkpt();
}
