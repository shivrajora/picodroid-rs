// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── LinkedHashMap / LinkedHashSet aliases own a map buffer like HashMap ─────

#[test]
fn gc_traces_and_frees_linked_hash_map_like_hash_map() {
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();

    let map_obj = objects.alloc(c::java_util_LinkedHashMap).unwrap();
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

    // Rooted: the entries are traced through the alias's buffer.
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
    assert!(objects.is_live(key_obj) && objects.is_live(val_obj));

    // Unrooted: the map, its entries and its buffer slot are all reclaimed.
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
    assert!(!objects.is_live(map_obj));
    assert_eq!(objects.map_len(buf_idx), 0);
    assert_eq!(
        objects.map_alloc(),
        Some(buf_idx),
        "buffer slot was not freed"
    );
}

#[test]
fn collect_now_prunes_native_state_with_the_sweep_result() {
    // `native_state_prune` is the hook a handler uses to drop native storage
    // keyed by heap slots (the JSON node pool binds each wrapper's slot). It
    // must fire on the frameless `collect_now` path too, after the sweep and
    // with `live` answering for the slots this collection freed.
    use crate::native::{NativeContext, NativeMethodHandler};
    use crate::types::{JvmError, MonitorKey};

    struct Recorder {
        seen: Vec<(u16, bool)>,
        probe: Vec<u16>,
    }
    impl NativeMethodHandler for Recorder {
        fn dispatch(
            &mut self,
            _class_name: &str,
            _method_name: &str,
            _ctx: &mut NativeContext<'_>,
        ) -> Option<Result<Option<Value>, JvmError>> {
            None
        }
        fn native_state_prune(&mut self, live: &dyn Fn(MonitorKey) -> bool) {
            for &slot in &self.probe {
                self.seen.push((slot, live(MonitorKey::Object(slot))));
            }
        }
    }

    let mut heap = crate::SharedJvmHeap::new();
    let dead = heap.objects.alloc("Garbage").unwrap();
    let kept = heap.objects.alloc("Kept").unwrap();
    heap.statics.set(b"K", b"f", Value::ObjectRef(kept));
    heap.gc_state.need_gc = true;

    let mut handler = Recorder {
        seen: Vec::new(),
        probe: alloc::vec![dead, kept],
    };
    heap.collect_now(&mut handler);

    assert_eq!(handler.seen, alloc::vec![(dead, false), (kept, true)]);
    assert!(!heap.objects.is_live(dead));
    assert!(heap.objects.is_live(kept));
}

// The collector runs on the fullest heap there is: its mark stack used to
// grow with an infallible push and abort the firmware from inside the one
// routine meant to relieve the pressure (reached once 8 B field slots let
// enough boxes live under the qa_oom budget). A heap that cannot hold the
// stack now makes the collection give up before the sweep.
#[test]
fn a_heap_too_full_for_the_mark_stack_makes_the_collection_give_up() {
    use crate::test_alloc::with_budget;
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let statics = StaticFieldStore::new();
    let mut gc = GcState::new();

    let keeper = objects.alloc("Keeper").unwrap();
    let garbage = objects.alloc("Garbage").unwrap();
    let frame = Frame::new(0, 0, &[Value::ObjectRef(keeper)], 4, 4).unwrap();
    let frames = [frame];

    // No budget at all: the mark stack cannot take its first root.
    let freed = with_budget(0, || {
        collect(
            &frames,
            &mut objects,
            &mut arrays,
            &mut strings,
            &statics,
            &ClassObjectCache::new(),
            &mut gc,
            |_| {},
        )
    });
    assert_eq!(freed, 0, "a collection that could not mark must not sweep");
    assert!(objects.is_live(keeper));
    assert!(objects.is_live(garbage), "unmarked is not unreachable");

    // With room, the same collection frees the garbage and keeps the root.
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
    assert!(objects.is_live(keeper));
    assert!(!objects.is_live(garbage));
}
