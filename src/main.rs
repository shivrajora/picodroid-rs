#![cfg_attr(not(any(test, feature = "sim")), no_std)]
#![cfg_attr(not(any(test, feature = "sim")), no_main)]

extern crate alloc;

mod app;
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
        unsafe { pdb::flash::read_flash_papk() }.expect("PAPK flash region invalid");

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
                defmt::info!("[jvm] starting app");
                pdb::pending::clear_stop();
                app::run_jvm_with(boot_apk);
                defmt::info!(
                    "[jvm] app exited — STOP_JVM={} FLASH_PARK={}",
                    pdb::pending::STOP_JVM.load(core::sync::atomic::Ordering::Relaxed),
                    pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Relaxed),
                );

                // Wake any child threads sleeping in vTaskDelay so they see STOP_JVM.
                pdb::pending::abort_all_child_delays();

                // Wait for all children to deregister themselves before resetting the heap.
                // The last child calls notify_jvm() when the counter reaches zero.
                // Check first to avoid blocking if they all exited before we got here.
                let active =
                    pdb::pending::ACTIVE_JVM_THREADS.load(core::sync::atomic::Ordering::Acquire);
                defmt::info!("[jvm] active_threads={}", active);
                if active > 0 {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // If pdb_task requested a flash install, park core 0 in RAM.
                // On success pdb_task calls SYSRESETREQ directly — park_for_flash
                // never returns.  It returns only on error (pdb_task sets
                // CORE0_RELEASE after sending STATUS_ERR); in that case just
                // restart the JVM loop.
                if pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Acquire) {
                    defmt::info!("[jvm] parking for flash (post-exit)");
                    unsafe { pdb::flash::park_for_flash() };
                    defmt::warn!("[jvm] park returned — install failed, restarting JVM");
                    continue;
                }

                // Natural app exit — sleep until pdb installs a new app.
                defmt::info!("[jvm] natural exit — sleeping for notification");
                CurrentTask::take_notification(true, Duration::infinite());
                defmt::info!(
                    "[jvm] woken: FLASH_PARK={}",
                    pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Relaxed),
                );

                if pdb::pending::FLASH_PARK_REQUESTED.load(core::sync::atomic::Ordering::Acquire) {
                    defmt::info!("[jvm] parking for flash (woken)");
                    unsafe { pdb::flash::park_for_flash() };
                    defmt::warn!("[jvm] park returned — install failed, restarting JVM");
                    continue;
                }

                defmt::warn!(
                    "[jvm] fell through all checks — looping back (unexpected if stopped by pdb)"
                );
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
