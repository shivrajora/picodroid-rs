// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Arrays native method tests ───────────────────────────────────────────

#[test]
fn arrays_sort_int_random() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[5, 3, 8, 1, 9, 2, 7, 4, 6]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(
        read_int_array(&arrays, idx),
        alloc::vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
}

#[test]
fn arrays_sort_int_already_sorted() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3, 4, 5]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, idx), alloc::vec![1, 2, 3, 4, 5]);
}

#[test]
fn arrays_sort_int_large_uses_quicksort_path() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    // Length > INSERTION_THRESHOLD to exercise the sort_unstable code path.
    let mut vs: alloc::vec::Vec<i32> = (0..32).rev().collect();
    let idx = make_int_array(&mut arrays, &vs);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    vs.sort();
    assert_eq!(read_int_array(&arrays, idx), vs);
}

#[test]
fn arrays_sort_int_empty_and_single_no_op() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let empty = make_int_array(&mut arrays, &[]);
    let single = make_int_array(&mut arrays, &[42]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(empty)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(single)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(arrays.length(empty), Some(0));
    assert_eq!(read_int_array(&arrays, single), alloc::vec![42]);
}

#[test]
fn arrays_sort_long() {
    use crate::array_heap::ATYPE_LONG;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, 5).unwrap();
    for (i, v) in [3i64, -10, 0, 7, -1].iter().enumerate() {
        arrays.store64(idx, i, *v).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([J)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i64> = (0..5).map(|i| arrays.load64(idx, i).unwrap()).collect();
    assert_eq!(got, alloc::vec![-10, -1, 0, 3, 7]);
}

#[test]
fn arrays_sort_double_with_nan_total_cmp() {
    use crate::array_heap::ATYPE_DOUBLE;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_DOUBLE, 4).unwrap();
    let input = [f64::NAN, 1.0, -2.5, 3.5];
    for (i, v) in input.iter().enumerate() {
        arrays.store64(idx, i, v.to_bits() as i64).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([D)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<f64> = (0..4)
        .map(|i| f64::from_bits(arrays.load64(idx, i).unwrap() as u64))
        .collect();
    // total_cmp sorts NaN last.
    assert_eq!(got[0], -2.5);
    assert_eq!(got[1], 1.0);
    assert_eq!(got[2], 3.5);
    assert!(got[3].is_nan());
}

#[test]
fn arrays_sort_byte_sign_extends() {
    use crate::array_heap::ATYPE_BYTE;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_BYTE, 4).unwrap();
    // Stored as i32, but logically byte: -1 (0xFF) must sort BELOW 1.
    arrays.store(idx, 0, 1).unwrap();
    arrays.store(idx, 1, -1).unwrap();
    arrays.store(idx, 2, 0).unwrap();
    arrays.store(idx, 3, -128).unwrap();
    arrays_dispatch(
        m::sort,
        "([B)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i32> = (0..4).map(|i| arrays.load(idx, i).unwrap()).collect();
    assert_eq!(got, alloc::vec![-128, -1, 0, 1]);
}

#[test]
fn arrays_fill_and_to_string_boolean_and_char() {
    // boolean[] had no arm at all (hard InvalidReference); char[] printed
    // code points ("[97, 98]") instead of the characters.
    use crate::array_heap::{ATYPE_BOOLEAN, ATYPE_CHAR};
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let b = arrays.alloc(ATYPE_BOOLEAN, 3).unwrap();
    arrays_dispatch(
        m::fill,
        "([ZZ)V",
        &[Value::ArrayRef(b), Value::Int(1)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, b), alloc::vec![1, 1, 1]);
    arrays.store(b, 1, 0);
    assert_eq!(
        arrays_to_string_str(d::aZ__String, b, &mut strings, &mut objects, &mut arrays),
        "[true, false, true]"
    );
    let c = arrays.alloc(ATYPE_CHAR, 2).unwrap();
    arrays.store(c, 0, b'a' as i32);
    arrays.store(c, 1, b'b' as i32);
    assert_eq!(
        arrays_to_string_str(d::aC__String, c, &mut strings, &mut objects, &mut arrays),
        "[a, b]"
    );
}

#[test]
fn arrays_fill_and_sort_range_overloads() {
    // fill(a, from, to, v) used to read `from` as the value and fill the
    // whole array; sort(a, from, to) sorted everything.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let a = make_int_array(&mut arrays, &[0, 0, 0, 0, 0]);
    arrays_dispatch(
        m::fill,
        "([IIII)V",
        &[
            Value::ArrayRef(a),
            Value::Int(1),
            Value::Int(3),
            Value::Int(9),
        ],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, a), alloc::vec![0, 9, 9, 0, 0]);

    let s = make_int_array(&mut arrays, &[5, 4, 3, 2, 1]);
    arrays_dispatch(
        m::sort,
        "([III)V",
        &[Value::ArrayRef(s), Value::Int(1), Value::Int(4)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, s), alloc::vec![5, 2, 3, 4, 1]);

    // Bad ranges throw like Java: to > length -> AIOOBE, from > to -> IAE.
    let r = arrays_dispatch(
        m::fill,
        "([IIII)V",
        &[
            Value::ArrayRef(a),
            Value::Int(1),
            Value::Int(9),
            Value::Int(0),
        ],
        &mut strings,
        &mut objects,
        &mut arrays,
    );
    let Err(JvmError::Exception(idx)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(idx),
        Some(c::java_lang_ArrayIndexOutOfBoundsException)
    );
    let r = arrays_dispatch(
        m::sort,
        "([III)V",
        &[Value::ArrayRef(a), Value::Int(3), Value::Int(1)],
        &mut strings,
        &mut objects,
        &mut arrays,
    );
    let Err(JvmError::Exception(idx)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(idx),
        Some(c::java_lang_IllegalArgumentException)
    );
}

#[test]
fn arrays_fill_object_array() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let arr = arrays.alloc(crate::array_heap::ATYPE_REF, 2).unwrap();
    let s = Value::Reference(strings.intern(m::x.as_bytes()).unwrap());
    arrays_dispatch(
        m::fill,
        d::aObject_Object__V,
        &[Value::ArrayRef(arr), s],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(
        crate::array_heap::decode_ref(arrays.load(arr, 1).unwrap()),
        s
    );
}

#[test]
fn arrays_fill_int() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[0, 0, 0, 0, 0]);
    arrays_dispatch(
        m::fill,
        "([II)V",
        &[Value::ArrayRef(idx), Value::Int(7)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, idx), alloc::vec![7; 5]);
}

