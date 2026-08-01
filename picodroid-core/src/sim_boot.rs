// SPDX-License-Identifier: GPL-3.0-only
//! Task topology for the simulator — `boot_tasks.rs` for the host.
//!
//! Deliberately the same shape as a device's `start_tasks`, in the same order,
//! because that is most of the point of running the real kernel here: the
//! simulator stops taking its own shortcuts (a synchronous filesystem behind a
//! `std::sync::Mutex`, an executor pool that falls back to the main queue) and
//! starts exercising the code paths hardware runs
//! (`docs/designs/freertos-host-sim.md` §1.2).
//!
//! # Why this is here and not in the family crate
//!
//! It was in `platforms/rp` for one day. Roughly four fifths of it names
//! nothing family-specific — the pool, the JVM task, the child-drain wait, the
//! scheduler handoff — so a second family's simulator would have copied it
//! verbatim, which is exactly the mechanism that gave the removed ESP scaffold
//! seventeen drifting sim-stub twins (`docs/designs/family-neutral-residue.md`
//! §0, B11). The simulator lives in this crate; its boot sequence belongs with
//! it.
//!
//! What genuinely is family policy arrives as [`BootLeaves`], the same shape
//! `register_sim_platform!` already uses for the other simulator leaves (that
//! doc's D6): stack sizing and the boot-budget model are chip-gated data this
//! crate has no business reading.
//!
//! It carried a third leaf, `extra_boot_tasks`, whose only caller spawned the
//! LittleFS worker. Stage 5 moved `fs` into this crate, so the sequence now
//! spawns that worker itself and the hook is gone — as B11 said it would be.
//!
//! # What this is *not*
//!
//! A device's `start_tasks` also creates a debug-bridge listener and a WiFi
//! task, pins tasks to cores, and parks its JVM task forever in a supervisor
//! loop waiting for the next install. None of that appears here: two of them
//! have no simulator endpoint, core affinity is meaningless on a single-core
//! port, and the simulator runs one app and exits. The supervisor loop stays
//! family-side for the reason `family-neutral-residue.md` D4 gives — it
//! encodes a flash topology, not a lifecycle.

use alloc::boxed::Box;
use std::panic::AssertUnwindSafe;

use crate::hal::sim::rtos as sim_rtos;
use crate::rtos::{self, TaskKind, TaskSpec};

/// The family policy this boot sequence needs, and nothing more.
///
/// Function pointers rather than a trait: there is exactly one simulator per
/// process and each of these is a leaf, so a vtable would buy indirection and
/// no dispatch.
pub struct BootLeaves {
    /// Run the app to completion. The family's, because where the app bytes
    /// come from is a property of its flash layout and build
    /// (`platforms/rp/src/app.rs`).
    pub run_app: fn(),

    /// Assert the boot budget adds up, once every task exists. Chip-gated
    /// arithmetic, so the family owns it.
    pub report_boot_budget: fn(),
}

/// Create the boot tasks, then hand this thread to the scheduler.
///
/// Returns when the JVM task has finished the app and ended the scheduler —
/// one app per process, as `sim-run.sh` already assumes.
pub fn run(leaves: BootLeaves) {
    // The filesystem worker first, matching the device's order: it has to
    // exist before anything can ask it for a file. Its stack charge rides the
    // same `charge_task_spawn` every other task's does, because it is created
    // through the `Rtos` seam rather than beside it.
    #[cfg(feature = "littlefs")]
    crate::fs::spawn_worker();

    // No sensor task. A device sampler exists to drive real I²C parts, and
    // there are none here; the simulator keeps its own backing, which
    // fabricates snapshots on a host thread outside the kernel and publishes
    // them through an all-atomic seqlock mailbox. The boot budget charges it
    // as a modeled task for the same reason.

    // Background thread pool. Without this the simulator falls back to
    // draining `Executors.backgroundExecutor()` work on the main queue, which
    // is a different concurrency shape from the device's four workers.
    crate::bg_worker::install();
    crate::executors::background_pool::spawn();

    // The JVM task, through the same seam every other task uses — so its
    // stack size and its boot-budget charge come from the platform's
    // registered hooks rather than from two more fields here.
    let run_app = leaves.run_app;
    let spec = TaskSpec {
        name: "jvm",
        kind: TaskKind::Jvm,
        priority: crate::task_priority::PRIORITY_JVM_NORM,
        stack_bytes: None, // platform's Jvm default (boot budget)
    };
    assert!(
        rtos::spawn(
            &spec,
            Box::new(move || {
                // Unwinding across the port's `extern "C"` task trampoline is
                // UB, and abort-on-panic is what a device does under
                // panic-probe. Catch here rather than letting the default hook
                // run, so the scheduler is not left owning a dead process's
                // main thread.
                if std::panic::catch_unwind(AssertUnwindSafe(run_app)).is_err() {
                    eprintln!("[sim] jvm task panicked — aborting");
                    std::process::abort();
                }

                // Wait for Java threads the app started, exactly as a device's
                // supervisor loop does before letting an install reboot the
                // app. Without it an `onCreate` that starts threads and
                // returns would end the scheduler out from under children that
                // never ran an instruction — the same visible outcome as a
                // host-thread model's deliberate no-op, reached by accident.
                //
                // A poll rather than the device's task notifications: the
                // device must be woken *promptly* because a flash erase is
                // queued behind it, and it has the bookkeeping to do that.
                // Here the only thing waiting is process exit.
                while sim_rtos::live_jvm_children() > 0 {
                    rtos::delay_ms(10);
                }

                // Releases the main thread from `start_scheduler` below, and
                // never returns — so the spawn trampoline's park is
                // unreachable for this task.
                sim_rtos::end_scheduler();
            }),
        ),
        "jvm task"
    );

    // Every boot task now exists, so the model is complete and comparable with
    // the device figure. Deliberately before the scheduler starts:
    // `Thread.start` charges through the same counter, and once the app runs
    // the total stops being a boot number.
    (leaves.report_boot_budget)();

    sim_rtos::start_scheduler();
}
