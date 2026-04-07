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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(idx)], 4, 4).unwrap();
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
    let mut frame = Frame::new(0, 0, &[], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(a)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(a)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::ArrayRef(arr)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::Reference(str_idx)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)], 4, 4).unwrap();
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

    let frame = Frame::new(0, 0, &[Value::ObjectRef(obj)], 4, 4).unwrap();
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
    let frame = Frame::new(0, 0, &[Value::Null, Value::Int(42)], 4, 4).unwrap();
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
            let frame = Frame::new(0, 0, &[Value::ObjectRef(i)], 4, 4).unwrap();
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
        &mut gc,
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
