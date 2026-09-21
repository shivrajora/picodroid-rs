// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── HashMap / HashSet GC tests ──────────────────────────────────────────────

#[test]
fn gc_retains_hashmap_entries() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    // Create a HashMap with ObjectRef key and ObjectRef value
    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let key_obj = objects.alloc("Key").unwrap();
    let val_obj = objects.alloc("Val").unwrap();
    let _ = objects.map_put(
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

    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));
    let _ = objects.map_put(buf_idx, Value::Int(1), Value::Int(10), &strings);

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

    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let key_obj = objects.alloc("OnlyInKey").unwrap();
    let _ = objects.map_put(buf_idx, Value::ObjectRef(key_obj), Value::Int(1), &strings);

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

    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let val_obj = objects.alloc("OnlyInValue").unwrap();
    let _ = objects.map_put(buf_idx, Value::Int(1), Value::ObjectRef(val_obj), &strings);

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

    let set_obj = objects.alloc(c::java_util_HashSet).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(set_obj, 0, Value::Int(buf_idx as i32));

    let member = objects.alloc("Member").unwrap();
    let _ = objects.map_put(buf_idx, Value::ObjectRef(member), Value::Int(1), &strings);

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
