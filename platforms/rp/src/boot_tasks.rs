// SPDX-License-Identifier: GPL-3.0-only
//! Task topology and the JVM supervisor loop.
//!
//! This is bin-layer code, not HAL. It reaches into `picodroid_core::fs`,
//! `crate::pdb::pending` and `crate::boot_budget`, none of which a hardware
//! abstraction has any business knowing; it lived under `hal/rp/` only
//! because it starts the scheduler next to `clock_init`.
//!
//! # The supervisor loop stays here on purpose
//!
//! Everything in the JVM task below the spawn — clear the stop flag, run the
//! app, wake sleeping children, drain them, then either park for a flash
//! write or wait for the next install — is framework lifecycle with no
//! silicon in it, and getting it wrong means a PDB install silently corrupts
//! the app. It is tempting to hoist into `picodroid_core::boot`.
//!
//! It is not hoisted because the park half exists for a reason specific to
//! this family: it executes in place from the very flash an installer erases,
//! on a core that must therefore be stopped first. Generalising that needs
//! five or more required hooks — clear stop, abort child delays, drain
//! children, park request/ack, wait — and would encode *this* flash topology
//! into `PlatformHooks` on a sample size of one. A dual-bank or run-from-RAM
//! family has a structurally different loop.
//!
//! What a second family owes here is therefore a checklist rather than an
//! interface, and this is the reference implementation for it. Revisit when
//! there is a second family to compare against — see
//! `docs/designs/family-neutral-residue.md` D4.

use freertos_rust::*;

use crate::task_affinity;
use crate::task_priority;

