// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::types::Value;

#[test]
fn alloc_returns_sequential_indices() {
    let mut heap = ObjectHeap::new();
    assert_eq!(heap.alloc("A"), Some(0));
    assert_eq!(heap.alloc("B"), Some(1));
    assert_eq!(heap.alloc("C"), Some(2));
}

#[test]
fn alloc_beyond_old_capacity_succeeds() {
    let mut heap = ObjectHeap::new();
    for i in 0..64 {
        assert!(heap.alloc(if i % 2 == 0 { "X" } else { "Y" }).is_some());
    }
}

#[test]
fn get_field_nonexistent_field_returns_none() {
    let mut heap = ObjectHeap::new();
    heap.alloc("A");
    assert_eq!(heap.get_field(0, 0), None);
}

#[test]
fn set_and_get_field_round_trip() {
    let mut heap = ObjectHeap::new();
    heap.alloc("A");
    heap.set_field(0, 0, Value::Int(42));
    assert_eq!(heap.get_field(0, 0), Some(Value::Int(42)));
}

#[test]
fn set_field_fills_gaps_with_null() {
    let mut heap = ObjectHeap::new();
    heap.alloc("A");
    heap.set_field(0, 2, Value::Int(5));
    assert_eq!(heap.get_field(0, 0), Some(Value::Null));
    assert_eq!(heap.get_field(0, 1), Some(Value::Null));
    assert_eq!(heap.get_field(0, 2), Some(Value::Int(5)));
}

#[test]
fn clone_object_shallow_copies_fields() {
    let mut heap = ObjectHeap::new();
    let src = heap.alloc("Point").unwrap();
    heap.set_field(src, 0, Value::Int(3));
    heap.set_field(src, 1, Value::ArrayRef(7)); // reference field

    let copy = heap.clone_object(src).unwrap();
    assert_ne!(src, copy, "clone must be a distinct object");
    assert_eq!(heap.class_name(copy), Some("Point"));
    assert_eq!(heap.get_field(copy, 0), Some(Value::Int(3)));
    // Shallow: the reference field shares its referent.
    assert_eq!(heap.get_field(copy, 1), Some(Value::ArrayRef(7)));

    // Mutating the copy leaves the original untouched.
    heap.set_field(copy, 0, Value::Int(99));
    assert_eq!(heap.get_field(src, 0), Some(Value::Int(3)));
}

#[test]
fn clone_object_invalid_index_returns_none() {
    let mut heap = ObjectHeap::new();
    assert_eq!(heap.clone_object(42), None);
}

#[test]
fn class_name_returns_correct_name() {
    let mut heap = ObjectHeap::new();
    heap.alloc("MyClass");
    assert_eq!(heap.class_name(0), Some("MyClass"));
}

#[test]
fn class_name_invalid_index_returns_none() {
    let heap = ObjectHeap::new();
    assert_eq!(heap.class_name(99), None);
}

#[test]
fn exception_message_register_get_free() {
    let mut heap = ObjectHeap::new();
    let obj = heap.alloc(c::java_lang_RuntimeException).unwrap();
    assert_eq!(heap.get_exception_message(obj), None);
    heap.register_exception_message(obj, 7).unwrap();
    assert_eq!(heap.get_exception_message(obj), Some(7));
    // Re-registering replaces the existing entry.
    heap.register_exception_message(obj, 11).unwrap();
    assert_eq!(heap.get_exception_message(obj), Some(11));
    heap.free_exception_message(obj);
    assert_eq!(heap.get_exception_message(obj), None);
}

#[test]
fn float_to_str_buf_matches_java_float_to_string() {
    // Shortest round-trip digits with Java's layout. The old 6-digit
    // formatter lost the rounding carry (0.99999994 -> "0.0") and
    // reinterpreted a saturated `as u32` as i32 above 2^31 (1e10 -> "-1…").
    let cases: &[(f32, &[u8])] = &[
        (0.99999994, b"0.99999994"),
        (1e10, b"1.0E10"),
        (1.0 / 3.0, b"0.33333334"),
        (100.0, b"100.0"),
        (1e-4, b"1.0E-4"),
        (-2.5, b"-2.5"),
        (0.1, b"0.1"),
        (3.4028235e38, b"3.4028235E38"),
        (0.0, b"0.0"),
        (-0.0, b"-0.0"),
    ];
    for &(f, want) in cases {
        let mut buf = [0u8; 32];
        assert_eq!(float_to_str_buf(f, &mut buf), want, "{f}");
    }
}

