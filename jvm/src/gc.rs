use crate::{
    array_heap::{ArrayHeap, ATYPE_REF},
    frame::Frame,
    heap::StringTable,
    object_heap::ObjectHeap,
    static_fields::StaticFieldStore,
    types::Value,
};
use alloc::vec::Vec;

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
}

impl GcState {
    pub const fn new() -> Self {
        Self {
            obj_marks: Vec::new(),
            arr_marks: Vec::new(),
            str_marks: Vec::new(),
            work: Vec::new(),
        }
    }

    /// Clear all buffers (O(1), no deallocation — capacity is retained).
    fn clear(&mut self) {
        self.obj_marks.clear();
        self.arr_marks.clear();
        self.str_marks.clear();
        self.work.clear();
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
/// Scans all roots (frame locals/stacks + static fields), transitively marks
/// reachable objects/arrays/strings, then sweeps unreachable heap entries.
///
/// Returns the number of heap entries freed.
#[allow(clippy::too_many_arguments)]
pub fn collect(
    frames: &[Frame],
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    strings: &mut StringTable,
    statics: &StaticFieldStore,
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

                // ATYPE_REF arrays hold object references as i32
                if arrays.atype(idx) == Some(ATYPE_REF) {
                    for &val in arrays.data_slice(idx) {
                        if val >= 0 {
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
            objects.free_lambda(i);
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

    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_empty_heap_is_noop() {
        let frames = [];
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();
        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
    }

    #[test]
    fn gc_collects_unreachable_object() {
        let frames = [];
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        objects.alloc("Garbage");
        assert!(objects.is_live(0));

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 1);
        assert!(!objects.is_live(0));
    }

    #[test]
    fn gc_retains_object_in_frame_local() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let idx = objects.alloc("Keeper").unwrap();
        let frame = Frame::new(0, 0, &[Value::ObjectRef(idx)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(objects.is_live(idx));
    }

    #[test]
    fn gc_retains_object_in_frame_stack() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let idx = objects.alloc("OnStack").unwrap();
        let mut frame = Frame::new(0, 0, &[]).unwrap();
        frame.push(Value::ObjectRef(idx)).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(objects.is_live(idx));
    }

    #[test]
    fn gc_retains_object_via_static_field() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let mut statics = StaticFieldStore::new();

        let idx = objects.alloc("StaticRef").unwrap();
        statics.set(b"MyClass", b"field", Value::ObjectRef(idx));

        let frames = [];
        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(objects.is_live(idx));
    }

    #[test]
    fn gc_retains_deep_chain() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // A → B → C → D (chain of 4)
        let a = objects.alloc("A").unwrap();
        let b = objects.alloc("B").unwrap();
        let c = objects.alloc("C").unwrap();
        let d = objects.alloc("D").unwrap();
        objects.set_field(a, 0, Value::ObjectRef(b));
        objects.set_field(b, 0, Value::ObjectRef(c));
        objects.set_field(c, 0, Value::ObjectRef(d));

        // Only A is rooted
        let frame = Frame::new(0, 0, &[Value::ObjectRef(a)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(objects.is_live(a));
        assert!(objects.is_live(b));
        assert!(objects.is_live(c));
        assert!(objects.is_live(d));
    }

    #[test]
    fn gc_collects_circular_refs() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // A → B → A cycle, no external root
        let a = objects.alloc("A").unwrap();
        let b = objects.alloc("B").unwrap();
        objects.set_field(a, 0, Value::ObjectRef(b));
        objects.set_field(b, 0, Value::ObjectRef(a));

        let frames = [];
        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 2);
        assert!(!objects.is_live(a));
        assert!(!objects.is_live(b));
    }

    #[test]
    fn gc_collects_after_field_overwrite() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let a = objects.alloc("A").unwrap();
        let b = objects.alloc("B").unwrap();
        let c = objects.alloc("C").unwrap();

        // A.field = B, then overwrite with C
        objects.set_field(a, 0, Value::ObjectRef(b));
        objects.set_field(a, 0, Value::ObjectRef(c));

        // Only A is rooted — B should be collected, C retained
        let frame = Frame::new(0, 0, &[Value::ObjectRef(a)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 1);
        assert!(objects.is_live(a));
        assert!(!objects.is_live(b));
        assert!(objects.is_live(c));
    }

    #[test]
    fn gc_retains_array_ref() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let arr = arrays.alloc(10, 4).unwrap(); // ATYPE_INT
        let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(arrays.is_live(arr));
    }

    #[test]
    fn gc_collects_unreachable_array() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        arrays.alloc(10, 4).unwrap();
        let frames = [];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 1);
        assert!(!arrays.is_live(0));
    }

    #[test]
    fn gc_retains_ref_array_elements() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // Create an ATYPE_REF array holding an object reference
        let obj = objects.alloc("Target").unwrap();
        let arr = arrays.alloc(ATYPE_REF, 2).unwrap();
        arrays.store(arr, 0, obj as i32);

        // Only the array is rooted — object should survive via ATYPE_REF scan
        let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(objects.is_live(obj));
        assert!(arrays.is_live(arr));
    }

    #[test]
    fn gc_retains_dynamic_string() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let str_idx = strings.intern_dyn(b"hello").unwrap();
        let frame = Frame::new(0, 0, &[Value::Reference(str_idx)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert_eq!(strings.resolve(str_idx), Some("hello"));
    }

    #[test]
    fn gc_collects_unreachable_dynamic_string() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let str_idx = strings.intern_dyn(b"garbage").unwrap();
        let frames = [];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 1);
        assert_eq!(strings.resolve(str_idx), None);
    }

    #[test]
    fn gc_static_strings_unaffected() {
        static HELLO: &[u8] = b"hello";
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let str_idx = strings.intern(HELLO).unwrap();
        let frames = []; // No roots — but static strings should never be freed

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert_eq!(strings.resolve(str_idx), Some("hello"));
    }

    #[test]
    fn gc_object_holding_string_ref() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let str_idx = strings.intern_dyn(b"kept").unwrap();
        let obj = objects.alloc("Holder").unwrap();
        objects.set_field(obj, 0, Value::Reference(str_idx));

        // Root the object — string should survive via field reference
        let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert_eq!(strings.resolve(str_idx), Some("kept"));
    }

    #[test]
    fn gc_object_holding_array_ref() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        let arr = arrays.alloc(10, 3).unwrap();
        let obj = objects.alloc("Holder").unwrap();
        objects.set_field(obj, 0, Value::ArrayRef(arr));

        let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
        assert!(arrays.is_live(arr));
    }

    #[test]
    fn gc_slot_reuse_after_collect() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // Allocate 5 objects, root none
        for _ in 0..5 {
            objects.alloc("Temp").unwrap();
        }
        let count_before = objects.slot_count();

        let frames = [];
        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 5);

        // Allocate 5 more — should reuse freed slots, not grow
        for _ in 0..5 {
            objects.alloc("Reused").unwrap();
        }
        assert_eq!(objects.slot_count(), count_before);
    }

    #[test]
    fn gc_null_refs_ignored() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // Frame with Null values should not crash
        let frame = Frame::new(0, 0, &[Value::Null, Value::Int(42)]).unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
    }

    #[test]
    fn gc_all_unreachable() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        objects.alloc("Obj1").unwrap();
        objects.alloc("Obj2").unwrap();
        arrays.alloc(10, 3).unwrap();
        strings.intern_dyn(b"str1").unwrap();

        let frames = [];
        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 4); // 2 objects + 1 array + 1 string
    }

    #[test]
    fn gc_primitive_values_untouched() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // Frame with only primitive values
        let frame = Frame::new(
            0,
            0,
            &[
                Value::Int(1),
                Value::Long(2),
                Value::Float(3.0),
                Value::Double(4.0),
            ],
        )
        .unwrap();
        let frames = [frame];

        let freed = collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &mut GcState::new(),
        );
        assert_eq!(freed, 0);
    }

    #[test]
    fn gc_many_small_allocations_bounded() {
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let mut strings = StringTable::new();
        let statics = StaticFieldStore::new();

        // Allocate 100 objects, keeping only the last 5 live at a time
        for i in 0u16..100 {
            objects.alloc("Temp").unwrap();
            // After every 10 allocations, collect with only the latest as root
            if (i + 1) % 10 == 0 {
                let frame = Frame::new(0, 0, &[Value::ObjectRef(i)]).unwrap();
                let frames = [frame];
                collect(
                    &frames,
                    &mut objects,
                    &mut arrays,
                    &mut strings,
                    &statics,
                    &mut GcState::new(),
                );
            }
        }
        // Heap should not have grown to 100 slots due to reuse
        assert!(objects.slot_count() < 20);
    }
}
