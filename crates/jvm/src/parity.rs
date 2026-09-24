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
static RESOLVES: AtomicUsize = AtomicUsize::new(0);
static CACHE_DECLINES: AtomicUsize = AtomicUsize::new(0);
static NATIVE_US: AtomicUsize = AtomicUsize::new(0);
static NATIVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static RESOLVE_US: AtomicUsize = AtomicUsize::new(0);
static CLINIT_US: AtomicUsize = AtomicUsize::new(0);
static INVOKE_US: AtomicUsize = AtomicUsize::new(0);
static FIELDS_US: AtomicUsize = AtomicUsize::new(0);
static NEW_US: AtomicUsize = AtomicUsize::new(0);
static OTHER_US: AtomicUsize = AtomicUsize::new(0);
static FIELD_OPS: AtomicUsize = AtomicUsize::new(0);
static INVOKES: AtomicUsize = AtomicUsize::new(0);
static FRAME_US: AtomicUsize = AtomicUsize::new(0);
static FASTEST_US: AtomicUsize = AtomicUsize::new(usize::MAX);

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

/// One resolution the executor's caches did not answer: a method walk, an
/// instance-field slot search or a static-field index search. Every
/// top-level invocation starts with empty caches, so this counts the
/// class-walking work a span really did, which is what a device pays for
/// at some microseconds each.
#[inline(always)]
pub fn count_resolve() {
    RESOLVES.fetch_add(1, Ordering::Relaxed);
}

/// One cache entry the heap refused to store (`helpers::cache_push`).
#[inline(always)]
pub fn count_cache_decline() {
    CACHE_DECLINES.fetch_add(1, Ordering::Relaxed);
}

static SLOWEST_US: AtomicUsize = AtomicUsize::new(0);
static SLOWEST_CLASS: AtomicUsize = AtomicUsize::new(0);
static SLOWEST_CLASS_LEN: AtomicUsize = AtomicUsize::new(0);
static SLOWEST_METHOD: AtomicUsize = AtomicUsize::new(0);
static SLOWEST_METHOD_LEN: AtomicUsize = AtomicUsize::new(0);

/// One native call that took `ns` on the handler's clock. Microseconds in
/// a `usize`, so a 32-bit device wraps after 71 minutes — readers take
/// wrapping deltas, never absolutes. Also keeps the slowest call since
/// [`reset_slowest_native`], by name, so a slow span can say whether one
/// native dominated it.
#[inline(always)]
pub fn count_native(ns: u64, class_name: &str, method_name: &str) {
    let us = (ns / 1_000) as usize;
    NATIVE_US.fetch_add(us, Ordering::Relaxed);
    NATIVE_CALLS.fetch_add(1, Ordering::Relaxed);
    if us < FASTEST_US.load(Ordering::Relaxed) {
        FASTEST_US.store(us, Ordering::Relaxed);
    }
    if us > SLOWEST_US.load(Ordering::Relaxed) {
        SLOWEST_US.store(us, Ordering::Relaxed);
        SLOWEST_CLASS.store(class_name.as_ptr() as usize, Ordering::Relaxed);
        SLOWEST_CLASS_LEN.store(class_name.len(), Ordering::Relaxed);
        SLOWEST_METHOD.store(method_name.as_ptr() as usize, Ordering::Relaxed);
        SLOWEST_METHOD_LEN.store(method_name.len(), Ordering::Relaxed);
    }
}

/// Forget the slowest native seen so far (a span is starting).
pub fn reset_slowest_native() {
    SLOWEST_US.store(0, Ordering::Relaxed);
    FASTEST_US.store(usize::MAX, Ordering::Relaxed);
}

