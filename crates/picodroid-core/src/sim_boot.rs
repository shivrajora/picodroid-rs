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
//! A device's `start_tasks` also creates a WiFi task and pins tasks to
//! cores; neither appears here — there is no simulator WiFi endpoint, and
//! core affinity is meaningless on a single-core port. The debug bridge
//! *is* here (`hal::sim::pdb`, a task of the device's priority on a Unix
//! socket), and so is its install park: the supervisor loop below is the
//! device's — run what the package directory says, stop what the app left
//! behind, park for the bridge when it asks, ask what runs next
//! (`packages::next_image`). The one divergence is what happens when the
//! answer is nothing: a device waits for an install, the simulator exits,
//! because one app per process is what `sim-run.sh`'s rows and every
//! `sim.sh --app X` invocation rely on. `PICODROID_SIM_WAIT_FOR_INSTALL=1`
//! chooses the device's behaviour instead.
//!
//! An install over the bridge ends with the device's reset, which here is a
//! process exec: the bridge ends the scheduler, [`main`] regains the thread,
//! and `hal::sim::pdb::reboot` dumps the region and starts the binary again
//! as a warm boot — the whole sequence above runs again from the top.

use alloc::boxed::Box;
use std::panic::AssertUnwindSafe;

use crate::hal::sim::allocator;
use crate::hal::sim::boot_budget::{self, BootBudgetModel};
use crate::hal::sim::rtos as sim_rtos;
use crate::rtos::{self, TaskKind, TaskSpec};

/// Boot the simulator and run apps until the directory has nothing to run.
///
/// Everything before the scheduler handoff is the work a device also does
/// pre-scheduler (arming the heap model, mounting the filesystem); everything
/// after only runs once the JVM task has ended the scheduler. Without system
/// apps that is one app per process, as `sim-run.sh` assumes; with a
/// launcher loaded the process runs until it is killed, as a device does.
pub fn main(model: &'static BootBudgetModel) {
    // The scheduling monitor's clock and printer thread, before any task
    // exists: the kernel hooks read the clock from the first task creation
    // on, and the thread must not be charged to the arena.
    #[cfg(feature = "sched-diag")]
    crate::hal::sim::rtos::sched_diag_start();
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

    // The app region, seeded from PICODROID_APK_PATH and PICODROID_SIM_APPS;
    // like the filesystem image it models flash, so it is not charged.
    crate::hal::sim::app_region::init();
    // Data of packages that are gone leaves now that the directory is
    // scanned and the volume is mounted (D10, P8), as on a device — but
    // only when the simulator models a device's directory (`--system-apps`):
    // `sim.sh --app X` alone installs X and nothing else, and sweeping every
    // other app's data on each such run would make switching apps in the
    // simulator lose it.
    #[cfg(all(has_multi_app, feature = "littlefs"))]
    {
        if std::env::var("PICODROID_SYSTEM_APKS").is_ok_and(|v| !v.is_empty()) {
            let _ = crate::storage::sweep_orphans();
        }
    }
    allocator::checkpoint("post-app-region");

    run(model);

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

    // The scheduler ended because the bridge installed or uninstalled an
    // app and asked for the device's reset. Every task has stopped, so the
    // region can be dumped and the process replaced.
    if crate::hal::sim::pdb::take_reboot_request() {
        crate::hal::sim::pdb::reboot();
    }
    crate::hal::sim::pdb::shutdown();
}

