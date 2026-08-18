// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    array_heap::{ArrayHeap, ATYPE_REF},
    class_objects::ClassObjectCache,
    frame::Frame,
    heap::StringTable,
    object_heap::ObjectHeap,
    static_fields::StaticFieldStore,
    types::Value,
};
use alloc::vec::Vec;

#[cfg(test)]
mod tests;

// ── Bitset helpers ───────────────────────────────────────────────────────────

fn mark_bit(bits: &mut Vec<u8>, idx: u16) {
    let i = idx as usize;
    let byte = i / 8;
    let bit = i % 8;
    if byte >= bits.len() {
        bits.resize(byte + 1, 0);
    }
    bits[byte] |= 1 << bit;
}

fn is_marked(bits: &[u8], idx: u16) -> bool {
    let i = idx as usize;
    let byte = i / 8;
    let bit = i % 8;
    byte < bits.len() && (bits[byte] & (1 << bit)) != 0
}

// ── Work stack item ──────────────────────────────────────────────────────────

enum GcRef {
    Object(u16),
    Array(u16),
    String(u16),
}

/// Push a GcRef onto the work stack if `v` is a reference type.
fn push_ref(work: &mut Vec<GcRef>, v: &Value) {
    match *v {
        Value::ObjectRef(idx) => work.push(GcRef::Object(idx)),
        Value::ArrayRef(idx) => work.push(GcRef::Array(idx)),
        Value::Reference(idx) => work.push(GcRef::String(idx)),
        _ => {}
    }
}

// ── Persistent GC state ─────────────────────────────────────────────────────

/// Reusable buffers for the mark-sweep collector.
///
/// Persisting these across GC cycles avoids allocating and freeing four Vecs
/// on every collection, which was a major source of FreeRTOS heap fragmentation.
pub struct GcState {
    obj_marks: Vec<u8>,
    arr_marks: Vec<u8>,
    str_marks: Vec<u8>,
    work: Vec<GcRef>,
    /// Scratch buffer for arena compaction: (slot_index, arena_offset, length).
    arena_compact_buf: Vec<(usize, u32, u16)>,
    /// Frame registry: every `interpreter::execute` on this heap registers
    /// its frame stack here for its whole run. A JVM task parked at a
    /// blocking yield point (sleep, socket accept/recv, queue wait) is
    /// mid-`execute` with live frames that the *collecting* task cannot see
    /// through its own `frames` argument — without this registry those
    /// locals get swept (first hit: the picoenvmon network thread's
    /// HttpServer, collected while the thread was parked in accept).
    ///
    /// Raw pointers, walked unsafely in [`collect`]: sound because the
    /// whole collection runs inside an [`crate::atomic_section`] guard
    /// (scheduler suspension) — parked tasks cannot resume while the
    /// collector runs, and registration/deregistration happen in the
    /// owning task's slice. (The former justification — single core plus
    /// `configUSE_TIME_SLICING = 0` — was insufficient: the SMP kernel's
    /// equal-priority wake yield preempted mid-collection; see the
    /// atomic_section module docs.) Entries can include the collector's
    /// own stack; re-visiting is harmless (marking is idempotent). A panic
    /// mid-`execute` leaks its entry — acceptable: panics are fatal on
    /// target.
    parked_frames: Vec<*const Vec<Frame>>,
    /// Allocations since the last GC, persistent across `execute()` calls so
    /// long native-driven callback bursts (e.g. sensor delivery) still trip
    /// the GC threshold instead of resetting to 0 on every fresh `Executor`.
    pub alloc_count: u16,
    /// Set when an allocator returned None (heap full). Cleared at the next
    /// GC. Like `alloc_count`, persists across `execute()` calls.
    pub need_gc: bool,
    /// Cumulative JVM allocations since boot (objects, arrays, dyn strings),
    /// for the mem-diag monitor's per-window churn deltas. Plain non-atomic
    /// field: only the thread that owns the heap mutates it, and thumbv6m
    /// has no atomic RMW. Distinct from `parity::ALLOCS` (atomic,
    /// sim<->device equality-checked, never reset) — do not merge them.
    #[cfg(feature = "mem-diag")]
    pub alloc_total: u32,
    /// live_bytes sum sampled right after the most recent GC sweep — the
    /// leak-detection floor (excludes not-yet-collected garbage).
    #[cfg(feature = "mem-diag")]
    pub last_post_gc_live: u32,
    /// Minimum post-GC live_bytes since the monitor last drained the window
    /// (`u32::MAX` = no GC ran in the current window).
    #[cfg(feature = "mem-diag")]
    pub min_post_gc_live: u32,
    /// Cumulative GC cycles on this heap since boot. Lives here — on the
    /// heap-wide `GcState` — rather than the per-executor native handler,
    /// so collections run by `Thread.start` children reach the mem-diag
    /// monitor; the per-handler counters missed them and `[memmon]`
    /// reported `gc=+0` under background-thread churn
    /// (docs/perf-memory-handover-2026-08.md §3). Plain non-atomic fields
    /// like `alloc_total`: only the task that owns the heap mutates them.
    #[cfg(feature = "mem-diag")]
    pub gc_runs_total: u32,
    /// Cumulative heap entries (objects + arrays + dyn strings) freed by GC.
    #[cfg(feature = "mem-diag")]
    pub gc_freed_entries_total: u32,
    /// Cumulative live-bytes reclaimed by GC (pre-sweep minus post-sweep).
    #[cfg(feature = "mem-diag")]
    pub gc_bytes_reclaimed_total: u32,
}

