// SPDX-License-Identifier: GPL-3.0-only
//! Core-1 parking for runtime flash operations.
//!
//! `with_xip_disabled!` (flash.rs) masks interrupts on the **calling core
//! only**.  Whatever core 1 does during an erase/program window — 45 ms per
//! sector, seconds for a PAPK slot — it does against disconnected flash.  An
//! exception entry is the fatal case: it reads the vector table and handler
//! code from flash the ROM routine has just turned off, the fetch bus-faults,
//! the HardFault vector fetch faults too, and core 1 goes into LOCKUP — still
//! holding whatever FreeRTOS spinlock it had, so core 0 wedges at its next
//! critical section.  Both chips get there, by different routes:
//!
//! - **RP2040** runs the tick on core 1 (`configTICK_CORE=1`), so a SysTick
//!   inside the window is guaranteed.  Bugs RP40-1 and RP40-2.
//! - **RP2350** runs the tick on core 0, but core 0's yields reach core 1 as
//!   doorbell IPIs, and the resulting PendSV runs `vTaskSwitchContext` from
//!   flash.  Rarer — it needs a yield to land in the window — which is why it
//!   read as flaky (5/10 `pdb install` cycles) rather than deterministic, and
//!   why it stayed hidden until `d66882b` made those IPIs actually fire.
//!
//! Both are one bug; see docs/bugs-rp2040-flash-2026-08-01.md.
//!
//! The fix is the pico-sdk `flash_safe_execute` lockout pattern, expressed as
//! a FreeRTOS task: a top-real-time-priority task pinned to core 1 blocks on
//! a notification.  Before disabling XIP, core 0 sets [`PARK_REQUESTED`],
//! notifies the task, and waits for [`PARKED`].  The task then spins in a
//! **RAM-resident, single-`asm!`-block** loop with interrupts masked — no
//! flash fetches, no exception entries — until core 0 (XIP restored) clears
//! the request.  Both flags live in SRAM, which stays accessible throughout.
//!
//! This requires `configRUN_MULTIPLE_PRIORITIES=1` (FreeRTOSConfig.h): the
//! kernel default of 0 bars the priority-22 fs task from core 0 while the
//! priority-30 parker holds core 1, and the park request can then never be
//! cleared.
//!
//! # Single-flash-op invariant
//! Callers of [`park_core1_for_flash`] must not overlap.  This is the same
//! invariant the flash primitives already rely on: LittleFS traffic is
//! serialised onto the fs worker, PAPK installs park the JVM first, and both
//! tasks are pinned to core 0.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

/// Set by core 0 to ask core 1 to park; cleared to release it.
static PARK_REQUESTED: AtomicU32 = AtomicU32::new(0);
/// Set by the parker once core 1 is spinning in RAM with interrupts masked;
/// cleared on its way out.  Core 0 must not touch flash while this is 0.
static PARKED: AtomicU32 = AtomicU32::new(0);

// SAFETY: written once from `start_tasks` before the scheduler starts
// (single-core at that point); read-only afterwards.
struct TaskCell(UnsafeCell<Option<freertos_rust::Task>>);
unsafe impl Sync for TaskCell {}
static PARKER_TASK: TaskCell = TaskCell(UnsafeCell::new(None));

/// Store the parker task handle.  Called once from `start_tasks`, right after
/// the task is created and before the scheduler starts.
pub fn set_parker_task(task: freertos_rust::Task) {
    unsafe { *PARKER_TASK.0.get() = Some(task) };
}

/// Body of the `flashpark` task (core 1, `PRIORITY_FLASH_PARK`).
pub fn run_parker_task() -> ! {
    loop {
        freertos_rust::CurrentTask::take_notification(true, freertos_rust::Duration::infinite());
        if PARK_REQUESTED.load(Ordering::SeqCst) != 0 {
            // Entered with XIP still enabled (core 0 waits for PARKED before
            // touching flash) and left after it is restored (core 0 clears
            // PARK_REQUESTED only after the operation completes).
            unsafe { park_until_released(PARK_REQUESTED.as_ptr(), PARKED.as_ptr()) };
        }
    }
}

/// The park loop itself: mask interrupts, signal `ack`, spin on `req`,
/// un-signal, unmask.  One `asm!` block in `.data` so that *nothing* — not
/// even a precompiled `core::` helper in a debug build — is fetched from
/// flash while core 0 has XIP disabled.  SRAM data reads are always safe.
#[link_section = ".data"]
#[inline(never)]
unsafe fn park_until_released(req: *const u32, ack: *mut u32) {
    core::arch::asm!(
        "cpsid i",
        "movs {tmp}, #1",
        "str  {tmp}, [{ack}]",
        "dmb",
        "2:",
        "ldr  {tmp}, [{req}]",
        "cmp  {tmp}, #0",
        "bne  2b",
        "movs {tmp}, #0",
        "str  {tmp}, [{ack}]",
        "dmb",
        "cpsie i",
        req = in(reg) req,
        ack = in(reg) ack,
        tmp = out(reg) _,
        options(nostack),
    );
}

/// Guard proving core 1 is parked (or that parking is unnecessary).
/// Dropping it releases core 1.
pub struct FlashParkGuard {
    active: bool,
}

/// Park core 1 for the duration of the returned guard.
///
/// Skips the handshake (returns an inactive guard) before the scheduler is
/// running: core 1 is only launched by `xPortStartScheduler`, so earlier
/// flash operations are single-core and need no parking.
pub fn park_core1_for_flash() -> FlashParkGuard {
    let parker = unsafe { (*PARKER_TASK.0.get()).as_ref() };
    let Some(parker) = parker else {
        return FlashParkGuard { active: false };
    };
    if freertos_rust::FreeRtosUtils::scheduler_state()
        != freertos_rust::FreeRtosSchedulerState::Running
    {
        return FlashParkGuard { active: false };
    }

    PARK_REQUESTED.store(1, Ordering::SeqCst);
    parker.notify(freertos_rust::TaskNotification::Increment);
    // Busy-wait, not block: the parker runs on core 1, so nothing on this
    // core needs to be scheduled for it to make progress.  Wake latency is
    // one cross-core yield IPI — microseconds.
    while PARKED.load(Ordering::SeqCst) == 0 {
        core::hint::spin_loop();
    }
    FlashParkGuard { active: true }
}

impl Drop for FlashParkGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        PARK_REQUESTED.store(0, Ordering::SeqCst);
        while PARKED.load(Ordering::SeqCst) != 0 {
            core::hint::spin_loop();
        }
    }
}
