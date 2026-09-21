// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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
        let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
        let buf_idx = objects.map_alloc().unwrap();
        objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));
        for j in 0..5 {
            let _ = objects.map_put(
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

    let map_obj = objects.alloc(c::java_util_HashMap).unwrap();
    let buf_idx = objects.map_alloc().unwrap();
    objects.set_field(map_obj, 0, Value::Int(buf_idx as i32));

    let mut value_objs = alloc::vec::Vec::new();
    for i in 0..500 {
        let val = objects.alloc("Val").unwrap();
        objects.set_field(val, 0, Value::Int(i));
        let _ = objects.map_put(buf_idx, Value::Int(i), Value::ObjectRef(val), &strings);
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