/// Create the boot tasks, then hand this thread to the scheduler.
///
/// Returns when the JVM task has run out of apps and ended the scheduler.
fn run(model: &'static BootBudgetModel) {
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

    // The debug bridge, where a device creates it: after the pool, before
    // the JVM task, so the arena's first-fit placement sees the same order.
    // Its priority outranks the JVM's, as on a device (`task_priority`).
    let bridge = TaskSpec {
        name: "pdb",
        kind: TaskKind::DebugBridge,
        priority: crate::task_priority::PRIORITY_RT_1,
        stack_bytes: None, // platform's DebugBridge default (boot budget)
    };
    assert!(
        rtos::spawn(
            &bridge,
            Box::new(|| {
                use crate::hal::sim::pdb::{SimCoordinator, SimPapkFlash, SimSysmon, SimTransport};
                crate::pdb::run_pdb_task(
                    SimTransport::new(),
                    SimCoordinator,
                    SimSysmon,
                    SimPapkFlash,
                )
            }),
        ),
        "pdb task"
    );

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
                // The app-switching loop a device's supervisor runs
                // (platforms/rp/src/boot_tasks.rs), install park included.
                crate::hal::sim::pdb::register_jvm_task();
                // A device with nothing to run waits for an install. The
                // simulator exits instead (module doc), unless asked not to.
                let wait_for_install =
                    std::env::var("PICODROID_SIM_WAIT_FOR_INSTALL").is_ok_and(|v| v == "1");
                let mut image = crate::packages::boot_image();
                if image.is_none() {
                    println!("[sim] no app to run");
                }
                loop {
                    match image {
                        Some(img) => {
                            crate::hal::sim::platform::set_stop_jvm(false);
                            // Unwinding across the port's `extern "C"` task
                            // trampoline is UB, and abort-on-panic is what a
                            // device does under panic-probe. Catch here
                            // rather than letting the default hook run, so
                            // the scheduler is not left owning a dead
                            // process's main thread.
                            if std::panic::catch_unwind(AssertUnwindSafe(|| {
                                crate::boot::run_app(img)
                            }))
                            .is_err()
                            {
                                eprintln!("[sim] jvm task panicked — aborting");
                                std::process::abort();
                            }

                            // One app runs at a time: Java threads the app
                            // left behind end here, exactly as a device's
                            // supervisor loop stops them before the next app
                            // (or an install) — STOP_JVM, wake the parked
                            // ones, wait. Without it an `onCreate` that
                            // starts threads and returns would end the
                            // scheduler out from under children that never
                            // ran an instruction.
                            //
                            // The device's wait, not a poll: the last child
                            // to leave notifies this task
                            // (`rtos_freertos::child_gone`), and the loop
                            // re-checks the count because a notification is
                            // "look again" — this task collects ones it never
                            // asked for (`boot_tasks.rs`, porting-seam A9).
                            crate::hal::sim::platform::set_stop_jvm(true);
                            crate::threads::wake_all_parked();
                            while sim_rtos::live_jvm_children() > 0 {
                                rtos::task_wait_notification(rtos::Timeout::Forever);
                            }
                        }
                        None => {
                            if !wait_for_install {
                                break;
                            }
                            // The device's idle state: nothing runs, `pdb
                            // list` names no running app, and the next
                            // thing to happen is an install's park request.
                            crate::packages::set_running(None);
                            println!("[sim] nothing to run; waiting for an install");
                            while !crate::hal::sim::pdb::park_requested() {
                                rtos::task_wait_notification(rtos::Timeout::Forever);
                            }
                        }
                    }

                    // A package verb that had to wait for the app to stop
                    // (reinstall or uninstall of the running package).
                    let directory = crate::packages::directory_generation();
                    crate::hal::sim::app_region::service_deferred();

                    // The device's park point (boot_tasks.rs): an app stopped
                    // for an install falls through to the park and keeps
                    // `image`, so a refused install resumes the same app; any
                    // other exit asks the directory what runs next.
                    if !crate::hal::sim::pdb::park_requested() {
                        image = crate::packages::next_image();
                        continue;
                    }
                    crate::hal::sim::pdb::park_until_released();
                    // Only a refused install returns here (a completed one
                    // reboots the process). The app's run is where it was
                    // unless the directory moved under the park — a
                    // compaction, or the erase of an in-place copy — in which
                    // case find the package again by name.
                    if crate::packages::directory_generation() != directory {
                        image = crate::packages::running()
                            .and_then(crate::packages::find)
                            .map(|e| e.image)
                            .or_else(crate::packages::next_image);
                    }
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