impl GcState {
    pub const fn new() -> Self {
        Self {
            obj_marks: Vec::new(),
            arr_marks: Vec::new(),
            str_marks: Vec::new(),
            work: Vec::new(),
            arena_compact_buf: Vec::new(),
            parked_frames: Vec::new(),
            alloc_count: 0,
            need_gc: false,
            #[cfg(feature = "mem-diag")]
            alloc_total: 0,
            #[cfg(feature = "mem-diag")]
            last_post_gc_live: 0,
            #[cfg(feature = "mem-diag")]
            min_post_gc_live: u32::MAX,
            #[cfg(feature = "mem-diag")]
            gc_runs_total: 0,
            #[cfg(feature = "mem-diag")]
            gc_freed_entries_total: 0,
            #[cfg(feature = "mem-diag")]
            gc_bytes_reclaimed_total: 0,
        }
    }

    /// Register a running `execute()`'s frame stack as a GC root source.
    /// Paired with [`Self::unregister_frames`] on `execute` exit.
    pub fn register_frames(&mut self, frames: *const Vec<Frame>) {
        self.parked_frames.push(frames);
    }

    /// Remove a frame stack registered by [`Self::register_frames`].
    /// Removes the most recent matching entry (registration is stack-like
    /// under nested `execute` calls).
    pub fn unregister_frames(&mut self, frames: *const Vec<Frame>) {
        if let Some(i) = self.parked_frames.iter().rposition(|&p| p == frames) {
            self.parked_frames.remove(i);
        }
    }

    /// Record a completed GC cycle: the post-sweep live_bytes floor (mem-diag
    /// monitor leak tracking) plus this cycle's contribution to the
    /// cumulative run/freed/reclaimed counters. Called at both GC sites: the
    /// interpreter safepoint and `SharedJvmHeap::collect_now` — from every
    /// executor, so child-thread collections are counted too.
    #[cfg(feature = "mem-diag")]
    pub fn note_gc_cycle(&mut self, freed_entries: usize, pre_bytes: usize, post_bytes: usize) {
        let b = post_bytes.min(u32::MAX as usize) as u32;
        self.last_post_gc_live = b;
        if b < self.min_post_gc_live {
            self.min_post_gc_live = b;
        }
        self.gc_runs_total = self.gc_runs_total.wrapping_add(1);
        self.gc_freed_entries_total = self
            .gc_freed_entries_total
            .wrapping_add(freed_entries.min(u32::MAX as usize) as u32);
        self.gc_bytes_reclaimed_total = self
            .gc_bytes_reclaimed_total
            .wrapping_add(pre_bytes.saturating_sub(post_bytes).min(u32::MAX as usize) as u32);
    }

