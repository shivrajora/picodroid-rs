// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn ref_encoding_round_trips_object_zero() {
    // Object slot 0 is a normal slot; it must not alias Null.
    let raw = encode_ref(Value::ObjectRef(0)).unwrap();
    assert_ne!(raw, 0);
    assert_eq!(decode_ref(raw), Value::ObjectRef(0));
    assert_eq!(decode_ref(0), Value::Null);
    assert_eq!(encode_ref(Value::Null), Some(0));
    for v in [Value::ObjectRef(7), Value::Reference(7), Value::ArrayRef(7)] {
        assert_eq!(decode_ref(encode_ref(v).unwrap()), v);
    }
    assert_eq!(encode_ref(Value::Int(3)), None);
}

#[test]
fn alloc_returns_sequential_indices() {
    let mut heap = ArrayHeap::new();
    assert_eq!(heap.alloc(ATYPE_INT, 4), Some(0));
    assert_eq!(heap.alloc(ATYPE_BYTE, 8), Some(1));
    assert_eq!(heap.alloc(ATYPE_CHAR, 2), Some(2));
}

#[test]
fn alloc_beyond_old_capacity_succeeds() {
    let mut heap = ArrayHeap::new();
    for i in 0..64u16 {
        assert_eq!(heap.alloc(ATYPE_INT, 1), Some(i));
    }
}

#[test]
fn alloc_large_array_succeeds() {
    let mut heap = ArrayHeap::new();
    assert_eq!(heap.alloc(ATYPE_INT, 1000), Some(0));
    assert_eq!(heap.length(0), Some(1000));
}

#[test]
fn alloc_zero_length_succeeds() {
    let mut heap = ArrayHeap::new();
    assert_eq!(heap.alloc(ATYPE_INT, 0), Some(0));
    assert_eq!(heap.length(0), Some(0));
}

#[test]
fn length_returns_correct_value() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 7);
    assert_eq!(heap.length(0), Some(7));
}

#[test]
fn store_and_load_int_roundtrip() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 4);
    assert_eq!(heap.store(0, 2, 99), Some(()));
    assert_eq!(heap.load(0, 2), Some(99));
}

// ── Packed byte[]/boolean[] (Inline8/Arena8) ────────────────────────────

#[test]
fn packed_byte_roundtrip_inline_and_arena() {
    let mut heap = ArrayHeap::new();
    let small = heap.alloc(ATYPE_BYTE, 8).unwrap(); // inline
    let big = heap.alloc(ATYPE_BYTE, 200).unwrap(); // arena8
    for (idx, len) in [(small, 8usize), (big, 200usize)] {
        for i in 0..len {
            assert_eq!(heap.store(idx, i, (i as i32 % 251) - 128), Some(()));
        }
        for i in 0..len {
            let expect = ((i as i32 % 251) - 128) as i8 as i32;
            assert_eq!(heap.load(idx, i), Some(expect), "idx={idx} i={i}");
        }
        assert_eq!(heap.length(idx), Some(len as u16));
    }
    // Payload accounting: 200-byte array costs 200 arena8 bytes, not 800.
    assert_eq!(heap.live_bytes(), 2 * 40 + 200);
}

#[test]
fn packed_byte_sign_extends_on_load() {
    let mut heap = ArrayHeap::new();
    let idx = heap.alloc(ATYPE_BYTE, 4).unwrap();
    heap.store(idx, 0, -1).unwrap();
    heap.store(idx, 1, 0x7f).unwrap();
    heap.store(idx, 2, -128).unwrap();
    assert_eq!(heap.load(idx, 0), Some(-1));
    assert_eq!(heap.load(idx, 1), Some(127));
    assert_eq!(heap.load(idx, 2), Some(-128));
}

#[test]
fn packed_boolean_roundtrip() {
    let mut heap = ArrayHeap::new();
    let idx = heap.alloc(ATYPE_BOOLEAN, 100).unwrap();
    heap.store(idx, 0, 1).unwrap();
    heap.store(idx, 99, 1).unwrap();
    assert_eq!(heap.load(idx, 0), Some(1));
    assert_eq!(heap.load(idx, 50), Some(0));
    assert_eq!(heap.load(idx, 99), Some(1));
}

