#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
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

#[cfg(all(not(any(test, feature = "sim")), feature = "chip-rp2040"))]
use rp_pico::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

#[cfg(all(not(any(test, feature = "sim")), feature = "chip-rp2350"))]
use rp235x_hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

/// RP2350 bootrom block loop: IMAGE_DEF + END block.
///
/// The bootrom requires a circular linked list of at least two blocks.
/// Both are placed right after .vector_table (which lives at flash origin
/// 0x10000000).  Each block is 20 bytes (5 words); the offset field is a
/// signed byte offset from this block's start marker to the next block's
/// start marker.
#[cfg(all(not(any(test, feature = "sim")), feature = "chip-rp2350"))]
#[link_section = ".start_block"]
#[used]
pub static IMAGE_DEF: [u32; 10] = [
    // Block 1: IMAGE_DEF (secure ARM executable for RP2350)
    0xffff_ded3, // BLOCK_MARKER_START
    0x1021_0142, // IMAGE_TYPE: EXE | CHIP_RP2350 | CPU_ARM | SECURITY_S
    0x0000_01ff, // ITEM_LAST(1)
    0x0000_0014, // offset: +20 bytes → end block
    0xab12_3579, // BLOCK_MARKER_END
    // Block 2: END block (closes the loop)
    0xffff_ded3, // BLOCK_MARKER_START
    0x0000_01fe, // ITEM_2BS_IGNORED (placeholder)
    0x0000_01ff, // ITEM_LAST(1)
    0xffff_ffec, // offset: −20 bytes → IMAGE_DEF block
    0xab12_3579, // BLOCK_MARKER_END
];

#[cfg(not(any(test, feature = "sim")))]
#[global_allocator]
static GLOBAL: FreeRtosAllocator = FreeRtosAllocator;

#[cfg(all(not(any(test, feature = "sim")), feature = "chip-rp2040"))]
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

#[cfg(all(not(any(test, feature = "sim")), feature = "chip-rp2350"))]
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

#[cfg(not(any(test, feature = "sim")))]
#[entry]
fn main() -> ! {
    clock_init();

    // The APK lives exclusively in the persistent PAPK flash region.
    // probe-rs writes it there on every firmware flash (via the .papk_flash_init
    // ELF section); pdb install updates it over UART.
    let boot_apk: &'static [u8] =
        unsafe { packagemanager::flash::read_flash_papk() }.expect("PAPK flash region invalid");

    // pdb listener on UART1 (GP4/GP5). Priority 2 preempts jvm_task (priority 1).
    // Pinned to core 1 so it never contends with the JVM interpreter on core 0.
    Task::new()
        .name("pdb")
        .stack_size(1024)
        .priority(TaskPriority(task_priority::PRIORITY_RT_1))
        .core_affinity(0b10) // core 1 only
        .start(move |_| pdb::run_pdb_task())
        .unwrap();

    // JVM task: runs the app in a loop, rebooting when a new install arrives.
    // Pinned to core 0; all JVM child threads are also pinned to core 0 so the
    // single-core safety assumption of SharedJvmState remains valid.
    Task::new()
        .name("jvm")
        .stack_size(4096)
        .priority(TaskPriority(task_priority::PRIORITY_JVM_NORM))
        .core_affinity(0b01) // core 0 only
        .start(move |_| {
            // Store our handle so pdb_task and child tasks can notify us.
            pdb::pending::set_jvm_task(Task::current().unwrap());
            loop {
                pdb::pending::clear_stop();
                app::run_jvm_with(boot_apk);

                // Wake any child threads sleeping in vTaskDelay so they see STOP_JVM.
                pdb::pending::abort_all_child_delays();

                // Wait for all children to deregister themselves before resetting the heap.
                // The last child calls notify_jvm() when the counter reaches zero.
                // Check first to avoid blocking if they all exited before we got here.
                if pdb::pending::ACTIVE_JVM_THREADS.load(core::sync::atomic::Ordering::Acquire) > 0
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // If pdb_task requested a flash install, park core 0 in RAM.
                // On success pdb_task calls SYSRESETREQ directly — park_for_flash
                // never returns.  It returns only on error (pdb_task sets
                // CORE0_RELEASE after sending STATUS_ERR); in that case just
                // restart the JVM loop.
                if pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Acquire) {
                    unsafe { packagemanager::flash::park_for_flash() };
                    continue;
                }

                // Natural app exit — sleep until pdb installs a new app.
                CurrentTask::take_notification(true, Duration::infinite());

                if pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Acquire) {
                    unsafe { packagemanager::flash::park_for_flash() };
                    continue;
                }
            }
        })
        .unwrap();

    FreeRtosUtils::start_scheduler();
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