    /// Drain the current monitor window: returns the post-GC live floor for
    /// the window that just ended and resets the windowed minimum. `None`
    /// when no GC has run yet at all (caller should fall back to the raw
    /// live_bytes sum, which is exact while nothing has been freed).
    #[cfg(feature = "mem-diag")]
    pub fn take_window_post_gc_floor(&mut self) -> Option<u32> {
        let gc_ran_this_window = self.min_post_gc_live != u32::MAX;
        let floor = if gc_ran_this_window {
            self.min_post_gc_live
        } else {
            self.last_post_gc_live
        };
        self.min_post_gc_live = u32::MAX;
        // last_post_gc_live == 0 with no window GC means no GC ever ran
        // (a genuine post-GC live of 0 only happens on an empty heap, where
        // the raw-live fallback reports the same thing).
        if !gc_ran_this_window && self.last_post_gc_live == 0 {
            None
        } else {
            Some(floor)
        }
    }

    /// Clear all buffers (O(1), no deallocation — capacity is retained).
    fn clear(&mut self) {
        self.obj_marks.clear();
        self.arr_marks.clear();
        self.str_marks.clear();
        self.work.clear();
        // arena_compact_buf is cleared inside compact_arena(), not here.
    }
}

impl Default for GcState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Mark-sweep collector ─────────────────────────────────────────────────────

