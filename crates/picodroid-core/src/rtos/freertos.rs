// SPDX-License-Identifier: GPL-3.0-only
//! What every FreeRTOS-hosted family shares, whatever the port.
//!
//! The one module outside `hal/sim` that names a kernel symbol. It exists
//! because both boots — the device's `boot_tasks.rs` and the simulator's
//! `sim_boot.rs` — installed the same fourteen lines and asked, in a comment,
//! to be kept in lockstep by hand. Routing scheduler suspension through the
//! `__pd_rtos_*` facade instead would put a seam crossing on every
//! `AtomicSection`, the hottest path in the JVM, for the sake of a family
//! that does not exist (docs/designs/porting-seam-2026-09.md E7). A family on
//! another kernel never calls this and writes its own installer;
//! `rtos::seam_guard` allowlists exactly this file.

/// Make the JVM's heap compound operations and the GC scheduler-atomic.
///
/// Install before the first task exists. On the device, every create-and-pin
/// (`task_affinity::spawn`, every runtime `Thread.start`) runs inside this
/// section, and the SMP kernel's equal-priority wake yield (`prvYieldForTask`
/// uses `>=`) would otherwise let an unblocked JVM task preempt the allocator
/// mid-resize — the picoenvmon span-overlap corruption. Scheduler suspension
/// nests safely with heap_4's own. On the simulator's single-core POSIX port
/// the guard costs a counter increment, and with it both boots make the same
/// promise: no `AtomicSection` is a silent no-op on one target and real on
/// the other.
#[cfg(not(test))]
pub fn install_heap_atomic_hooks() {
    extern "C" {
        fn vTaskSuspendAll();
        fn xTaskResumeAll() -> i32;
    }
    fn enter() {
        // SAFETY: FFI into the kernel; nests with the allocator's own
        // suspension.
        unsafe { vTaskSuspendAll() };
    }
    fn exit() {
        // SAFETY: as above; whether a yield happened is not needed.
        unsafe {
            xTaskResumeAll();
        }
    }
    pico_jvm::atomic_section::set_hooks(enter, exit);
    // The other half of the heap's concurrency contract, created here for the
    // same reason: before the first task, by both boots (`crate::jvm_run_lock`).
    crate::jvm_run_lock::init();
}

/// The kernel's run-time counter for the calling task: the CPU time it has
/// been scheduled for, in the port's run-time-stats unit (microseconds on
/// the RP boards). Wraps; take deltas. Feeds the `parity-metrics` span
/// report, where wall time minus this is the time the UI task spent
/// preempted or blocked. Device only: the host port compiles the kernel
/// without run-time stats.
#[cfg(all(not(test), not(feature = "sim"), feature = "parity-metrics"))]
pub fn current_task_runtime_counter() -> u32 {
    extern "C" {
        fn ulTaskGetRunTimeCounter(task: *const core::ffi::c_void) -> u32;
    }
    // SAFETY: FFI into the kernel; a null handle names the calling task.
    unsafe { ulTaskGetRunTimeCounter(core::ptr::null()) }
}
