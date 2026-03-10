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

use bsp::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

fn clock_init() {
    let mut pac = pac::Peripherals::take().unwrap();
    let _sio = Sio::new(pac.SIO);
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let _clocks = init_clocks_and_plls(
        12_000_000u32,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();
}

#[entry]
fn main() -> ! {
    clock_init();

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
    #[allow(clippy::empty_loop)]
    loop {}
}

#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(_ef: &ExceptionFrame) -> ! {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationMallocFailedHook() {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationStackOverflowHook(_pxTask: FreeRtosTaskHandle, _pcTaskName: FreeRtosCharPtr) {
    asm::bkpt();
}
