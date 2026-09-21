// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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

#[test]
fn gc_stress_steady_state_flat() {
    // The growth sentinel's contract in unit form (docs/memory-diagnostics.md):
    // a steady-state workload — allocate a burst, retain one object, GC —
    // must keep the post-GC live-bytes floor flat. Warm up for 10 cycles,
    // then assert the remaining 90 never exceed the warmup floor by more
    // than the 4096 B the runtime sentinel trips on (two fields-arena growth
    // steps: FIELDS_ARENA_CHUNK = 256 Slots = 2048 B).
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let mut warmup_floor: usize = 0;
    for cycle in 0u16..100 {
        for _ in 0..30 {
            objects.alloc("Steady").unwrap();
        }
        for _ in 0..8 {
            arrays.alloc(10, 24).unwrap(); // arena-backed
        }
        for j in 0..6u16 {
            strings
                .intern_dyn(format!("s{}_{}", cycle, j).as_bytes())
                .unwrap();
        }

        // Retain exactly one object per cycle (dropped next cycle) — live
        // set is constant, everything else is garbage.
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

        let live = objects.live_bytes() + arrays.live_bytes() + strings.live_bytes();
        if cycle < 10 {
            warmup_floor = warmup_floor.max(live);
        } else {
            assert!(
                live <= warmup_floor + 4096,
                "steady-state live floor grew: cycle {} live {} B vs warmup floor {} B",
                cycle,
                live,
                warmup_floor
            );
        }
    }
}
