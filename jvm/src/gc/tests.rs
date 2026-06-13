// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use alloc::format;

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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(idx)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(idx));
}

#[test]
fn gc_traces_suppressed_through_live_owner() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let owner = objects.alloc("java/lang/RuntimeException").unwrap();
    let suppressed = objects.alloc("java/lang/RuntimeException").unwrap();
    objects.add_suppressed(owner, suppressed);

    // Only the owner is rooted; the suppressed Throwable is reachable
    // solely through the side table — exactly the post-try-with-resources
    // shape (the catch block's locals are gone).
    let frame = Frame::new(0, 0, &[Value::ObjectRef(owner)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(suppressed));
    assert_eq!(objects.suppressed_list(owner), &[suppressed]);
}

#[test]
fn gc_drops_suppressed_table_with_owner() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let owner = objects.alloc("java/lang/RuntimeException").unwrap();
    let suppressed = objects.alloc("java/lang/RuntimeException").unwrap();
    objects.add_suppressed(owner, suppressed);

    let frames = [];
    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 2);
    assert!(!objects.is_live(owner));
    assert!(!objects.is_live(suppressed));
    assert!(objects.suppressed_list(owner).is_empty());
}

#[test]
fn gc_traces_exception_message_through_live_owner() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    // A dynamically-built message ("x = " + v) is reachable only through
    // the message side table once the construction expression is done.
    let owner = objects.alloc("java/lang/RuntimeException").unwrap();
    let msg = strings.intern_dyn(b"dynamic message").unwrap();
    objects.register_exception_message(owner, msg);

    let frame = Frame::new(0, 0, &[Value::ObjectRef(owner)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert_eq!(strings.resolve(msg), Some("dynamic message"));
}

#[test]
fn gc_retains_object_in_frame_stack() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let idx = objects.alloc("OnStack").unwrap();
    let mut frame = Frame::new(0, 0, &[], 4, 4).unwrap();
    frame.push(Value::ObjectRef(idx)).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(a)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(a)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::Reference(str_idx)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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

    let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
    let frame = Frame::new(0, 0, &[Value::Null, Value::Int(42)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
        4,
        4,
    )
    .unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
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
            let frame = Frame::new(0, 0, &[Value::ObjectRef(i)], 4, 4).unwrap();
            let frames = [frame];
            collect(
                &frames,
                &mut objects,
                &mut arrays,
                &mut strings,
                &statics,
                &ClassObjectCache::new(),
                &mut GcState::new(),
                |_| {},
            );
        }
    }
    // Heap should not have grown to 100 slots due to reuse
    assert!(objects.slot_count() < 20);
}

#[test]
fn gc_compacts_array_arena() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    // Allocate 3 large (arena-backed) arrays
    let a0 = arrays.alloc(10, 20).unwrap(); // ATYPE_INT, 20 elements
    let a1 = arrays.alloc(10, 20).unwrap();
    let a2 = arrays.alloc(10, 20).unwrap();
    arrays.store(a0, 0, 111);
    arrays.store(a1, 0, 222);
    arrays.store(a2, 0, 333);

    // Root only the first and third arrays — middle should be collected
    let frame = Frame::new(0, 0, &[Value::ArrayRef(a0), Value::ArrayRef(a2)], 4, 4).unwrap();
    let frames = [frame];

    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert_eq!(freed, 1);
    assert!(arrays.is_live(a0));
    assert!(!arrays.is_live(a1));
    assert!(arrays.is_live(a2));

    // Arena should have been compacted: 2 * 20 = 40 elements
    assert_eq!(arrays.data_slice(a0).len(), 20);
    assert_eq!(arrays.data_slice(a2).len(), 20);
    assert_eq!(arrays.load(a0, 0), Some(111));
    assert_eq!(arrays.load(a2, 0), Some(333));
}

// ── Stress tests ─────────────────────────────────────────────────────────────

#[test]
fn gc_stress_alloc_free_cycles() {
    // 500 objects allocated in batches of 50, GC keeps only the last one alive.
    // Slot reuse must keep slot_count bounded.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    for batch in 0u16..10 {
        for i in 0u16..50 {
            objects.alloc("Churn").unwrap();
            let _ = (batch, i); // suppress unused warnings
        }
        // Root only the most recently allocated object.
        let last_idx = objects.slot_count() as u16 - 1;
        let frame = Frame::new(0, 0, &[Value::ObjectRef(last_idx)], 4, 4).unwrap();
        collect(
            &[frame],
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &ClassObjectCache::new(),
            &mut gc,
            |_| {},
        );
    }
    // Slot count should stay bounded due to slot reuse (well under 500).
    assert!(
        objects.slot_count() < 60,
        "slot_count {} should be < 60 with slot reuse",
        objects.slot_count()
    );
}

