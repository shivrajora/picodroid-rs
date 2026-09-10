// SPDX-License-Identifier: GPL-3.0-only
//! Deterministic execution counters for simulator ↔ device parity
//! comparison (docs/parity-audit.md P1, `parity-metrics` feature).
//!
//! These count *work performed* — bytecode dispatches and JVM allocations —
//! which is identical between simulator and hardware whenever the runtime
//! truly behaves the same. Cross-environment checks assert these for
//! **equality**; wall-clock time never enters the comparison (a host CPU
//! predicts nothing about a Cortex-M).
//!
//! `AtomicUsize` keeps thumbv6m compatibility (no 64-bit atomics there):
//! 32-bit devices wrap at ~4.3e9 instructions, far beyond any parity scene;
//! documented in the audit's honest-limits section.

use core::sync::atomic::{AtomicUsize, Ordering};

static INSNS: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

/// One bytecode dispatch. Called from the interpreter main loop.
#[inline(always)]
pub fn count_insn() {
    INSNS.fetch_add(1, Ordering::Relaxed);
}

/// `n` JVM allocations (objects, arrays, dynamic strings). Called from the
/// single `bump_alloc_count` funnel.
#[inline(always)]
pub fn count_allocs(n: usize) {
    ALLOCS.fetch_add(n, Ordering::Relaxed);
}

pub fn insns() -> usize {
    INSNS.load(Ordering::Relaxed)
}

pub fn allocs() -> usize {
    ALLOCS.load(Ordering::Relaxed)
}
