#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
extern crate alloc;

mod framework;
mod system;

#[cfg(not(test))]
use cortex_m::asm;
#[cfg(not(test))]
use cortex_m_rt::{entry, exception, ExceptionFrame};
#[cfg(not(test))]
use defmt_rtt as _;
#[cfg(not(test))]
use freertos_rust::*;
#[cfg(not(test))]
use panic_probe as _;

#[cfg(all(not(test), feature = "chip-rp2040"))]
use rp_pico::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

#[cfg(all(not(test), feature = "chip-rp2350"))]
use rp235x_hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

#[cfg(not(test))]
#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;

#[cfg(all(not(test), feature = "chip-rp2040"))]
fn clock_init() {
    // RP2040: 12 MHz crystal → 125 MHz system clock
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

#[cfg(all(not(test), feature = "chip-rp2350"))]
fn clock_init() {
    // RP2350: 12 MHz crystal → 150 MHz system clock
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

#[cfg(not(test))]
#[entry]
fn main() -> ! {
    clock_init();

    Task::new()
        .name("jvm")
        .stack_size(4096)
        .start(move |_| {
            framework::run_jvm();
        })
        .unwrap();
    FreeRtosUtils::start_scheduler();
}

#[cfg(not(test))]
#[allow(non_snake_case)]
#[exception]
unsafe fn DefaultHandler(_irqn: i16) {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(test))]
#[allow(non_snake_case)]
#[exception]
unsafe fn HardFault(_ef: &ExceptionFrame) -> ! {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(test))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationMallocFailedHook() {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(test))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationStackOverflowHook(_pxTask: FreeRtosTaskHandle, _pcTaskName: FreeRtosCharPtr) {
    asm::bkpt();
}
