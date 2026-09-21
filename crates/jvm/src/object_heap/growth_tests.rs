// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use alloc::vec;

#[test]
fn reserve_fallible_reports_exhaustion_instead_of_aborting() {
    let mut v: Vec<u64> = Vec::new();
    assert_eq!(reserve_fallible(&mut v, 4), Ok(()));
    assert!(v.capacity() >= 4);
    // An impossible request fails softly on both growth paths.
    assert_eq!(reserve_fallible(&mut v, usize::MAX / 16), Err(Exhausted));
    assert!(v.capacity() < usize::MAX / 16);
}

#[test]
fn boxed_cache_shares_the_jls_range_only() {
    let mut heap = ObjectHeap::new();
    assert_eq!(heap.cached_box(c::java_lang_Integer, Value::Int(127)), None);
    let a = heap.alloc(c::java_lang_Integer).unwrap();
    heap.cache_box(c::java_lang_Integer, Value::Int(127), a);
    assert_eq!(
        heap.cached_box(c::java_lang_Integer, Value::Int(127)),
        Some(a)
    );
    // Out of range: never cached.
    let b = heap.alloc(c::java_lang_Integer).unwrap();
    heap.cache_box(c::java_lang_Integer, Value::Int(128), b);
    assert_eq!(heap.cached_box(c::java_lang_Integer, Value::Int(128)), None);
    // Long has its own table; char only caches 0..=127; Boolean two slots.
    let l = heap.alloc(c::java_lang_Long).unwrap();
    heap.cache_box(c::java_lang_Long, Value::Long(-128), l);
    assert_eq!(
        heap.cached_box(c::java_lang_Long, Value::Long(-128)),
        Some(l)
    );
    assert_eq!(
        heap.cached_box(c::java_lang_Integer, Value::Int(-128)),
        None
    );
    let ch = heap.alloc(c::java_lang_Character).unwrap();
    heap.cache_box(c::java_lang_Character, Value::Int(200), ch);
    assert_eq!(
        heap.cached_box(c::java_lang_Character, Value::Int(200)),
        None
    );
    let t = heap.alloc(c::java_lang_Boolean).unwrap();
    heap.cache_box(c::java_lang_Boolean, Value::Int(1), t);
    assert_eq!(
        heap.cached_box(c::java_lang_Boolean, Value::Int(1)),
        Some(t)
    );
    assert_eq!(heap.cached_box(c::java_lang_Boolean, Value::Int(0)), None);
    // Doubles are never shared (no identity contract).
    let d = heap.alloc(c::java_lang_Double).unwrap();
    heap.cache_box(c::java_lang_Double, Value::Double(1.0), d);
    assert_eq!(
        heap.cached_box(c::java_lang_Double, Value::Double(1.0)),
        None
    );
    let roots: Vec<u16> = heap.boxed_cache_roots().collect();
    assert_eq!(roots, vec![a, l, t]);
}

// ── QA 2026-09-13 regression guards ──────────────────────────────────

fn boxed(heap: &mut ObjectHeap, class: &'static str, v: Value) -> Value {
    let idx = heap.alloc(class).unwrap();
    heap.set_field(idx, 0, v);
    Value::ObjectRef(idx)
}

// J-fix 390d1510: two boxes are one key only when their class and value
// agree -- field 0 alone made Integer(1), Short(1), Byte(1) and
// Boolean(true) a single entry.
#[test]
fn key_eq_separates_boxes_of_different_classes() {
    let mut heap = ObjectHeap::new();
    let strings = crate::heap::StringTable::new();
    let i = boxed(&mut heap, c::java_lang_Integer, Value::Int(1));
    let sh = boxed(&mut heap, c::java_lang_Short, Value::Int(1));
    let by = boxed(&mut heap, c::java_lang_Byte, Value::Int(1));
    let bo = boxed(&mut heap, c::java_lang_Boolean, Value::Int(1));
    for (a, b) in [(i, sh), (i, by), (i, bo), (sh, by), (by, bo)] {
        assert!(!key_eq(a, b, &heap, &strings), "{a:?} must not equal {b:?}");
    }
    let i2 = boxed(&mut heap, c::java_lang_Integer, Value::Int(1));
    assert!(key_eq(i, i2, &heap, &strings));
}

// J-fix 390d1510: a class without an equals of its own compares by
// identity, however its fields line up.
#[test]
fn key_eq_compares_plain_objects_by_identity() {
    let mut heap = ObjectHeap::new();
    let strings = crate::heap::StringTable::new();
    let a = boxed(&mut heap, "Point", Value::Int(7));
    let b = boxed(&mut heap, "Point", Value::Int(7));
    assert!(!key_eq(a, b, &heap, &strings));
    assert!(key_eq(a, a, &heap, &strings));
}