#[test]
fn double_to_str_buf_matches_java_layout() {
    let mut b = [0u8; 32];
    assert_eq!(double_to_str_buf(100.0, &mut b), b"100.0");
    let mut b = [0u8; 32];
    assert_eq!(double_to_str_buf(1e10, &mut b), b"1.0E10");
    let mut b = [0u8; 32];
    assert_eq!(double_to_str_buf(-1.5e-5, &mut b), b"-1.5E-5");
    let mut b = [0u8; 32];
    assert_eq!(
        double_to_str_buf(f64::MAX, &mut b),
        b"1.7976931348623157E308"
    );
    let mut b = [0u8; 32];
    assert_eq!(double_to_str_buf(-0.0, &mut b), b"-0.0");
}

#[test]
fn int_to_decimal_buf_handles_min_value() {
    // `wrapping_neg` is a no-op on MIN, so the digit loop used to skip
    // and the function returned a bare "-".
    let mut b = [0u8; 12];
    assert_eq!(int_to_decimal_buf(i32::MIN, &mut b), b"-2147483648");
    let mut b = [0u8; 12];
    assert_eq!(int_to_decimal_buf(i32::MAX, &mut b), b"2147483647");
    let mut b = [0u8; 12];
    assert_eq!(int_to_decimal_buf(-1, &mut b), b"-1");
}

#[test]
fn long_to_decimal_buf_handles_min_value() {
    let mut b = [0u8; 21];
    assert_eq!(
        long_to_decimal_buf(i64::MIN, &mut b),
        b"-9223372036854775808"
    );
    let mut b = [0u8; 21];
    assert_eq!(
        long_to_decimal_buf(i64::MAX, &mut b),
        b"9223372036854775807"
    );
}

#[test]
fn get_field_invalid_object_returns_none() {
    let heap = ObjectHeap::new();
    assert_eq!(heap.get_field(99, 0), None);
}

#[test]
fn string_builder_instances_are_distinct() {
    let mut heap = ObjectHeap::new();
    let idx1 = heap.alloc(c::java_lang_StringBuilder);
    let idx2 = heap.alloc(c::java_lang_StringBuilder);
    assert!(idx1.is_some());
    assert_ne!(idx1, idx2);
}

#[test]
fn sb_append_bytes_and_contents() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(b, b"hello");
    assert_eq!(heap.sb_contents_slice(b), b"hello");
    assert_eq!(heap.sb_len(b), 5);
}

#[test]
fn sb_alloc_creates_fresh_buffer() {
    let mut heap = ObjectHeap::new();
    let a = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(a, b"hello");
    let b = heap.sb_alloc().unwrap();
    assert_eq!(heap.sb_len(b), 0);
    assert_eq!(heap.sb_len(a), 5);
}

#[test]
fn sb_char_at() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(b, b"abc");
    assert_eq!(heap.sb_char_at(b, 0), Some(b'a'));
    assert_eq!(heap.sb_char_at(b, 2), Some(b'c'));
    assert_eq!(heap.sb_char_at(b, 3), None);
}

#[test]
fn sb_append_int_zero() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    heap.sb_append_int(b, 0);
    assert_eq!(heap.sb_contents_slice(b), b"0");
}

#[test]
fn sb_append_int_positive() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    heap.sb_append_int(b, 12345);
    assert_eq!(heap.sb_contents_slice(b), b"12345");
}

#[test]
fn sb_append_int_negative() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    heap.sb_append_int(b, -42);
    assert_eq!(heap.sb_contents_slice(b), b"-42");
}

