// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::array_heap::ATYPE_BYTE;
use crate::native::zip::crc32_update;

// ── java/util/zip/CRC32 native step tests ────────────────────────────────

fn make_byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> u16 {
    let idx = arrays.alloc(ATYPE_BYTE, bytes.len() as u16).unwrap();
    for (i, b) in bytes.iter().enumerate() {
        arrays.store(idx, i, *b as i8 as i32).unwrap();
    }
    idx
}

fn crc32_dispatch(
    method: &str,
    desc: &str,
    args: &[Value],
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects: &mut objects,
        arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_zip_CRC32, method, &mut ctx)
        .expect("CRC32 method not handled")
}

/// The check value every CRC-32 implementation quotes.
const CHECK: u32 = 0xCBF4_3926;

#[test]
fn crc32_check_value_and_empty_stream() {
    assert_eq!(crc32_update(0, b"123456789"), CHECK);
    assert_eq!(crc32_update(0, b""), 0);
}

#[test]
fn crc32_continues_a_finalised_value() {
    let head = crc32_update(0, b"1234");
    assert_eq!(crc32_update(head, b"56789"), CHECK);
    // Byte by byte through the `(int, int)` step gives the same stream.
    let mut crc = 0u32;
    for &b in b"123456789" {
        crc = crc32_update(crc, &[b]);
    }
    assert_eq!(crc, CHECK);
}

#[test]
fn crc32_update_bytes_native_over_a_range() {
    let mut arrays = ArrayHeap::new();
    let arr = make_byte_array(&mut arrays, b"xx123456789yy");
    let r = crc32_dispatch(
        m::updateBytes,
        "(I[BII)I",
        &[
            Value::Int(0),
            Value::ArrayRef(arr),
            Value::Int(2),
            Value::Int(9),
        ],
        &mut arrays,
    )
    .unwrap();
    assert_eq!(r, Some(Value::Int(CHECK as i32)));
    // Bytes above 0x7F are stored sign-extended; the step must see them unsigned.
    let arr = make_byte_array(&mut arrays, &[0xFF, 0x80]);
    let r = crc32_dispatch(
        m::updateBytes,
        "(I[BII)I",
        &[
            Value::Int(0),
            Value::ArrayRef(arr),
            Value::Int(0),
            Value::Int(2),
        ],
        &mut arrays,
    )
    .unwrap();
    assert_eq!(r, Some(Value::Int(crc32_update(0, &[0xFF, 0x80]) as i32)));
}

#[test]
fn crc32_update_int_native_takes_the_low_byte() {
    let mut arrays = ArrayHeap::new();
    let r = crc32_dispatch(
        m::update,
        "(II)I",
        &[Value::Int(0), Value::Int(0x1_31)],
        &mut arrays,
    )
    .unwrap();
    assert_eq!(r, Some(Value::Int(crc32_update(0, b"1") as i32)));
    // The public bytecode `update(int)` shares the name; its descriptor is not served here.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: "(I)V",
        args: &[Value::Int(0), Value::Int(1)],
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        upcall: None,
    };
    assert!(BuiltinHandler
        .dispatch(c::java_util_zip_CRC32, m::update, &mut ctx)
        .is_none());
}

#[test]
fn crc32_update_bytes_rejects_a_range_off_the_array() {
    let mut arrays = ArrayHeap::new();
    let arr = make_byte_array(&mut arrays, b"abc");
    for (off, len) in [(2, 2), (-1, 1), (0, -1), (4, 0)] {
        let r = crc32_dispatch(
            m::updateBytes,
            "(I[BII)I",
            &[
                Value::Int(0),
                Value::ArrayRef(arr),
                Value::Int(off),
                Value::Int(len),
            ],
            &mut arrays,
        );
        assert!(
            matches!(r, Err(JvmError::InvalidReference)),
            "off={off} len={len}"
        );
    }
}
