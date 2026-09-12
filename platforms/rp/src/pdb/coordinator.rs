// SPDX-License-Identifier: GPL-3.0-only
//! Parking the JVM core so flash can be written under it.
//!
//! Stays family-side deliberately. The handshake exists because this family
//! executes in place from the very flash an install erases, on a core that
//! must therefore be stopped first — a dual-bank or run-from-RAM family has a
//! structurally different problem, and generalising this one on a sample size
//! of one would bake this flash topology into a shared trait.

use core::sync::atomic::Ordering;

use freertos_rust::{CurrentTask, Duration};
use picodroid_core::install::CoreCoordinator;

use super::pending;

// ── PDB core coordinator ────────────────────────────────────────────────────

/// Coordinates JVM stop and core-0 parking using the PDB pending flags.
pub struct PdbCoreCoordinator;

impl CoreCoordinator for PdbCoreCoordinator {
    fn request_stop_and_park(&mut self) {
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);

        pending::set_stop_jvm();
        // Relaxed is fine: PDB and JVM share core 0, so the store is
        // immediately visible without a cross-core barrier.
        pending::FLASH_PARK_REQUESTED.store(true, Ordering::Relaxed);
        // We intentionally do NOT call abort_jvm_delay() here.  PDB and
        // JVM share core 0, so PDB (higher priority) must yield via
        // notify_jvm() to let the JVM task run and observe STOP_JVM.
        pending::notify_jvm();
    }

    // `inline(never)`: the generic install/uninstall code calls this at
    // several sites, and an inlined copy of the wait loop at each cost
    // ~600 B of RP2040 flash where the old body was one 48 B function.
    #[inline(never)]
    fn wait_for_park(&mut self) -> bool {
        // PDB and JVM share core 0: this task, the higher-priority one, has
        // to block for the JVM task to run and park. jvm_task notifies us the
        // moment it sets CORE0_PARKED; the flag is re-checked on every wake
        // because a notification is "look again", not a credit. Fifteen
        // seconds in all, as before, but woken at once instead of on a 10 ms
        // poll (docs/scheduling-audit-2026-09.md, F12).
        // `black_box` on the bound: LLVM otherwise unrolls the loop and
        // lays the notification call, the tick-period lookup and its
        // division out fifteen times (660 B of RP2040 flash for this body).
        for _ in 0..core::hint::black_box(15u32) {
            if pending::CORE0_PARKED.load(Ordering::Acquire) {
                return true;
            }
            let _ = CurrentTask::take_notification(true, Duration::ms(1000));
        }
        pending::CORE0_PARKED.load(Ordering::Acquire)
    }

    fn release(&mut self) {
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);
        pending::notify_jvm();
    }

    fn cancel_park_request(&mut self) {
        pending::FLASH_PARK_REQUESTED.store(false, Ordering::Release);
    }
}