#[test]
fn sb_append_no_truncation() {
    let mut heap = ObjectHeap::new();
    let b = heap.sb_alloc().unwrap();
    let long_str = [b'x'; 70];
    heap.sb_append_bytes(b, &long_str);
    assert_eq!(heap.sb_len(b), 70);
}

/// Two builders alive at once must not interleave. Under the old shared
/// LIFO stack every append landed in whichever builder was constructed
/// last, so `outer` came back empty.
#[test]
fn sb_interleaved_builders_stay_independent() {
    let mut heap = ObjectHeap::new();
    let outer = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(outer, b"foo");
    let inner = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(inner, b"bar");
    // Interleave: appending to the older builder still reaches it.
    heap.sb_append_int(outer, 7);
    assert_eq!(heap.sb_contents_slice(inner), b"bar");
    assert_eq!(heap.sb_contents_slice(outer), b"foo7");
}

#[test]
fn sb_nested_builders_preserve_content() {
    let mut heap = ObjectHeap::new();
    // Outer: "hi " + (inner) + 42
    let outer = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(outer, b"hi ");
    // Inner: "Hello, World!" + " bye "
    let inner = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(inner, b"Hello, World!");
    heap.sb_append_bytes(inner, b" bye ");
    assert_eq!(heap.sb_contents_slice(inner), b"Hello, World! bye ");
    // Outer is untouched by the inner builder's appends.
    assert_eq!(heap.sb_contents_slice(outer), b"hi ");
    let inner_bytes = heap.sb_contents_slice(inner).to_vec();
    heap.sb_append_bytes(outer, &inner_bytes);
    heap.sb_append_int(outer, 42);
    assert_eq!(heap.sb_contents_slice(outer), b"hi Hello, World! bye 42");
}

#[test]
fn sb_free_releases_slot_for_reuse() {
    let mut heap = ObjectHeap::new();
    let a = heap.sb_alloc().unwrap();
    heap.sb_append_bytes(a, b"gone");
    heap.sb_free(a);
    // The freed slot is handed out again, zeroed.
    let b = heap.sb_alloc().unwrap();
    assert_eq!(b, a);
    assert_eq!(heap.sb_len(b), 0);
}

#[test]
fn gc_slot_reuse() {
    let mut heap = ObjectHeap::new();
    assert_eq!(heap.alloc("A"), Some(0));
    assert_eq!(heap.alloc("B"), Some(1));
    // Simulate GC freeing slot 0
    heap.objects[0] = None;
    heap.first_free = 0;
    // Next alloc should reuse slot 0
    assert_eq!(heap.alloc("C"), Some(0));
    // Slot 1 still intact
    assert_eq!(heap.class_name(1), Some("B"));
}

#[test]
fn float_to_str_special() {
    let mut buf = [0u8; 32];
    assert_eq!(float_to_str_buf(f32::NAN, &mut buf), b"NaN");
    assert_eq!(float_to_str_buf(f32::INFINITY, &mut buf), b"Infinity");
    assert_eq!(float_to_str_buf(f32::NEG_INFINITY, &mut buf), b"-Infinity");
}

#[test]
fn float_to_str_zero() {
    let mut buf = [0u8; 32];
    let s = float_to_str_buf(0.0, &mut buf);
    assert_eq!(s, b"0.0");
}

#[test]
fn float_to_str_integer() {
    let mut buf = [0u8; 32];
    let s = float_to_str_buf(42.0, &mut buf);
    // 42.0 → "42.0"
    assert_eq!(s, b"42.0");
}

#[test]
fn float_to_str_negative() {
    let mut buf = [0u8; 32];
    let s = float_to_str_buf(-3.14, &mut buf);
    // Should start with "-3."
    assert!(s.starts_with(b"-3."));
}

// ── list_bufs tests ──────────────────────────────────────────────────────

#[test]
fn list_alloc_returns_sequential_indices() {
    let mut heap = ObjectHeap::new();
    assert_eq!(heap.list_alloc(), Some(0));
    assert_eq!(heap.list_alloc(), Some(1));
    assert_eq!(heap.list_alloc(), Some(2));
}

