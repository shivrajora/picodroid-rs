// SPDX-License-Identifier: GPL-3.0-only
use super::*;

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

#[test]
fn collect_now_frees_garbage_without_frames() {
    // Native code paths (e.g. the sensor-event drain loop) allocate between
    // bytecode executions, where no interpreter safepoint can run the
    // emergency GC. SharedJvmHeap::collect_now is their recovery path: a
    // full collect with no frames, rooted only by statics/Class objects and
    // the handler's native roots.
    let mut heap = crate::SharedJvmHeap::new();

    let obj = heap.objects.alloc("Garbage").unwrap();
    // Write past the declared field count so the object owns a lazy-grown
    // span in the fields arena — collect_now must sweep and compact it too.
    heap.objects.set_field(obj, 3, Value::Int(7)).unwrap();
    let arr = heap
        .arrays
        .alloc(crate::array_heap::ATYPE_FLOAT, 1)
        .unwrap();

    heap.gc_state.alloc_count = 42;
    heap.gc_state.need_gc = true;

    let freed = heap.collect_now(&mut crate::BuiltinHandler);

    assert_eq!(freed, 2);
    assert!(!heap.objects.is_live(obj));
    assert!(!heap.arrays.is_live(arr));
    assert_eq!(heap.gc_state.alloc_count, 0);
    assert!(!heap.gc_state.need_gc);
}

#[test]
fn registered_parked_frames_are_gc_roots() {
    // A JVM task parked at a blocking native (sleep, socket accept) is
    // mid-`execute` with live frame locals — but the GC runs with the
    // *collecting* task's frames only. The frame registry (GcState::
    // register_frames) is what keeps the parked task's locals alive; without
    // it the picoenvmon network thread's HttpServer was swept while the
    // thread sat in accept().
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let parked_obj = objects.alloc("ParkedLocal").unwrap();
    let garbage = objects.alloc("Garbage").unwrap();

    // Synthetic parked stack: one frame whose only reference to parked_obj
    // is a local slot.
    let parked_frames: alloc::vec::Vec<crate::frame::Frame> = alloc::vec![crate::frame::Frame {
        class_idx: 0,
        method_idx: 0,
        pc: 0,
        inst_pc: 0,
        locals: alloc::vec![Value::ObjectRef(parked_obj)],
        stack: alloc::vec::Vec::new(),
        box_return: 0,
        monitor: None,
    }];
    gc.register_frames(&parked_frames);

    // Collect with NO frames of our own — like collect_now, or another
    // executor whose own stack holds nothing.
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
    assert_eq!(freed, 1, "only the unreferenced object may be swept");
    assert!(
        objects.is_live(parked_obj),
        "parked frame local must survive"
    );
    assert!(!objects.is_live(garbage));

    // After the parked task's execute() ends, its locals become collectable.
    gc.unregister_frames(&parked_frames);
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
    assert_eq!(freed, 1);
    assert!(!objects.is_live(parked_obj));
}