// J-fix 390d1510: Double/Float keys follow Double.equals -- NaN equals
// NaN, and 0.0 is not -0.0.
#[test]
fn key_eq_follows_double_equals_for_nan_and_signed_zero() {
    let mut heap = ObjectHeap::new();
    let strings = crate::heap::StringTable::new();
    let nan_a = boxed(&mut heap, c::java_lang_Double, Value::Double(f64::NAN));
    let nan_b = boxed(&mut heap, c::java_lang_Double, Value::Double(f64::NAN));
    assert!(key_eq(nan_a, nan_b, &heap, &strings));
    let pos = boxed(&mut heap, c::java_lang_Double, Value::Double(0.0));
    let neg = boxed(&mut heap, c::java_lang_Double, Value::Double(-0.0));
    assert!(!key_eq(pos, neg, &heap, &strings));
    let fa = boxed(&mut heap, c::java_lang_Float, Value::Float(f32::NAN));
    let fb = boxed(&mut heap, c::java_lang_Float, Value::Float(f32::NAN));
    assert!(key_eq(fa, fb, &heap, &strings));
}

// Found writing the QA round's out-of-memory tests: this table grew with
// an infallible push, so the most Java-reachable allocation there is --
// every `new`, every box -- aborted the firmware on a full heap, which on
// a device is a board reset. A heap too small for one more object must
// refuse, the way every caller of `alloc` already expects.
#[test]
fn an_object_slot_the_heap_cannot_hold_is_refused_not_aborted() {
    let mut heap = ObjectHeap::new();
    // Warm the table so the next chunk is the allocation under test.
    for _ in 0..8 {
        heap.alloc(c::java_lang_Object).expect("warm-up");
    }
    // One of these must be refused: 16 bytes cannot hold a new chunk.
    let refused = crate::test_alloc::with_budget(16, || {
        (0..1024).any(|_| heap.alloc(c::java_lang_Object).is_none())
    });
    assert!(refused, "an exhausted heap must refuse, not abort");
    // The refusal leaves the heap usable.
    assert!(heap.alloc(c::java_lang_Object).is_some());
}

// The last infallible allocation of the 2026-09-13 QA round: the lambda
// registry doubled through a plain push, and on the touch kit the step
// from 64 to 128 entries (7,680 bytes) reset the board from a burst of
// `execute(() -> ...)` posts. A registry the heap cannot grow must
// refuse, so `invokedynamic` can throw OutOfMemoryError instead.
#[test]
fn a_lambda_the_registry_cannot_hold_is_refused_not_aborted() {
    let proxy = || LambdaProxy {
        target: LambdaTarget::Java {
            class_idx: 0,
            method_idx: 0,
        },
        captures: Vec::new(),
        sam_name: b"run",
    };
    let mut heap = ObjectHeap::new();
    // Warm the registry so the next growth step is the allocation under test.
    for i in 0..4 {
        heap.register_lambda(i, proxy()).expect("warm-up");
    }
    let refused = crate::test_alloc::with_budget(16, || {
        (4..1024u16).any(|i| heap.register_lambda(i, proxy()).is_err())
    });
    assert!(refused, "an exhausted heap must refuse, not abort");
    // The refusal leaves the registry usable, and the refused entry absent.
    let n = heap.lambda_proxies.len() as u16;
    assert!(heap.get_lambda(n).is_none());
    heap.register_lambda(n, proxy())
        .expect("usable after a refusal");
    assert!(heap.get_lambda(n).is_some());
}

// The iterator registry has the same shape and the same fix.
#[test]
fn an_iterator_the_registry_cannot_hold_is_refused_not_aborted() {
    let state = || iter_store::IteratorState {
        source: iter_store::IterSource::List(0),
        position: 0,
        owner: 0,
        expected_len: 0,
        last_returned: None,
    };
    let mut heap = ObjectHeap::new();
    for i in 0..4 {
        heap.iter_register(i, state()).expect("warm-up");
    }
    let refused = crate::test_alloc::with_budget(16, || {
        (4..1024u16).any(|i| heap.iter_register(i, state()).is_err())
    });
    assert!(refused, "an exhausted heap must refuse, not abort");
    let n = heap.iter_states.len() as u16;
    heap.iter_register(n, state())
        .expect("usable after a refusal");
    assert!(heap.iter_get(n).is_some());
}