#[test]
fn list_add_and_get() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(10));
    heap.list_add(idx, Value::Int(20));
    assert_eq!(heap.list_len(idx), 2);
    assert_eq!(heap.list_get(idx, 0), Some(Value::Int(10)));
    assert_eq!(heap.list_get(idx, 1), Some(Value::Int(20)));
    assert_eq!(heap.list_get(idx, 2), None);
}

#[test]
fn list_set_returns_old_value() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(1));
    let old = heap.list_set(idx, 0, Value::Int(99));
    assert_eq!(old, Some(Value::Int(1)));
    assert_eq!(heap.list_get(idx, 0), Some(Value::Int(99)));
}

#[test]
fn list_remove_returns_value_and_shifts() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(1));
    heap.list_add(idx, Value::Int(2));
    heap.list_add(idx, Value::Int(3));
    let removed = heap.list_remove(idx, 1);
    assert_eq!(removed, Some(Value::Int(2)));
    assert_eq!(heap.list_len(idx), 2);
    assert_eq!(heap.list_get(idx, 1), Some(Value::Int(3)));
}

#[test]
fn list_insert_shifts_right() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(1));
    heap.list_add(idx, Value::Int(3));
    heap.list_insert(idx, 1, Value::Int(2));
    assert_eq!(heap.list_len(idx), 3);
    assert_eq!(heap.list_get(idx, 1), Some(Value::Int(2)));
    assert_eq!(heap.list_get(idx, 2), Some(Value::Int(3)));
}

#[test]
fn list_clear_empties_list() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(1));
    heap.list_add(idx, Value::Int(2));
    heap.list_clear(idx);
    assert_eq!(heap.list_len(idx), 0);
}

#[test]
fn list_free_slot_is_reused() {
    let mut heap = ObjectHeap::new();
    let idx0 = heap.list_alloc().unwrap();
    let _idx1 = heap.list_alloc().unwrap();
    heap.list_free(idx0);
    let reused = heap.list_alloc().unwrap();
    assert_eq!(reused, idx0);
}

#[test]
fn list_iter_yields_elements() {
    let mut heap = ObjectHeap::new();
    let idx = heap.list_alloc().unwrap();
    heap.list_add(idx, Value::Int(7));
    heap.list_add(idx, Value::Int(8));
    let collected: alloc::vec::Vec<Value> = heap.list_iter(idx).collect();
    assert_eq!(collected, [Value::Int(7), Value::Int(8)]);
}

/// G10: a slice buffer smaller than the live set compacts the fields arena
/// in several passes and lands exactly where the one-pass compaction did.
#[test]
fn fields_compaction_in_bounded_slices_matches_one_pass() {
    let mut heap = ObjectHeap::new();
    let mut objs = Vec::new();
    for n in 0..60u16 {
        let o = heap.alloc("A").unwrap();
        for f in 0..(1 + n as usize % 5) {
            heap.set_field(o, f, Value::Int(n as i32 * 10 + f as i32))
                .unwrap();
        }
        objs.push(o);
    }
    for n in (0..60).step_by(3) {
        heap.free(objs[n]);
    }
    let live_span: usize = heap
        .objects
        .iter()
        .flatten()
        .map(|o| o.fields_cap as usize)
        .sum();
    // Four keys per pass: 40 survivors take ten passes.
    let mut buf = Vec::with_capacity(4);
    heap.compact_fields_arena(&mut buf);
    assert_eq!(buf.capacity(), 4, "the slice buffer must never regrow");
    assert_eq!(heap.fields_arena.len(), live_span);
    for n in 0..60usize {
        if n % 3 == 0 {
            continue;
        }
        for f in 0..(1 + n % 5) {
            assert_eq!(
                heap.get_field(objs[n], f),
                Some(Value::Int(n as i32 * 10 + f as i32))
            );
        }
    }
    #[cfg(feature = "mem-diag")]
    assert!(heap.integrity_check().is_ok());
}