/// Run a full stop-the-world mark-sweep GC cycle.
///
/// Scans all roots (frame locals/stacks + static fields + cached `Class`
/// objects), transitively marks reachable objects/arrays/strings, then sweeps
/// unreachable heap entries.
///
/// Returns the number of heap entries freed.
#[allow(clippy::too_many_arguments)]
pub fn collect(
    frames: &[Frame],
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    strings: &mut StringTable,
    statics: &StaticFieldStore,
    class_objects: &ClassObjectCache,
    gc: &mut GcState,
    extra_roots: impl Fn(&mut dyn FnMut(Value)),
) -> usize {
    // The whole collection is scheduler-atomic: a mid-GC yield to the other
    // JVM task (SMP equal-priority wake preemption at an internal Vec-growth
    // malloc) let it mutate the heap between mark and compact — sweeping
    // rooted objects and overrunning the compaction cursor. See
    // `atomic_section` module docs.
    let _atomic = crate::atomic_section::AtomicSection::enter();
    gc.clear();
    // Offensive: if the span invariant is already broken on entry, the
    // damage predates this collection — panic here rather than letting
    // compaction crash on it later with the causal context long gone.
    #[cfg(feature = "mem-diag")]
    objects.debug_check_spans("gc-entry");
    let obj_marks = &mut gc.obj_marks;
    let arr_marks = &mut gc.arr_marks;
    let str_marks = &mut gc.str_marks;
    let work = &mut gc.work;
    #[cfg(feature = "mem-diag")]
    let offensive = crate::mem_diag::offensive();

    // ── Phase 1: scan roots ──────────────────────────────────────────────

    // Frame locals and operand stacks — the collecting executor's own stack.
    for frame in frames {
        for v in &frame.locals {
            push_ref(work, v);
        }
        for v in &frame.stack {
            push_ref(work, v);
        }
    }

    // Frame stacks of every other `execute()` in flight on this heap — JVM
    // tasks parked at a blocking yield point still hold live locals (see the
    // `parked_frames` field docs for the safety contract). May re-visit the
    // collector's own registered stack; marking is idempotent.
    for &pf in &gc.parked_frames {
        // SAFETY: the owning task is parked (single core, no time slicing)
        // and cannot mutate its frames while this task runs; the pointer is
        // valid for the duration of that task's `execute()` call, which
        // spans this collection.
        let parked = unsafe { &*pf };
        for frame in parked {
            for v in &frame.locals {
                push_ref(work, v);
            }
            for v in &frame.stack {
                push_ref(work, v);
            }
        }
    }

    // Static fields
    for v in statics.values_iter() {
        push_ref(work, &v);
    }

    // Cached `java.lang.Class` objects — one per loaded class. Each Class
    // object's `String name` field is reached transitively via the standard
    // field-scan in phase 2.
    for obj_ref in class_objects.iter() {
        work.push(GcRef::Object(obj_ref));
    }

    // Native handler roots — Activity stacks, sensor registrations, service
    // bindings, etc. These references live entirely in handler state and
    // would otherwise be invisible to the mark phase.
    extra_roots(&mut |v| push_ref(work, &v));

    // ── Phase 2: mark (transitive closure) ───────────────────────────────

    while let Some(r) = work.pop() {
        match r {
            GcRef::Object(idx) => {
                if is_marked(obj_marks, idx) {
                    continue;
                }
                mark_bit(obj_marks, idx);

                // Scan object fields for outgoing references.
                for v in objects.fields_slice(idx) {
                    // Offensive mode: a live object's field must never hold
                    // the poison pattern written into freed spans — seeing
                    // it means use-after-free or an arena-compaction bug,
                    // caught here at the moment of damage.
                    #[cfg(feature = "mem-diag")]
                    if offensive {
                        if let Value::Int(x) = v {
                            if *x == crate::mem_diag::POISON_I32 {
                                panic!(
                                    "mem-diag: live object {} ({}) field holds poison — \
                                     use-after-free or fields-arena corruption",
                                    idx,
                                    objects.class_name(idx).unwrap_or("?")
                                );
                            }
                        }
                    }
                    push_ref(work, v);
                }

                // ArrayList: scan backing list_bufs for references
                if objects.class_name(idx) == Some("java/util/ArrayList") {
                    if let Some(Value::Int(buf_idx)) = objects.get_field(idx, 0) {
                        for v in objects.list_iter(buf_idx as u16) {
                            push_ref(work, &v);
                        }
                    }
                }

                // HashMap / HashSet: scan backing map_bufs for references
                let cn = objects.class_name(idx);
                if cn == Some("java/util/HashMap") || cn == Some("java/util/HashSet") {
                    if let Some(Value::Int(buf_idx)) = objects.get_field(idx, 0) {
                        for (k, v) in objects.map_iter(buf_idx as u16) {
                            push_ref(work, &k);
                            push_ref(work, &v);
                        }
                    }
                }

                // Lambda proxy: scan captured values for references
                if let Some(lambda) = objects.get_lambda(idx) {
                    for v in &lambda.captures {
                        push_ref(work, v);
                    }
                }

                // Throwable side tables: the constructor message and any
                // suppressed exceptions live in ObjectHeap side tables, not
                // fields — trace them while their owner is live. Suppressed
                // Throwables in particular are usually reachable ONLY through
                // the table once the recording catch block's locals go dead.
                if let Some(msg) = objects.get_exception_message(idx) {
                    work.push(GcRef::String(msg));
                }
                for &t in objects.suppressed_list(idx) {
                    work.push(GcRef::Object(t));
                }
                if let Some(cause) = objects.get_exception_cause(idx) {
                    work.push(GcRef::Object(cause));
                }
            }
            GcRef::Array(idx) => {
                if is_marked(arr_marks, idx) {
                    continue;
                }
                mark_bit(arr_marks, idx);

                // ATYPE_REF arrays hold one of three reference kinds,
                // disambiguated by tag bits — see array_heap::REF_TAG /
                // ARRAY_TAG. Untagged non-negative values are ObjectRefs.
                if arrays.atype(idx) == Some(ATYPE_REF) {
                    for &val in arrays.data_slice(idx) {
                        let u = val as u32;
                        if u & crate::array_heap::REF_TAG != 0 {
                            work.push(GcRef::String((u & !crate::array_heap::REF_TAG) as u16));
                        } else if u & crate::array_heap::ARRAY_TAG != 0 {
                            work.push(GcRef::Array((u & !crate::array_heap::ARRAY_TAG) as u16));
                        } else if val >= 0 {
                            work.push(GcRef::Object(val as u16));
                        }
                    }
                }
            }
            GcRef::String(idx) => {
                // Strings don't reference other objects; just mark.
                mark_bit(str_marks, idx);
            }
        }
    }

    // ── Phase 3: sweep ───────────────────────────────────────────────────

    let mut freed = 0usize;

    // Sweep objects
    for idx in 0..objects.slot_count() {
        let i = idx as u16;
        if objects.is_live(i) && !is_marked(obj_marks, i) {
            // Free ArrayList backing store if applicable
            if objects.class_name(i) == Some("java/util/ArrayList") {
                if let Some(Value::Int(buf_idx)) = objects.get_field(i, 0) {
                    objects.list_free(buf_idx as u16);
                }
            }
            // Free HashMap / HashSet backing store if applicable
            let cn = objects.class_name(i);
            if cn == Some("java/util/HashMap") || cn == Some("java/util/HashSet") {
                if let Some(Value::Int(buf_idx)) = objects.get_field(i, 0) {
                    objects.map_free(buf_idx as u16);
                }
            }
            objects.free_lambda(i);
            objects.iter_free(i);
            objects.free_exception_message(i);
            objects.free_suppressed(i);
            objects.free_exception_cause(i);
            objects.free(i);
            freed += 1;
        }
    }

    // Sweep arrays
    for idx in 0..arrays.slot_count() {
        let i = idx as u16;
        if arrays.is_live(i) && !is_marked(arr_marks, i) {
            arrays.free(i);
            freed += 1;
        }
    }

    // Sweep dynamic strings (static strings are never freed)
    let dyn_start = strings.dyn_start();
    for idx in dyn_start..strings.total_len() {
        let i = idx as u16;
        if strings.is_dyn_live(i) && !is_marked(str_marks, i) {
            strings.free_dyn(i);
            freed += 1;
        }
    }

    // ── Phase 4: compact array arena ─────────────────────────────────────
    arrays.compact_arena(&mut gc.arena_compact_buf);

    // ── Phase 5: compact object fields arena (M6) ────────────────────────
    // Reclaims spans left by swept objects and set_field lazy-grow moves.
    // Reuses the same scratch buffer — phases run sequentially.
    objects.compact_fields_arena(&mut gc.arena_compact_buf);

    // ── Offensive root audit ─────────────────────────────────────────────
    // Re-walk every root source and panic if any ref points at a slot this
    // collection swept. Within a self-consistent collection this cannot
    // happen — everything a root visitor reports gets marked, and marked
    // slots are never swept. A hit therefore proves the mark/sweep state
    // was disturbed mid-collection (concurrent mutation, bitmap damage),
    // caught at the fatal collect with the collector's task in the
    // backtrace. Added for the picoenvmon rooted-but-swept corruption
    // (docs/picoenvmon-qa.md, 2026-08-17).
    #[cfg(feature = "mem-diag")]
    if offensive {
        let dyn_start = strings.dyn_start();
        let check = |src: &str, v: &Value| {
            let (kind, idx, stale) = match *v {
                Value::ObjectRef(i) => ("obj", i, !objects.is_live(i)),
                Value::ArrayRef(i) => ("arr", i, !arrays.is_live(i)),
                Value::Reference(i) => (
                    "str",
                    i,
                    (i as usize) >= dyn_start && !strings.is_dyn_live(i),
                ),
                _ => return,
            };
            if stale {
                panic!("mem-diag: root audit — {src} {kind} ref {idx} swept by this collection");
            }
        };
        for frame in frames {
            for v in frame.locals.iter().chain(frame.stack.iter()) {
                check("frame", v);
            }
        }
        for &pf in &gc.parked_frames {
            // SAFETY: same contract as the mark-phase walk above.
            let parked = unsafe { &*pf };
            for frame in parked {
                for v in frame.locals.iter().chain(frame.stack.iter()) {
                    check("parked-frame", v);
                }
            }
        }
        for v in statics.values_iter() {
            check("static", &v);
        }
        extra_roots(&mut |v| check("native", &v));
    }

    freed
}
