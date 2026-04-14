#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
#[cfg(all(
    not(any(test, feature = "sim")),
    any(
        feature = "board-testbench-rp2040",
        feature = "board-testbench-rp2350",
        feature = "board-testbench-rp2350w",
        feature = "board-pico-enviro-mon"
    )
))]
mod boards;
#[allow(dead_code)]
mod drivers;
#[allow(dead_code)]
#[cfg(not(test))]
mod fs;
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
#[cfg(feature = "sim")]
mod sim_allocator;
mod system;
mod task_priority;

// Host-testable pure-logic slices of RP HAL drivers. The rest of `hal::rp`
// is ARM-only and cfg-gated out on the host; these modules have no
// `rp-pico`/`rp235x-hal` deps, so pulling them in directly via `#[path]`
// lets their `#[cfg(test)]` blocks run under `scripts/test.sh`.
#[cfg(test)]
#[path = "hal/rp/i2c/protocol.rs"]
mod hal_rp_i2c_protocol_tests;
#[cfg(test)]
#[path = "hal/rp/pdb_usb/protocol.rs"]
mod hal_rp_pdb_usb_protocol_tests;
#[cfg(test)]
#[path = "hal/rp/spi/protocol.rs"]
mod hal_rp_spi_protocol_tests;

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

#[cfg(feature = "sim")]
#[global_allocator]
static GLOBAL: sim_allocator::CappedAllocator = sim_allocator::CappedAllocator::new();

#[cfg(not(any(test, feature = "sim")))]
#[entry]
fn main() -> ! {
    hal::boot::clock_init();

    let boot_apk: &'static [u8] =
        unsafe { hal::flash::read_flash_papk() }.expect("PAPK flash region invalid");

    // Mount LittleFS on the FS_FLASH region.  Runs pre-scheduler (single-core),
    // formats on first boot or after corruption.  A failure here is non-fatal
    // for the JVM — persistence simply remains unavailable until the next boot.
    if let Err(e) = fs::init() {
        defmt::warn!("[fs] init failed: {}", defmt::Display2Format(&e));
    }

    hal::boot::start_tasks(boot_apk)
}

#[cfg(feature = "sim")]
fn main() {
    if let Err(e) = fs::init() {
        eprintln!("[sim][fs] init failed: {}", e);
    }

    app::run_jvm();

    let (current, peak, limit) = GLOBAL.heap_stats();
    if limit == usize::MAX {
        println!("[sim] heap: peak {} KB (unlimited)", peak / 1024);
    } else {
        println!(
            "[sim] heap: peak {} KB / {} KB limit ({} KB current)",
            peak / 1024,
            limit / 1024,
            current / 1024,
        );
    }
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
    #[cfg(not(feature = "chip-rp2350-hal"))]
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationMallocFailedHook() {
    // Intentionally a no-op: pvPortMalloc returns NULL after this hook,
    // allowing Rust's try_reserve_exact to return Err and trigger GC.
    // Non-try allocations still abort via Rust's handle_alloc_error.
}

#[cfg(not(any(test, feature = "sim")))]
#[allow(non_snake_case)]
#[no_mangle]
fn vApplicationStackOverflowHook(_pxTask: FreeRtosTaskHandle, _pcTaskName: FreeRtosCharPtr) {
    asm::bkpt();
    #[allow(clippy::empty_loop)]
    loop {}
}
