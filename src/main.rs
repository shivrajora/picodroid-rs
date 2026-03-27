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

    // On boot, prefer a persistent PAPK from flash over the baked-in APK.
    let boot_apk: &'static [u8] = unsafe { pdb::flash::read_flash_papk().unwrap_or(app::APK_DATA) };

    // pdb listener on UART1 (GP4/GP5). Priority 2 preempts jvm_task (priority 1).
    // Pinned to core 1 so it never contends with the JVM interpreter on core 0.
    Task::new()
        .name("pdb")
        .stack_size(1024)
        .priority(TaskPriority(task_priority::PRIORITY_RT_1))
        .core_affinity(0b10) // core 1 only
        .start(move |_| pdb::run_pdb_task())
        .unwrap();

    // JVM task: loop forever, hot-swapping the app whenever pdb installs a new one.
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
                let apk = pdb::pending::take().unwrap_or(boot_apk);
                pdb::pending::clear_stop();
                app::run_jvm_with(apk);

                // Wake any child threads sleeping in vTaskDelay so they see STOP_JVM.
                pdb::pending::abort_all_child_delays();

                // Wait for all children to deregister themselves before resetting the heap.
                // The last child calls notify_jvm() when the counter reaches zero.
                // Check first to avoid blocking if they all exited before we got here.
                if pdb::pending::ACTIVE_JVM_THREADS.load(core::sync::atomic::Ordering::Acquire) > 0
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // If nothing new to run, idle until the next install.
                // pdb_task calls notify_jvm() after setting HAS_PENDING.
                // Check first to avoid blocking if the install already arrived.
                if !pdb::pending::HAS_PENDING.load(core::sync::atomic::Ordering::Relaxed) {
                    CurrentTask::take_notification(true, Duration::infinite());
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