#[test]
fn gc_stress_mixed_type_churn() {
    // Allocate a mix of objects, arrays (inline + arena), and dynamic strings
    // in a loop.  Root a rotating window of 10.  Verify all rooted items survive.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let mut window: Vec<Value> = Vec::new();
    let mut total_freed = 0usize;

    for i in 0u16..200 {
        let obj = objects.alloc("Mix").unwrap();
        let arr_small = arrays.alloc(10, 4).unwrap(); // inline (≤8)
        let arr_large = arrays.alloc(10, 20).unwrap(); // arena-backed
        arrays.store(arr_large, 0, i as i32);
        let s = strings.intern_dyn(format!("s{}", i).as_bytes()).unwrap();

        // Keep a rotating window of 10 sets (obj, arr_small, arr_large, str).
        window.push(Value::ObjectRef(obj));
        window.push(Value::ArrayRef(arr_small));
        window.push(Value::ArrayRef(arr_large));
        window.push(Value::Reference(s));
        if window.len() > 40 {
            window.drain(..4);
        }

        let frame = Frame::new(0, 0, &window, window.len() as u16, 4).unwrap();
        let freed = collect(
            &[frame],
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &ClassObjectCache::new(),
            &mut gc,
            |_| {},
        );
        total_freed += freed;
    }

    // Verify all items in the current window are alive.
    for v in &window {
        match v {
            Value::ObjectRef(idx) => assert!(objects.is_live(*idx)),
            Value::ArrayRef(idx) => assert!(arrays.is_live(*idx)),
            Value::Reference(idx) => assert!(strings.resolve(*idx).is_some()),
            _ => {}
        }
    }
    // Many items should have been freed over 200 iterations.
    assert!(
        total_freed > 500,
        "expected substantial collection, got {total_freed}"
    );
}

#[test]
fn gc_stress_arena_fragmentation() {
    // Allocate arena-backed arrays of varying sizes, free every other one,
    // compact, verify data integrity.  Repeat for multiple rounds.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    for round in 0..5 {
        let mut indices = Vec::new();
        // Allocate 50 arrays of varying sizes (10-100 elements).
        for i in 0u16..50 {
            let size = 10 + (i as u16 % 10) * 10; // 10, 20, ..., 100
            let idx = arrays.alloc(10, size).unwrap();
            // Write a sentinel: round*1000 + i
            arrays.store(idx, 0, round * 1000 + i as i32);
            indices.push(idx);
        }

        // Root only even-indexed arrays; odd ones become garbage.
        let roots: Vec<Value> = indices
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, &idx)| Value::ArrayRef(idx))
            .collect();

        let frame = Frame::new(0, 0, &roots, roots.len() as u16, 4).unwrap();
        collect(
            &[frame],
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &ClassObjectCache::new(),
            &mut gc,
            |_| {},
        );

        // Verify surviving arrays have correct sentinels.
        for (i, &idx) in indices.iter().enumerate() {
            if i % 2 == 0 {
                assert!(
                    arrays.is_live(idx),
                    "round {round} idx {idx} should be live"
                );
                let expected = round * 1000 + i as i32;
                assert_eq!(arrays.load(idx, 0), Some(expected));
            } else {
                assert!(!arrays.is_live(idx));
            }
        }
    }
}