/// The slowest native call since the last reset: (microseconds, class,
/// method). The names are the interpreter's own: constant-pool bytes of a
/// loaded class or a `names::c` constant, both alive as long as the app is,
/// which is the lifetime of anyone reading a diagnostic about it. Empty
/// names when nothing was recorded.
pub fn slowest_native() -> (usize, &'static str, &'static str) {
    let us = SLOWEST_US.load(Ordering::Relaxed);
    if us == 0 {
        return (0, "", "");
    }
    // SAFETY: the pointers were taken from `&str`s that borrow class data
    // registered for the life of the JVM (see above); the lengths were
    // stored with them. A torn read across two threads could pair a class
    // pointer with the wrong length, so both are re-checked for UTF-8.
    let name = |ptr: usize, len: usize| unsafe {
        core::str::from_utf8(core::slice::from_raw_parts(ptr as *const u8, len)).unwrap_or("?")
    };
    (
        us,
        name(
            SLOWEST_CLASS.load(Ordering::Relaxed),
            SLOWEST_CLASS_LEN.load(Ordering::Relaxed),
        ),
        name(
            SLOWEST_METHOD.load(Ordering::Relaxed),
            SLOWEST_METHOD_LEN.load(Ordering::Relaxed),
        ),
    )
}

/// Time spent resolving a method or field site (cache probe plus the walk
/// on a miss), on the handler's clock.
#[inline(always)]
pub fn count_resolve_time(ns: u64) {
    RESOLVE_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
}

/// Time spent in `ensure_class_initialized` checks (the initialised-set
/// probe; a real `<clinit>` runs as frames and is not inside this).
#[inline(always)]
pub fn count_clinit_time(ns: u64) {
    CLINIT_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
}

/// One invoke-family opcode, `ns` from dispatch to return (resolution,
/// argument transfer, a native's body or a Java frame push — everything).
#[inline(always)]
pub fn count_invoke(ns: u64) {
    INVOKE_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
    INVOKES.fetch_add(1, Ordering::Relaxed);
}

/// Time to build one Java frame (two fallible buffer reservations).
#[inline(always)]
pub fn count_frame_time(ns: u64) {
    FRAME_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
}

/// A field opcode (`getfield`/`putfield`/`getstatic`/`putstatic`).
#[inline(always)]
pub fn count_fields(ns: u64) {
    FIELDS_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
    FIELD_OPS.fetch_add(1, Ordering::Relaxed);
}

pub fn field_ops() -> usize {
    FIELD_OPS.load(Ordering::Relaxed)
}

/// A `new`.
#[inline(always)]
pub fn count_new(ns: u64) {
    NEW_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
}

/// Any other opcode's handler.
#[inline(always)]
pub fn count_other(ns: u64) {
    OTHER_US.fetch_add((ns / 1_000) as usize, Ordering::Relaxed);
}

pub fn fields_us() -> usize {
    FIELDS_US.load(Ordering::Relaxed)
}

pub fn new_us() -> usize {
    NEW_US.load(Ordering::Relaxed)
}

pub fn other_us() -> usize {
    OTHER_US.load(Ordering::Relaxed)
}

pub fn invoke_us() -> usize {
    INVOKE_US.load(Ordering::Relaxed)
}

pub fn invokes() -> usize {
    INVOKES.load(Ordering::Relaxed)
}

pub fn frame_us() -> usize {
    FRAME_US.load(Ordering::Relaxed)
}

/// The quickest native call since the last reset, in microseconds: a
/// trivial getter's time is the handler's fixed dispatch cost.
pub fn fastest_native_us() -> usize {
    let v = FASTEST_US.load(Ordering::Relaxed);
    if v == usize::MAX {
        0
    } else {
        v
    }
}

pub fn resolve_us() -> usize {
    RESOLVE_US.load(Ordering::Relaxed)
}

pub fn clinit_us() -> usize {
    CLINIT_US.load(Ordering::Relaxed)
}

pub fn native_us() -> usize {
    NATIVE_US.load(Ordering::Relaxed)
}

pub fn native_calls() -> usize {
    NATIVE_CALLS.load(Ordering::Relaxed)
}

pub fn resolves() -> usize {
    RESOLVES.load(Ordering::Relaxed)
}

pub fn cache_declines() -> usize {
    CACHE_DECLINES.load(Ordering::Relaxed)
}

pub fn insns() -> usize {
    INSNS.load(Ordering::Relaxed)
}

pub fn allocs() -> usize {
    ALLOCS.load(Ordering::Relaxed)
}