// The exception side tables doubled through a plain push as well: the
// last infallible `ObjectHeap` growth after the 2026-09-13 QA round. A
// refusal leaves the table as it was, so the caller can throw the
// Throwable without the entry.
#[test]
fn an_exception_message_the_table_cannot_hold_is_refused_not_aborted() {
    let mut heap = ObjectHeap::new();
    for i in 0..4 {
        heap.register_exception_message(i, i).expect("warm-up");
    }
    let n = crate::test_alloc::with_budget(16, || {
        (4..1024u16).find(|&i| heap.register_exception_message(i, i).is_err())
    })
    .expect("an exhausted heap must refuse, not abort");
    assert_eq!(heap.get_exception_message(n), None);
    assert_eq!(heap.get_exception_message(n - 1), Some(n - 1));
    heap.register_exception_message(n, n)
        .expect("usable after a refusal");
    assert_eq!(heap.get_exception_message(n), Some(n));
}

#[test]
fn an_exception_cause_the_table_cannot_hold_is_refused_not_aborted() {
    let mut heap = ObjectHeap::new();
    for i in 0..4 {
        heap.register_exception_cause(i, i + 1).expect("warm-up");
    }
    let n = crate::test_alloc::with_budget(16, || {
        (4..1024u16).find(|&i| heap.register_exception_cause(i, i + 1).is_err())
    })
    .expect("an exhausted heap must refuse, not abort");
    assert_eq!(heap.get_exception_cause(n), None);
    assert_eq!(heap.get_exception_cause(n - 1), Some(n));
    heap.register_exception_cause(n, n + 1)
        .expect("usable after a refusal");
    assert_eq!(heap.get_exception_cause(n), Some(n + 1));
}

// `suppressed` grows in two places: the table, one entry per owner, and
// each owner's own list.
#[test]
fn a_suppressed_owner_the_table_cannot_hold_is_refused_not_aborted() {
    let mut heap = ObjectHeap::new();
    for owner in 0..4 {
        heap.add_suppressed(owner, 1000).expect("warm-up");
    }
    let n = crate::test_alloc::with_budget(16, || {
        (4..1024u16).find(|&owner| heap.add_suppressed(owner, 1000).is_err())
    })
    .expect("an exhausted heap must refuse, not abort");
    assert!(heap.suppressed_list(n).is_empty());
    assert_eq!(heap.suppressed_list(n - 1), &[1000]);
    heap.add_suppressed(n, 1000)
        .expect("usable after a refusal");
    assert_eq!(heap.suppressed_list(n), &[1000]);
}

#[test]
fn a_suppressed_entry_the_owner_list_cannot_hold_is_refused_not_aborted() {
    let mut heap = ObjectHeap::new();
    for t in 0..4 {
        heap.add_suppressed(7, t).expect("warm-up");
    }
    let n = crate::test_alloc::with_budget(16, || {
        (4..1024u16).find(|&t| heap.add_suppressed(7, t).is_err())
    })
    .expect("an exhausted heap must refuse, not abort");
    // Everything before the refusal is kept, in order, and nothing after.
    let kept: Vec<u16> = (0..n).collect();
    assert_eq!(heap.suppressed_list(7), kept.as_slice());
    heap.add_suppressed(7, n).expect("usable after a refusal");
    assert_eq!(heap.suppressed_list(7).last(), Some(&n));
}

// J-fix aeb8ecc1: clear() hands the buffer back, so an app that clears
// after an OutOfMemoryError actually recovers the arena.
#[test]
fn list_clear_releases_the_buffer_capacity() {
    let mut heap = ObjectHeap::new();
    let buf = heap.list_alloc().unwrap();
    for i in 0..64 {
        heap.list_add(buf, Value::Int(i)).unwrap();
    }
    assert!(heap.list_bufs[buf as usize].as_ref().unwrap().capacity() >= 64);
    heap.list_clear(buf);
    assert_eq!(heap.list_len(buf), 0);
    assert_eq!(heap.list_bufs[buf as usize].as_ref().unwrap().capacity(), 0);
}

#[test]
fn map_clear_releases_the_buffer_capacity() {
    let mut heap = ObjectHeap::new();
    let strings = crate::heap::StringTable::new();
    let buf = heap.map_alloc().unwrap();
    for i in 0..64 {
        heap.map_put(buf, Value::Int(i), Value::Int(i), &strings)
            .unwrap();
    }
    assert!(heap.map_bufs[buf as usize].as_ref().unwrap().capacity() >= 64);
    heap.map_clear(buf);
    assert_eq!(heap.map_len(buf), 0);
    assert_eq!(heap.map_bufs[buf as usize].as_ref().unwrap().capacity(), 0);
}