#[test]
fn gc_stress_string_table_churn() {
    // Intern 200 dynamic strings, root every 10th, GC, verify.
    // Then intern 200 more and verify slot reuse keeps total_len bounded.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let mut rooted_indices = Vec::new();
    for i in 0u16..200 {
        let idx = strings.intern_dyn(format!("str{}", i).as_bytes()).unwrap();
        if i % 10 == 0 {
            rooted_indices.push(idx);
        }
    }

    let roots: Vec<Value> = rooted_indices
        .iter()
        .map(|&idx| Value::Reference(idx))
        .collect();
    let frame = Frame::new(0, 0, &roots, roots.len() as u16, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );

    // Should have freed 180 of 200 strings (keeping every 10th).
    assert_eq!(freed, 180);
    for &idx in &rooted_indices {
        assert!(
            strings.resolve(idx).is_some(),
            "rooted string {idx} should survive"
        );
    }

    let total_after_first_gc = strings.total_len();

    // Intern 200 more — slot reuse should prevent unbounded growth.
    for i in 200u16..400 {
        strings.intern_dyn(format!("str{}", i).as_bytes()).unwrap();
    }

    // Collect everything except the original rooted set.
    let frame2 = Frame::new(0, 0, &roots, roots.len() as u16, 4).unwrap();
    let freed2 = collect(
        &[frame2],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert!(freed2 >= 180, "second GC should free at least 180 strings");

    // total_len should not have grown much beyond the first GC's value.
    let total_after_second_gc = strings.total_len();
    assert!(
        total_after_second_gc <= total_after_first_gc + 20,
        "string table grew too much: {} vs {}",
        total_after_second_gc,
        total_after_first_gc
    );
}

#[test]
fn gc_stress_persistent_state_reuse() {
    // Run 50 GC cycles with a single GcState instance.  Verify that the
    // persistent Vecs retain capacity but don't leak.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    for cycle in 0u16..50 {
        // Allocate a burst of items.
        for _ in 0..20 {
            objects.alloc("Cycle").unwrap();
        }
        for _ in 0..5 {
            arrays.alloc(10, 16).unwrap(); // arena-backed
        }
        for j in 0..5u16 {
            strings
                .intern_dyn(format!("c{}_{}", cycle, j).as_bytes())
                .unwrap();
        }

        // Root only one object from this cycle.
        let last = objects.slot_count() as u16 - 1;
        let frame = Frame::new(0, 0, &[Value::ObjectRef(last)], 4, 4).unwrap();
        collect(
            &[frame],
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &ClassObjectCache::new(),
            &mut gc,
            |_| {},
        );
    }

    // GcState buffers should have reasonable capacity — not growing each cycle.
    // obj_marks capacity depends on peak object count, which stays bounded
    // due to slot reuse.
    assert!(
        objects.slot_count() < 30,
        "objects should be bounded via slot reuse, got {}",
        objects.slot_count()
    );
}

// ── HashMap / HashSet GC tests ──────────────────────────────────────────────

#[test]
fn gc_retains_hashmap_entries() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    // Create a HashMap with ObjectRef key and ObjectRef value
    let map_obj = objects.alloc("java/util/HashMap").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let key_obj = objects.alloc("Key").unwrap();
    let val_obj = objects.alloc("Val").unwrap();
    objects.map_put(
        buf_idx,
        Value::ObjectRef(key_obj),
        Value::ObjectRef(val_obj),
        &strings,
    );

    // Root only the map — key and value should survive via map entry tracing
    let frame = Frame::new(0, 0, &[Value::ObjectRef(map_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(map_obj));
    assert!(objects.is_live(key_obj));
    assert!(objects.is_live(val_obj));
}

#[test]
fn gc_collects_unreachable_hashmap() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let map_obj = objects.alloc("java/util/HashMap").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));
    objects.map_put(buf_idx, Value::Int(1), Value::Int(10), &strings);

    // No roots — map should be collected
    let frames = [];
    let freed = collect(
        &frames,
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 1);
    assert!(!objects.is_live(map_obj));
}