#[test]
fn packed_clone_copies_payload() {
    let mut heap = ArrayHeap::new();
    let idx = heap.alloc(ATYPE_BYTE, 64).unwrap();
    for i in 0..64 {
        heap.store(idx, i, i as i32).unwrap();
    }
    let copy = heap.clone(idx).unwrap();
    heap.store(idx, 0, 42).unwrap(); // clone must be independent
    assert_eq!(heap.load(copy, 0), Some(0));
    for i in 1..64 {
        assert_eq!(heap.load(copy, i), Some(i as i32));
    }
}

#[test]
fn packed_compaction_reclaims_and_relocates() {
    let mut heap = ArrayHeap::new();
    let a = heap.alloc(ATYPE_BYTE, 100).unwrap();
    let b = heap.alloc(ATYPE_BYTE, 100).unwrap();
    let c = heap.alloc(ATYPE_BYTE, 100).unwrap();
    for i in 0..100 {
        heap.store(a, i, 1).unwrap();
        heap.store(b, i, 2).unwrap();
        heap.store(c, i, 3).unwrap();
    }
    heap.free(b);
    let mut buf = Vec::new();
    heap.compact_arena(&mut buf);
    // b's 100 bytes reclaimed; a and c intact after the slide.
    assert_eq!(heap.arena8.len(), 200);
    for i in 0..100 {
        assert_eq!(heap.load(a, i), Some(1));
        assert_eq!(heap.load(c, i), Some(3));
    }
}

#[test]
fn packed_and_i32_arenas_are_independent() {
    let mut heap = ArrayHeap::new();
    let ints = heap.alloc(ATYPE_INT, 50).unwrap();
    let bytes = heap.alloc(ATYPE_BYTE, 50).unwrap();
    for i in 0..50 {
        heap.store(ints, i, 1000 + i as i32).unwrap();
        heap.store(bytes, i, i as i32).unwrap();
    }
    let mut buf = Vec::new();
    heap.compact_arena(&mut buf); // both passes run, nothing freed
    for i in 0..50 {
        assert_eq!(heap.load(ints, i), Some(1000 + i as i32));
        assert_eq!(heap.load(bytes, i), Some(i as i32));
    }
    assert_eq!(heap.arena.len(), 50);
    assert_eq!(heap.arena8.len(), 50);
}

#[test]
fn elements_default_to_zero() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 4);
    assert_eq!(heap.load(0, 0), Some(0));
    assert_eq!(heap.load(0, 3), Some(0));
}

#[test]
fn load_out_of_bounds_returns_none() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 3);
    assert_eq!(heap.load(0, 3), None);
    assert_eq!(heap.load(0, 10), None);
}

#[test]
fn store_out_of_bounds_returns_none() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 3);
    assert_eq!(heap.store(0, 3, 1), None);
}

#[test]
fn load_invalid_array_index_returns_none() {
    let heap = ArrayHeap::new();
    assert_eq!(heap.load(99, 0), None);
}

#[test]
fn byte_sign_extension_semantics() {
    // Store -128 as byte (i8), load back as i32 should be -128
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_BYTE, 2);
    // Store raw i32 value that represents byte -128
    heap.store(0, 0, -128i32);
    let raw = heap.load(0, 0).unwrap();
    let as_byte = raw as i8 as i32;
    assert_eq!(as_byte, -128);
}

#[test]
fn char_zero_extension_semantics() {
    // Store 0xFFFF as char, load back as i32 zero-extended should be 65535
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_CHAR, 1);
    heap.store(0, 0, 0xFFFFu16 as i32);
    let raw = heap.load(0, 0).unwrap();
    let as_char = raw as u16 as i32;
    assert_eq!(as_char, 65535);
}

#[test]
fn atype_returns_correct_value() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_BYTE, 4);
    heap.alloc(ATYPE_CHAR, 2);
    assert_eq!(heap.atype(0), Some(ATYPE_BYTE));
    assert_eq!(heap.atype(1), Some(ATYPE_CHAR));
}

#[test]
fn gc_slot_reuse() {
    let mut heap = ArrayHeap::new();
    assert_eq!(heap.alloc(ATYPE_INT, 4), Some(0));
    assert_eq!(heap.alloc(ATYPE_INT, 8), Some(1));
    // Simulate GC freeing slot 0
    heap.arrays[0] = None;
    heap.first_free = 0;
    // Next alloc should reuse slot 0
    assert_eq!(heap.alloc(ATYPE_BYTE, 2), Some(0));
    // Slot 1 still intact
    assert_eq!(heap.length(1), Some(8));
}

// ── Arena-backed array tests ────────────────────────────────────────────

