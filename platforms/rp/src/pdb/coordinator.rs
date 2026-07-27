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

    fn wait_for_park(&mut self) -> bool {
        // PDB and JVM share core 0.  PDB (higher priority) must yield
        // so the JVM task can run and exit.
        for _ in 0..1500u32 {
            if pending::CORE0_PARKED.load(Ordering::Acquire) {
                return true;
            }
            CurrentTask::delay(Duration::ms(10));
        }
        false
    }

    fn release(&mut self) {
        pending::CORE0_PARKED.store(false, Ordering::Relaxed);
        pending::notify_jvm();
    }

    fn cancel_park_request(&mut self) {
        pending::FLASH_PARK_REQUESTED.store(false, Ordering::Release);
    }
}