#[test]
fn arrays_fill_long() {
    use crate::array_heap::ATYPE_LONG;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, 4).unwrap();
    arrays_dispatch(
        m::fill,
        "([JJ)V",
        &[Value::ArrayRef(idx), Value::Long(0xCAFE_BABE)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    for i in 0..4 {
        assert_eq!(arrays.load64(idx, i), Some(0xCAFE_BABE));
    }
}

#[test]
fn arrays_copy_of_grow_zero_pads() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3]);
    let result = arrays_dispatch(
        m::copyOf,
        "([II)[I",
        &[Value::ArrayRef(idx), Value::Int(5)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let new_idx = match result {
        Value::ArrayRef(i) => i,
        _ => panic!("expected ArrayRef"),
    };
    assert_eq!(read_int_array(&arrays, new_idx), alloc::vec![1, 2, 3, 0, 0]);
}

#[test]
fn arrays_copy_of_shrink_truncates() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3, 4, 5]);
    let result = arrays_dispatch(
        m::copyOf,
        "([II)[I",
        &[Value::ArrayRef(idx), Value::Int(2)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let new_idx = match result {
        Value::ArrayRef(i) => i,
        _ => panic!("expected ArrayRef"),
    };
    assert_eq!(read_int_array(&arrays, new_idx), alloc::vec![1, 2]);
}

#[test]
fn arrays_to_string_int() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3]);
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "[1, 2, 3]");
}

#[test]
fn arrays_to_string_empty() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[]);
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "[]");
}

#[test]
fn arrays_to_string_null() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::Null],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "null");
}
