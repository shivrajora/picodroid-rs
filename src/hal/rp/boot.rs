#[cfg(feature = "chip-rp2040")]
use rp_pico::hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

#[cfg(feature = "chip-rp2350-hal")]
use rp235x_hal::{clocks::init_clocks_and_plls, pac, sio::Sio, watchdog::Watchdog};

use freertos_rust::*;

use crate::task_priority;

/// RP2350 bootrom block loop: IMAGE_DEF + END block.
///
/// The bootrom requires a circular linked list of at least two blocks.
/// Both are placed right after .vector_table (which lives at flash origin
/// 0x10000000).  Each block is 20 bytes (5 words); the offset field is a
/// signed byte offset from this block's start marker to the next block's
/// start marker.
#[cfg(feature = "chip-rp2350-hal")]
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

#[cfg(feature = "chip-rp2040")]
pub fn clock_init() {
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

#[cfg(feature = "chip-rp2350-hal")]
pub fn clock_init() {
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

/// Create FreeRTOS tasks and start the scheduler (never returns).
///
/// RP2040/RP2350 are dual-core:
///   - PDB task on core 1 (priority 2) — listens for UART1 installs
///   - JVM task on core 0 (priority 1) — runs the app
pub fn start_tasks(boot_apk: &'static [u8]) -> ! {
    // pdb listener on UART1 (GP4/GP5). Priority 2 preempts jvm_task (priority 1).
    // Pinned to core 1 so it never contends with the JVM interpreter on core 0.
    Task::new()
        .name("pdb")
        .stack_size(2048)
        .priority(TaskPriority(task_priority::PRIORITY_RT_1))
        .core_affinity(0b10) // core 1 only
        .start(move |_| crate::pdb::run_pdb_task())
        .unwrap();

    // JVM task: runs the app in a loop, rebooting when a new install arrives.
    // Pinned to core 0; all JVM child threads are also pinned to core 0 so the
    // single-core safety assumption of SharedJvmState remains valid.
    // RP2350 (Cortex-M33, FPU) needs a larger stack than RP2040 (Cortex-M0+)
    // because each interrupt pushes an extended exception frame (~100 bytes)
    // when the FPU has been used (configENABLE_FPU=1).
    #[cfg(feature = "chip-rp2350-hal")]
    let jvm_stack: u16 = 8192;
    #[cfg(not(feature = "chip-rp2350-hal"))]
    let jvm_stack: u16 = 4096;

    Task::new()
        .name("jvm")
        .stack_size(jvm_stack)
        .priority(TaskPriority(task_priority::PRIORITY_JVM_NORM))
        .core_affinity(0b01) // core 0 only
        .start(move |_| {
            // Store our handle so pdb_task and child tasks can notify us.
            crate::pdb::pending::set_jvm_task(Task::current().unwrap());
            loop {
                crate::pdb::pending::clear_stop();
                crate::app::run_jvm_with(boot_apk);

                // Wake any child threads sleeping in vTaskDelay so they see STOP_JVM.
                crate::pdb::pending::abort_all_child_delays();

                // Wait for all child threads to deregister.  Loop because
                // notifications may arrive from pdb_task (consumed harmlessly)
                // before the last child calls notify_jvm().
                while crate::pdb::pending::ACTIVE_JVM_THREADS
                    .load(core::sync::atomic::Ordering::Acquire)
                    > 0
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // If pdb_task requested a flash install, park core 0 in RAM.
                // On success pdb_task triggers a watchdog reset — park_for_flash
                // never returns.  It returns only on error (pdb_task sets
                // CORE0_RELEASE after sending STATUS_ERR); in that case just
                // restart the JVM loop.
                if crate::pdb::pending::FLASH_PARK_REQUESTED
                    .load(core::sync::atomic::Ordering::Acquire)
                {
                    unsafe { crate::hal::flash::park_for_flash() };
                    continue;
                }

                // Natural app exit — wait for pdb to install a new app.
                //
                // On RP2040 (configTICK_CORE=1), cross-core notifications
                // work so we sleep and let notify_jvm() wake us directly.
                // On RP2350 (configTICK_CORE=0), the doorbell-based yield
                // is unreliable; use WFE to sleep at low power until an
                // SEV from notify_jvm() wakes us.
                #[cfg(not(feature = "chip-rp2350-hal"))]
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                    if crate::pdb::pending::FLASH_PARK_REQUESTED
                        .load(core::sync::atomic::Ordering::Acquire)
                    {
                        unsafe { crate::hal::flash::park_for_flash() };
                        continue;
                    }
                }
                #[cfg(feature = "chip-rp2350-hal")]
                loop {
                    if crate::pdb::pending::FLASH_PARK_REQUESTED
                        .load(core::sync::atomic::Ordering::Acquire)
                    {
                        unsafe { crate::hal::flash::park_for_flash() };
                        break;
                    }
                    cortex_m::asm::wfe();
                }
            }
        })
        .unwrap();

    FreeRtosUtils::start_scheduler()
}
