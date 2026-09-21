// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Iterator GC tests ───────────────────────────────────────────────────────

#[test]
fn gc_collects_iterator() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    // Create a list and an iterator over it
    let list_obj = objects.alloc(c::java_util_ArrayList).unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    let _ = objects.list_add(buf_idx, Value::Int(10));

    let iter_obj = objects.alloc(c::java_util_Iterator).unwrap();
    objects
        .iter_register(
            iter_obj,
            IteratorState {
                source: IterSource::List(buf_idx),
                position: 0,
                owner: list_obj,
                expected_len: 0,
                last_returned: None,
            },
        )
        .unwrap();

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

/// `for (x in temp())`: the temporary list is reachable only through the
/// iterator. The iterator's `owner` pins it, so its buffer and the
/// references inside survive a collection mid-loop.
#[test]
fn gc_iterator_pins_temporary_list() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let list_obj = objects.alloc(c::java_util_ArrayList).unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    let elem = objects.alloc(c::java_lang_Object).unwrap();
    let _ = objects.list_add(buf_idx, Value::ObjectRef(elem));

    let iter_obj = objects.alloc(c::java_util_Iterator).unwrap();
    objects
        .iter_register(
            iter_obj,
            IteratorState {
                source: IterSource::List(buf_idx),
                position: 0,
                owner: list_obj,
                expected_len: 0,
                last_returned: None,
            },
        )
        .unwrap();

    // Root only the iterator: the list and its element must survive.
    let frame = Frame::new(0, 0, &[Value::ObjectRef(iter_obj)], 4, 4).unwrap();
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
    assert!(objects.is_live(elem));
    assert_eq!(objects.list_get(buf_idx, 0), Some(Value::ObjectRef(elem)));

    // Drop the iterator too: now everything goes.
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
    assert_eq!(freed, 3);
}

/// A `keySet()`/`values()`/`entrySet()` view over a temporary map holds the
/// map in its field 1, so the map buffer outlives the map's last direct
/// reference for as long as the view (or an iterator over it) does.
#[test]
fn gc_map_view_pins_temporary_map() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));
    let key = objects.alloc(c::java_lang_Object).unwrap();
    let _ = objects.map_put(buf_idx, Value::ObjectRef(key), Value::Int(1), &mut strings);

    let view = objects.alloc(c::java_util_HashMap_KeySet).unwrap();
    objects.set_field(view, 0, Value::Int(buf_idx as i32));
    objects.set_field(view, 1, Value::ObjectRef(map_obj));
    let iter_obj = objects.alloc(c::java_util_Iterator).unwrap();
    objects
        .iter_register(
            iter_obj,
            IteratorState {
                source: IterSource::MapKeys(buf_idx),
                position: 0,
                owner: view,
                expected_len: 0,
                last_returned: None,
            },
        )
        .unwrap();

    let frame = Frame::new(0, 0, &[Value::ObjectRef(iter_obj)], 4, 4).unwrap();
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
    assert!(objects.is_live(map_obj) && objects.is_live(view) && objects.is_live(key));
    assert!(objects.map_contains_key(buf_idx, Value::ObjectRef(key), &strings));
}

#[test]
fn gc_retains_iterator_and_source() {
    use crate::object_heap::iter_store::{IterSource, IteratorState};

    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let list_obj = objects.alloc(c::java_util_ArrayList).unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    let _ = objects.list_add(buf_idx, Value::Int(10));

    let iter_obj = objects.alloc(c::java_util_Iterator).unwrap();
    objects
        .iter_register(
            iter_obj,
            IteratorState {
                source: IterSource::List(buf_idx),
                position: 0,
                owner: list_obj,
                expected_len: 0,
                last_returned: None,
            },
        )
        .unwrap();

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
    let list_obj = objects.alloc(c::java_util_ArrayList).unwrap();
    let buf_idx = objects.list_alloc().unwrap();
    objects.set_field(list_obj, 0, Value::Int(buf_idx as i32));
    for i in 0..10 {
        let _ = objects.list_add(buf_idx, Value::Int(i));
    }

    // Create 500 iterators on the same list, each abandoned after partial iteration
    let mut last_iter = 0u16;
    for i in 0u16..500 {
        let iter_obj = objects.alloc(c::java_util_Iterator).unwrap();
        objects
            .iter_register(
                iter_obj,
                IteratorState {
                    source: IterSource::List(buf_idx),
                    position: (i as usize) % 5,
                    owner: list_obj,
                    expected_len: 0,
                    last_returned: None,
                },
            )
            .unwrap();
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