#[test]
fn arena_load_store_roundtrip() {
    let mut heap = ArrayHeap::new();
    // 20 elements > INLINE_DATA(8) → arena-backed
    heap.alloc(ATYPE_INT, 20);
    for i in 0..20 {
        assert_eq!(heap.store(0, i, (i * 10) as i32), Some(()));
    }
    for i in 0..20 {
        assert_eq!(heap.load(0, i), Some((i * 10) as i32));
    }
}

#[test]
fn arena_data_slice() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 10);
    heap.store(0, 0, 100);
    heap.store(0, 9, 999);
    let slice = heap.data_slice(0);
    assert_eq!(slice.len(), 10);
    assert_eq!(slice[0], 100);
    assert_eq!(slice[9], 999);
}

#[test]
fn arena_multiple_arrays() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_INT, 10); // slot 0, arena [0..10)
    heap.alloc(ATYPE_INT, 20); // slot 1, arena [10..30)
    heap.store(0, 5, 55);
    heap.store(1, 15, 1515);
    assert_eq!(heap.load(0, 5), Some(55));
    assert_eq!(heap.load(1, 15), Some(1515));
    // Verify arena contains both arrays' data
    assert_eq!(heap.arena.len(), 30);
}

#[test]
fn arena_compaction_reclaims_space() {
    let mut heap = ArrayHeap::new();
    let mut buf = Vec::new();
    // Allocate 3 arena-backed arrays of 10 elements each
    heap.alloc(ATYPE_INT, 10); // slot 0
    heap.alloc(ATYPE_INT, 10); // slot 1
    heap.alloc(ATYPE_INT, 10); // slot 2
                               // Write sentinel values
    heap.store(0, 0, 111);
    heap.store(1, 0, 222);
    heap.store(2, 0, 333);
    assert_eq!(heap.arena.len(), 30);

    // Free the middle array
    heap.free(1);
    heap.compact_arena(&mut buf);

    // Arena should shrink: 2 live arrays * 10 = 20
    assert_eq!(heap.arena.len(), 20);
    // Surviving data intact
    assert_eq!(heap.load(0, 0), Some(111));
    assert_eq!(heap.load(2, 0), Some(333));
}

#[test]
fn arena_compaction_updates_offsets() {
    let mut heap = ArrayHeap::new();
    let mut buf = Vec::new();
    heap.alloc(ATYPE_INT, 10); // slot 0
    heap.alloc(ATYPE_INT, 10); // slot 1
    heap.alloc(ATYPE_INT, 10); // slot 2
                               // Fill each with distinct pattern
    for i in 0..10 {
        heap.store(0, i, 100 + i as i32);
        heap.store(1, i, 200 + i as i32);
        heap.store(2, i, 300 + i as i32);
    }
    // Free first array, compact
    heap.free(0);
    heap.compact_arena(&mut buf);

    assert_eq!(heap.arena.len(), 20);
    // Array at slot 1 should now start at offset 0
    for i in 0..10 {
        assert_eq!(heap.load(1, i), Some(200 + i as i32));
    }
    // Array at slot 2 should start at offset 10
    for i in 0..10 {
        assert_eq!(heap.load(2, i), Some(300 + i as i32));
    }
}

#[test]
fn arena_alloc_after_compact_reuses_space() {
    let mut heap = ArrayHeap::new();
    let mut buf = Vec::new();
    heap.alloc(ATYPE_INT, 10); // slot 0
    heap.alloc(ATYPE_INT, 10); // slot 1
    heap.store(1, 0, 42);

    // Free all and compact
    heap.free(0);
    heap.free(1);
    heap.compact_arena(&mut buf);
    assert_eq!(heap.arena.len(), 0);

    // New allocation reuses slot 0 and appends to (now empty) arena
    assert_eq!(heap.alloc(ATYPE_INT, 10), Some(0));
    heap.store(0, 5, 99);
    assert_eq!(heap.load(0, 5), Some(99));
    assert_eq!(heap.arena.len(), 10);
}

#[test]
fn arena_mixed_inline_and_arena() {
    let mut heap = ArrayHeap::new();
    let mut buf = Vec::new();
    heap.alloc(ATYPE_INT, 4); // slot 0, inline
    heap.alloc(ATYPE_INT, 20); // slot 1, arena
    heap.alloc(ATYPE_INT, 2); // slot 2, inline
    heap.alloc(ATYPE_INT, 15); // slot 3, arena
    heap.store(0, 0, 1);
    heap.store(1, 10, 2);
    heap.store(2, 0, 3);
    heap.store(3, 10, 4);

    // Free one arena array, compact
    heap.free(1);
    heap.compact_arena(&mut buf);

    // Inline arrays unaffected
    assert_eq!(heap.load(0, 0), Some(1));
    assert_eq!(heap.load(2, 0), Some(3));
    // Surviving arena array intact
    assert_eq!(heap.load(3, 10), Some(4));
    // Arena shrunk to just the one live arena array
    assert_eq!(heap.arena.len(), 15);
}

