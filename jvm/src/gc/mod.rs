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
}

impl GcState {
    pub const fn new() -> Self {
        Self {
            obj_marks: Vec::new(),
            arr_marks: Vec::new(),
            str_marks: Vec::new(),
            work: Vec::new(),
            arena_compact_buf: Vec::new(),
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
) -> usize {
    gc.clear();
    let obj_marks = &mut gc.obj_marks;
    let arr_marks = &mut gc.arr_marks;
    let str_marks = &mut gc.str_marks;
    let work = &mut gc.work;

    // ── Phase 1: scan roots ──────────────────────────────────────────────

    // Frame locals and operand stacks
    for frame in frames {
        for v in &frame.locals {
            push_ref(work, v);
        }
        for v in &frame.stack {
            push_ref(work, v);
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

    // ── Phase 2: mark (transitive closure) ───────────────────────────────

    while let Some(r) = work.pop() {
        match r {
            GcRef::Object(idx) => {
                if is_marked(obj_marks, idx) {
                    continue;
                }
                mark_bit(obj_marks, idx);

                // Scan object fields (inline + overflow)
                let (inline, overflow) = objects.field_slices(idx);
                for v in inline {
                    push_ref(work, v);
                }
                for v in overflow {
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

    freed
}
