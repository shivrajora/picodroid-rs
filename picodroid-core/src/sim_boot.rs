// SPDX-License-Identifier: GPL-3.0-only
//! The simulator's `main` and task topology — `main.rs` plus `boot_tasks.rs`
//! for the host.
//!
//! Deliberately the same shape as a device boot, in the same order, because
//! that is most of the point of running the real kernel here: the simulator
//! stops taking its own shortcuts (a synchronous filesystem behind a
//! `std::sync::Mutex`, an executor pool that falls back to the main queue)
//! and starts exercising the code paths hardware runs
//! (`docs/designs/freertos-host-sim.md` §1.2).
//!
//! # Why this is here and not in the family crate
//!
//! It was in `platforms/rp` for one day. Roughly four fifths of it names
//! nothing family-specific — arming the allocator, the boot-budget precharge,
//! the pool, the JVM task, the child-drain wait, the scheduler handoff, the
//! closing heap banner — so a second family's simulator would have copied it
//! verbatim, which is exactly the mechanism that gave the removed ESP scaffold
//! seventeen drifting sim-stub twins (`docs/designs/family-neutral-residue.md`
//! §0, B11). The simulator lives in this crate; its boot sequence belongs
//! with it.
//!
//! What genuinely is family policy arrives as `register_sim_platform!`
//! parameters and reaches [`main`] as arguments: the boot-budget model
//! (chip-gated data this crate has no business reading) and the function
//! that runs the family's app (`docs/designs/porting-seam-2026-09.md` E6).
//! The generated `sim_main()` in the family's `glue.rs` is the one line that
//! joins them.
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

use crate::hal::sim::allocator;
use crate::hal::sim::boot_budget::{self, BootBudgetModel};
use crate::hal::sim::rtos as sim_rtos;
use crate::rtos::{self, TaskKind, TaskSpec};

/// Boot the simulator and run one app to completion.
///
/// Everything before the scheduler handoff is the work a device also does
/// pre-scheduler (arming the heap model, mounting the filesystem); everything
/// after only runs once the JVM task has ended the scheduler. One app per
/// process, as `sim-run.sh` already assumes.
pub fn main(model: &'static BootBudgetModel, run_app: fn()) {
    // Start device-heap accounting at the sim's "reset vector". Everything
    // before this is host-runtime noise; everything after is charged to the
    // heap_4 arena exactly as the device charges its FreeRTOS heap.
    allocator::arm();
    // Charge the FreeRTOS boot structures (task stacks, TCBs, queues) the
    // device allocates from this same arena — measured at ~85 KB on HW (V4).
    boot_budget::precharge(model);
    allocator::checkpoint("baseline");

    // The host-file image has the same block layout as a device's flash
    // region, so its bytes stay interchangeable with a flash dump.
    #[cfg(feature = "littlefs")]
    if let Err(e) = crate::fs::init_host_image() {
        eprintln!("[sim][fs] init failed: {}", e);
    }
    allocator::checkpoint("post-fs-init");

    run(model, run_app);

    allocator::checkpoint("final");

    let (current, peak, limit) = allocator::heap_stats();
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

/// Create the boot tasks, then hand this thread to the scheduler.
///
/// Returns when the JVM task has finished the app and ended the scheduler.
fn run(model: &'static BootBudgetModel, run_app: fn()) {
    // JVM heap compound operations and the GC are scheduler-atomic here
    // exactly as on a device: the same installer, so no `AtomicSection` is a
    // silent no-op on one target and real on the other.
    crate::rtos::freertos::install_heap_atomic_hooks();

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
    // registered hooks rather than from two more arguments here.
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
    boot_budget::report(model);

    sim_rtos::start_scheduler();
}