// ── 64-bit element tests (ATYPE_LONG / ATYPE_DOUBLE) ───────────────────

#[test]
fn long_array_length_reports_user_visible_count() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_LONG, 5);
    assert_eq!(heap.length(0), Some(5));
}

#[test]
fn long_array_inline_roundtrip() {
    // 4 longs → 8 i32 slots, fits inline (INLINE_DATA == 8)
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_LONG, 4);
    heap.store64(0, 0, i64::MIN).unwrap();
    heap.store64(0, 1, -1).unwrap();
    heap.store64(0, 2, 0x1122_3344_5566_7788).unwrap();
    heap.store64(0, 3, i64::MAX).unwrap();
    assert_eq!(heap.load64(0, 0), Some(i64::MIN));
    assert_eq!(heap.load64(0, 1), Some(-1));
    assert_eq!(heap.load64(0, 2), Some(0x1122_3344_5566_7788));
    assert_eq!(heap.load64(0, 3), Some(i64::MAX));
}

#[test]
fn long_array_arena_roundtrip() {
    // 16 longs → 32 i32 slots → arena-backed
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_LONG, 16);
    for i in 0..16 {
        heap.store64(0, i, (i as i64) * 1_000_000_000_000).unwrap();
    }
    for i in 0..16 {
        assert_eq!(heap.load64(0, i), Some((i as i64) * 1_000_000_000_000));
    }
}

#[test]
fn long_array_out_of_bounds_returns_none() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_LONG, 3);
    assert_eq!(heap.load64(0, 3), None);
    assert_eq!(heap.store64(0, 3, 0), None);
}

#[test]
fn double_array_nan_roundtrip() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_DOUBLE, 2);
    // Use a specific NaN bit pattern to confirm we preserve all bits.
    let bits: u64 = 0x7ff8_0000_dead_beef;
    heap.store64(0, 0, bits as i64).unwrap();
    let raw = heap.load64(0, 0).unwrap() as u64;
    assert_eq!(raw, bits);
}

#[test]
fn long_array_does_not_alias_neighbors() {
    let mut heap = ArrayHeap::new();
    heap.alloc(ATYPE_LONG, 3);
    heap.store64(0, 1, 0x7777_7777_7777_7777).unwrap();
    // Neighbor slots stay zero
    assert_eq!(heap.load64(0, 0), Some(0));
    assert_eq!(heap.load64(0, 2), Some(0));
}

/// G10: a slice buffer smaller than the live set compacts in several
/// passes and lands exactly where the one-pass compaction did.
#[test]
fn compaction_in_bounded_slices_matches_one_pass() {
    let mut heap = ArrayHeap::new();
    let mut ints = Vec::new();
    let mut bytes = Vec::new();
    for n in 0..40 {
        let a = heap.alloc(ATYPE_INT, 50).unwrap();
        let b = heap.alloc(ATYPE_BYTE, 50).unwrap();
        for i in 0..50 {
            heap.store(a, i, n * 100 + i as i32).unwrap();
            heap.store(b, i, (n + i as i32) % 100).unwrap();
        }
        ints.push(a);
        bytes.push(b);
    }
    for n in (0..40).step_by(3) {
        heap.free(ints[n]);
        heap.free(bytes[n]);
    }
    // Three keys per pass: 26 survivors per arena take nine passes each.
    let mut buf = Vec::with_capacity(3);
    heap.compact_arena(&mut buf);
    assert_eq!(buf.capacity(), 3, "the slice buffer must never regrow");
    assert_eq!(heap.arena.len(), 26 * 50);
    assert_eq!(heap.arena8.len(), 26 * 50);
    for n in 0..40 {
        if n % 3 == 0 {
            continue;
        }
        for i in 0..50 {
            assert_eq!(heap.load(ints[n], i), Some(n as i32 * 100 + i as i32));
            assert_eq!(heap.load(bytes[n], i), Some((n as i32 + i as i32) % 100));
        }
    }
    #[cfg(feature = "mem-diag")]
    assert!(heap.integrity_check().is_ok());
}