/// Create FreeRTOS tasks and start the scheduler (never returns).
///
/// RP2040/RP2350 are dual-core; every task goes through
/// `task_affinity::spawn`, which names its core (see that module for the
/// invariant and its guard):
///   - flashpark on core 1, cyw43 on core 1 (network boards)
///   - pdb, fs, sensor, jvm-bg workers and the JVM task on core 0
pub fn start_tasks(boot_apk: &'static [u8]) -> ! {
    // Installed before the first task exists: `task_affinity::spawn` runs
    // every create+pin inside this section (a no-op until the scheduler
    // starts, load-bearing for every runtime `Thread.start` after it). The
    // installer is core's, shared with the simulator's boot, so the two make
    // the same promise; its docs carry the SMP reasoning.
    picodroid_core::rtos::freertos::install_heap_atomic_hooks();

    // Core-1 flash parker.  Runtime flash erase/program must stop core 1
    // first: any exception it takes during the XIP-off window fetches
    // vectors and handler code from disconnected flash and locks the core
    // up (RP40-1/-2 on rp2040 via the tick, which runs there; on rp2350 via
    // the PendSV that core 0's yield IPI raises).  Created first and
    // registered before the scheduler starts so every runtime flash path
    // finds it; see hal/rp/core1_park.rs for the handshake.
    {
        let parker = task_affinity::spawn(
            "flashpark",
            crate::boot_budget::FLASHPARK_STACK_WORDS,
            task_priority::PRIORITY_FLASH_PARK,
            task_affinity::CORE1,
            move |_| crate::hal::core1_park::run_parker_task(),
        )
        .unwrap();
        crate::hal::core1_park::set_parker_task(parker);
    }

    // fs-worker: serialises all LittleFS access onto one core-0-pinned task.
    // Must be spawned after fs::init() (done in main) and before the
    // scheduler starts; any Java thread calling fs::with_fs blocks until
    // this task processes its request.
    picodroid_core::fs::spawn_worker();

    // Sensor sampler: owns all sensor I²C off the JVM/UI task. Spawned
    // pre-scheduler so its i2c::init calls can't race Java I2cDevice users
    // (init's check-then-act is not task-safe; per-transfer bus locks take
    // over once initialised).
    #[cfg(any_sensor)]
    picodroid_core::hardware::sensors::sampler::spawn();

    // Background thread pool: pre-spawns the workers that back
    // `Executors.backgroundExecutor()`. Each worker parks on the shared
    // work queue and lazily constructs its own Jvm on first submit so it
    // picks up the registered class loader from `run_jvm_with`.
    // The loop each worker runs lives in the platform crate (it needs the
    // app's class loader and shared heap), so install it before spawning.
    picodroid_core::bg_worker::install();
    picodroid_core::executors::background_pool::spawn();

    // pdb listener on USB CDC. Priority 2 preempts jvm_task (priority 1).
    // Pinned to core 0: on RP2350, cross-core SRAM visibility between
    // core 0 and core 1 is unreliable for flow-control counters shared
    // between the USB ISR and the PDB task.  Keeping both on core 0
    // avoids the issue entirely.
    task_affinity::spawn(
        "pdb",
        crate::boot_budget::PDB_STACK_WORDS,
        task_priority::PRIORITY_RT_1,
        task_affinity::CORE0,
        move |_| crate::pdb::run_pdb_task(),
    )
    .unwrap();

    // The network link task (Pico 2 W only): core's `run_link_task` drives
    // this family's cyw43 link driver — driver init, MAC, IP stack start,
    // WiFi join, then the driver poll loop.
    //
    // Pinned to core 1 so core 0 stays free for the JVM, and because
    // `Cyw43Link::init` arms the host-wake IRQ on the calling core's NVIC
    // bank (the RP2350 banks them per core).  This is safe under
    // real SMP (configRUN_MULTIPLE_PRIORITIES=1) because the gSPI transport
    // is PIO+DMA (`hal/rp/pio_spi.rs`): bus frames are clocked by the PIO
    // state machine and fed by DMA, so they complete autonomously no matter
    // what either CPU core executes — unlike the deleted bit-bang, whose
    // instruction-timed clock broke the moment a loaded core 0 contended for
    // XIP (DHCP stuck at 0.0.0.0; see docs/designs/cyw43-pio-transport.md).
    //
    // Core 1 is shared with the flash parker (priority 30 > 22): a park
    // mid-transfer only delays the CS deassert past an already-completed
    // frame, which the chip tolerates.
    #[cfg(network_cyw43)]
    task_affinity::spawn(
        "cyw43",
        crate::boot_budget::CYW43_STACK_WORDS,
        task_priority::PRIORITY_RT_2,
        task_affinity::CORE1,
        move |_| {
            picodroid_core::hal::freertos_tcp::run_link_task(crate::hal::cyw43::link::Cyw43Link)
        },
    )
    .unwrap();

    // JVM task: runs the app in a loop, rebooting when a new install arrives.
    // Pinned to core 0; all JVM child threads are also pinned to core 0 so the
    // single-core safety assumption of SharedJvmState remains valid.
    // RP2350 (Cortex-M33, FPU) needs a larger stack than RP2040 (Cortex-M0+)
    // because each interrupt pushes an extended exception frame (~100 bytes)
    // when the FPU has been used (configENABLE_FPU=1). Size lives in
    // boot_budget so the sim pre-charges the identical amount (M4).
    task_affinity::spawn(
        "jvm",
        crate::boot_budget::JVM_STACK_WORDS,
        task_priority::PRIORITY_JVM_NORM,
        task_affinity::CORE0,
        move |_| {
            // Store our handle so pdb_task and child tasks can notify us.
            crate::pdb::pending::set_jvm_task(Task::current().unwrap());
            loop {
                crate::pdb::pending::clear_stop();
                crate::app::run_jvm_with(boot_apk);

                // Wake any child threads sleeping in vTaskDelay so they see STOP_JVM,
                // and end every Thread.sleep / join / Object.wait park the same way.
                crate::pdb::pending::abort_all_child_delays();
                picodroid_core::threads::wake_all_parked();

                // Every wait below re-checks the condition it is waiting for.
                // That is the rtos seam's contract for task notifications —
                // "look again", not a credit — and this task collects
                // notifications it never asked for: the fs worker outranks
                // us, so `SerialWorker::submit` returns with the completion
                // notification still latched on us, once per `with_fs` call.
                // A bare wait here returned at once, and `bootcount` re-ran
                // `Application.onCreate` seven times a second
                // (docs/designs/porting-seam-2026-09.md A9).

                // Wait for all child threads to deregister; the last one
                // notifies us.
                while crate::pdb::pending::ACTIVE_JVM_THREADS
                    .load(core::sync::atomic::Ordering::Acquire)
                    > 0
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // Natural app exit: nothing to do until pdb_task installs a
                // new app, and an install always opens with a park request
                // (`CoreCoordinator::request_stop_and_park`), so that is the
                // condition. An app that was stopped for an install already
                // has the request pending and falls straight through.
                while !crate::pdb::pending::FLASH_PARK_REQUESTED
                    .load(core::sync::atomic::Ordering::Acquire)
                {
                    CurrentTask::take_notification(true, Duration::infinite());
                }

                // pdb_task requested a flash install: park core 0. Signal
                // CORE0_PARKED then block. PDB (higher priority on the same
                // core) performs the flash write and either resets the chip
                // or, on the error path, clears CORE0_PARKED and wakes us —
                // then we restart the JVM loop.
                crate::pdb::pending::CORE0_PARKED
                    .store(true, core::sync::atomic::Ordering::Release);
                while crate::pdb::pending::CORE0_PARKED.load(core::sync::atomic::Ordering::Acquire)
                {
                    let _ = CurrentTask::take_notification(true, Duration::infinite());
                }
            }
        },
    )
    .unwrap();

    FreeRtosUtils::start_scheduler()
}
