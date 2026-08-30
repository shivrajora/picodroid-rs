// SPDX-License-Identifier: GPL-3.0-only
//! Scheduler-atomic sections for compound heap mutations.
//!
//! The FreeRTOS SMP kernel yields to an equal-priority task the moment it
//! is unblocked (`prvYieldForTask` uses `uxPriority >=`), and the global
//! allocator's `xTaskResumeAll` exit is a yield point — so any compound
//! heap operation that itself allocates (arena `resize`/`try_reserve`, GC
//! scratch-Vec growth) can be interleaved by another JVM task mid-sequence
//! even with every JVM task pinned to one core at one priority with time
//! slicing off. One observed interleave: two tasks read the same arena
//! length in `alloc_span`; the loser's `resize` then shrinks the arena over
//! the winner's fresh span, orphaning its descriptor past the tail —
//! overlapping spans, compaction range panics, mid-GC sweeps of rooted
//! objects (picoenvmon P1, docs/picoenvmon-qa.md 2026-08-17).
//!
//! The platform installs suspend/resume hooks (`vTaskSuspendAll`/
//! `xTaskResumeAll` on both the device and the simulator's hosted kernel;
//! they nest with the allocator's own suspension so the inner resume never
//! yields); an [`AtomicSection`] guard brackets each compound operation.
//! With no hooks installed (`cargo test`, where no scheduler runs) the guard
//! is a no-op.
//!
//! Sections must never block: nothing inside a guard may call a blocking
//! RTOS primitive. The guarded regions are short (an arena grow, one GC).

use core::sync::atomic::{AtomicUsize, Ordering};

static ENTER_FN: AtomicUsize = AtomicUsize::new(0);
static EXIT_FN: AtomicUsize = AtomicUsize::new(0);

/// Install the platform's scheduler suspend/resume pair. Call once, before
/// any JVM task runs.
pub fn set_hooks(enter: fn(), exit: fn()) {
    ENTER_FN.store(enter as usize, Ordering::Release);
    EXIT_FN.store(exit as usize, Ordering::Release);
}

/// RAII scheduler-atomic section. See the module docs.
pub struct AtomicSection;

impl AtomicSection {
    #[inline]
    pub fn enter() -> Self {
        let p = ENTER_FN.load(Ordering::Relaxed);
        if p != 0 {
            // SAFETY: only ever stored from a valid `fn()` by set_hooks.
            let f: fn() = unsafe { core::mem::transmute(p) };
            f();
        }
        Self
    }
}

impl Drop for AtomicSection {
    #[inline]
    fn drop(&mut self) {
        let p = EXIT_FN.load(Ordering::Relaxed);
        if p != 0 {
            // SAFETY: as above.
            let f: fn() = unsafe { core::mem::transmute(p) };
            f();
        }
    }
}