#[test]
fn gc_hashmap_key_keeps_object_alive() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let map_obj = objects.alloc("java/util/HashMap").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let key_obj = objects.alloc("OnlyInKey").unwrap();
    objects.map_put(buf_idx, Value::ObjectRef(key_obj), Value::Int(1), &strings);

    let frame = Frame::new(0, 0, &[Value::ObjectRef(map_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(key_obj));
}

#[test]
fn gc_hashmap_value_keeps_object_alive() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let map_obj = objects.alloc("java/util/HashMap").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let val_obj = objects.alloc("OnlyInValue").unwrap();
    objects.map_put(buf_idx, Value::Int(1), Value::ObjectRef(val_obj), &strings);

    let frame = Frame::new(0, 0, &[Value::ObjectRef(map_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(val_obj));
}

#[test]
fn gc_hashset_retains_members() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let set_obj = objects.alloc("java/util/HashSet").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(set_obj, 0, Value::Int(buf_idx as i32));

    let member = objects.alloc("Member").unwrap();
    objects.map_put(buf_idx, Value::ObjectRef(member), Value::Int(1), &strings);

    let frame = Frame::new(0, 0, &[Value::ObjectRef(set_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(member));
}

// ── HashMap / HashSet stress tests ──────────────────────────────────────────

#[test]
fn gc_stress_hashmap_churn() {
    // Create 200 HashMaps in a loop, each with 5 entries. Root only the latest.
    // GC every 20 iterations. Verify slot reuse keeps heap bounded.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let mut last_map = 0u16;
    for i in 0u16..200 {
        let map_obj = objects.alloc("java/util/HashMap").unwrap();
        let buf_idx = objects.map_alloc().unwrap();
        objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));
        for j in 0..5 {
            objects.map_put(
                buf_idx,
                Value::Int(j),
                Value::Int(i as i32 * 10 + j),
                &strings,
            );
        }
        last_map = map_obj;

        if (i + 1) % 20 == 0 {
            let frame = Frame::new(0, 0, &[Value::ObjectRef(last_map)], 4, 4).unwrap();
            collect(
                &[frame],
                &mut objects,
                &mut arrays,
                &mut strings,
                &statics,
                &ClassObjectCache::new(),
                &mut gc,
                |_| {},
            );
        }
    }

    // Verify last map is alive with correct entries
    assert!(objects.is_live(last_map));
    let buf_idx = match objects.get_field(last_map, 0) {
        Some(Value::Int(n)) => n as u16,
        _ => panic!("expected map buf index"),
    };
    assert_eq!(objects.map_len(buf_idx), 5);

    // Heap should be bounded due to slot reuse
    assert!(
        objects.slot_count() < 30,
        "slot_count {} should be < 30 with slot reuse",
        objects.slot_count()
    );
}

#[test]
fn gc_stress_hashmap_large_map() {
    // Single HashMap with 500 entries, all Integer keys + ObjectRef values.
    // GC with map rooted — all 500 values survive. Then unroot and GC — all freed.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let map_obj = objects.alloc("java/util/HashMap").unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let mut value_objs = alloc::vec::Vec::new();
    for i in 0..500 {
        let val = objects.alloc("Val").unwrap();
        objects.set_field(val, 0, Value::Int(i));
        objects.map_put(buf_idx, Value::Int(i), Value::ObjectRef(val), &strings);
        value_objs.push(val);
    }

    // GC with map rooted — all 500 values survive
    let frame = Frame::new(0, 0, &[Value::ObjectRef(map_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert_eq!(freed, 0);
    for &val in &value_objs {
        assert!(objects.is_live(val), "value {} should be live", val);
    }

    // GC with no roots — everything freed
    let freed = collect(
        &[],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert!(
        freed >= 501,
        "expected at least 501 freed (1 map + 500 vals), got {}",
        freed
    );
    assert!(!objects.is_live(map_obj));
}

// ── Iterator GC tests ───────────────────────────────────────────────────────

#[test]
fn gc_collects_iterator() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    // Create a list and an iterator over it
    let list_obj = objects.alloc("java/util/ArrayList").unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    objects.list_add(buf_idx, Value::Int(10));

    let iter_obj = objects.alloc("java/util/Iterator").unwrap();
    objects.iter_register(
        iter_obj,
        IteratorState {
            source: IterSource::List(buf_idx),
            position: 0,
        },
    );

    // Root only the list — iterator should be collected
    let frame = Frame::new(0, 0, &[Value::ObjectRef(list_obj)], 4, 4).unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 1); // iterator freed
    assert!(!objects.is_live(iter_obj));
    assert!(objects.is_live(list_obj));
    // iter_state should have been cleaned up
    assert!(objects.iter_get(iter_obj).is_none());
}

#[test]
fn gc_retains_iterator_and_source() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let list_obj = objects.alloc("java/util/ArrayList").unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    objects.list_add(buf_idx, Value::Int(10));

    let iter_obj = objects.alloc("java/util/Iterator").unwrap();
    objects.iter_register(
        iter_obj,
        IteratorState {
            source: IterSource::List(buf_idx),
            position: 0,
        },
    );

    // Root both list and iterator
    let frame = Frame::new(
        0,
        0,
        &[Value::ObjectRef(list_obj), Value::ObjectRef(iter_obj)],
        4,
        4,
    )
    .unwrap();
    let freed = collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(list_obj));
    assert!(objects.is_live(iter_obj));
}

#[test]
fn gc_stress_iterator_churn() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    // Create one ArrayList
    let list_obj = objects.alloc("java/util/ArrayList").unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    for i in 0..10 {
        objects.list_add(buf_idx, Value::Int(i));
    }

    // Create 500 iterators on the same list, each abandoned after partial iteration
    let mut last_iter = 0u16;
    for i in 0u16..500 {
        let iter_obj = objects.alloc("java/util/Iterator").unwrap();
        objects.iter_register(
            iter_obj,
            IteratorState {
                source: IterSource::List(buf_idx),
                position: (i as usize) % 5,
            },
        );
        last_iter = iter_obj;

        if (i + 1) % 50 == 0 {
            // Root only the list and the latest iterator
            let frame = Frame::new(
                0,
                0,
                &[Value::ObjectRef(list_obj), Value::ObjectRef(last_iter)],
                4,
                4,
            )
            .unwrap();
            collect(
                &[frame],
                &mut objects,
                &mut arrays,
                &mut strings,
                &statics,
                &ClassObjectCache::new(),
                &mut gc,
                |_| {},
            );
        }
    }

    // Heap should be bounded
    assert!(
        objects.slot_count() < 60,
        "slot_count {} should be < 60 with iterator reuse",
        objects.slot_count()
    );
    // iter_states should also be bounded (freed iterators have their states removed)
    // Only the last iterator and the list should be live
    let frame = Frame::new(
        0,
        0,
        &[Value::ObjectRef(list_obj), Value::ObjectRef(last_iter)],
        4,
        4,
    )
    .unwrap();
    collect(
        &[frame],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert!(objects.is_live(list_obj));
    assert!(objects.is_live(last_iter));
}

// ── GcState lifecycle fields ────────────────────────────────────────────────
//
// `alloc_count` and `need_gc` are mutated from many sites in the interpreter
// (object_heap, array_heap, ops_*). They persist across `execute()` calls so a
// burst of small native callbacks still trips the GC threshold. These tests
// pin down the contract those callers rely on.

#[test]
fn gcstate_new_starts_clean() {
    let gc = GcState::new();
    assert_eq!(gc.alloc_count, 0);
    assert!(!gc.need_gc);
}

#[test]
fn gcstate_default_matches_new() {
    let a = GcState::default();
    let b = GcState::new();
    assert_eq!(a.alloc_count, b.alloc_count);
    assert_eq!(a.need_gc, b.need_gc);
}

#[test]
fn gcstate_alloc_count_is_mutable() {
    let mut gc = GcState::new();
    gc.alloc_count = 5;
    assert_eq!(gc.alloc_count, 5);
    gc.alloc_count += 1;
    assert_eq!(gc.alloc_count, 6);
}

#[test]
fn gcstate_need_gc_toggle() {
    let mut gc = GcState::new();
    assert!(!gc.need_gc);
    gc.need_gc = true;
    assert!(gc.need_gc);
    gc.need_gc = false;
    assert!(!gc.need_gc);
}

/// Reusing a GcState across two GC cycles must not leak retained-mark state
/// from the first cycle into the second.
#[test]
fn gcstate_reused_across_cycles_does_not_leak_marks() {
    let mut gc = GcState::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    objects.alloc("A");
    objects.alloc("B");
    let freed1 = collect(
        &[],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert_eq!(freed1, 2);

    // Second cycle on a fresh allocation must collect cleanly — if the first
    // cycle's marks leaked, the new object would appear pre-marked and not
    // be freed.
    objects.alloc("C");
    let freed2 = collect(
        &[],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut gc,
        |_| {},
    );
    assert_eq!(freed2, 1);
}

#[test]
fn gc_retains_object_via_extra_roots() {
    // The native handler keeps Views referenced only by its listener maps alive
    // by visiting them through the `extra_roots` hook (see
    // PicodroidNativeHandler::gc_visit_roots, and the per-widget
    // visit_*_listener_roots fns). This guards that mechanism: an object
    // reachable from NO frame/field/static survives iff `extra_roots` visits it —
    // the exact contract the widget listener-map roots depend on.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let idx = objects.alloc("ListenerOnlyView").unwrap();

    let freed = collect(
        &[],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |visit| visit(Value::ObjectRef(idx)),
    );
    assert_eq!(freed, 0);
    assert!(objects.is_live(idx));
}

#[test]
fn gc_collects_object_when_extra_roots_omits_it() {
    // Contrast to gc_retains_object_via_extra_roots: the same listener-only
    // object IS swept when the extra-roots hook does not visit it — the
    // missing-GC-root bug this fix addresses (a Switch/EditText reachable only
    // through an unvisited native listener map).
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let idx = objects.alloc("ListenerOnlyView").unwrap();

    let freed = collect(
        &[],
        &mut objects,
        &mut arrays,
        &mut strings,
        &statics,
        &ClassObjectCache::new(),
        &mut GcState::new(),
        |_| {},
    );
    assert_eq!(freed, 1);
    assert!(!objects.is_live(idx));
}
