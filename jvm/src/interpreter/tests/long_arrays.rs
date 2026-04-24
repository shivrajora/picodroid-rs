use super::*;

// long[] roundtrip: allocate long[3], store lconst_1 at [0], laload it back,
// truncate via l2i, return. Expected: 1.
#[test]
fn lastore_laload_roundtrip() {
    let code = &[
        0x06, // iconst_3
        0xBC, 0x0B, // newarray long (atype 11)
        0x4C, // astore_1
        0x2B, // aload_1
        0x03, // iconst_0 (index)
        0x0A, // lconst_1
        0x50, // lastore
        0x2B, // aload_1
        0x03, // iconst_0
        0x2F, // laload
        0x88, // l2i
        0xAC, // ireturn
    ];
    assert_eq!(run_code(3, 2, code).unwrap(), Some(Value::Int(1)));
}

// double[] roundtrip: allocate double[2], store dconst_1 at [1], daload,
// d2i, return. Expected: 1.
#[test]
fn dastore_daload_roundtrip() {
    let code = &[
        0x05, // iconst_2
        0xBC, 0x07, // newarray double (atype 7)
        0x4C, // astore_1
        0x2B, // aload_1
        0x04, // iconst_1 (index)
        0x0F, // dconst_1
        0x52, // dastore
        0x2B, // aload_1
        0x04, // iconst_1
        0x31, // daload
        0x8E, // d2i
        0xAC, // ireturn
    ];
    assert_eq!(run_code(3, 2, code).unwrap(), Some(Value::Int(1)));
}

// arraylength on long[] returns the user-visible element count, not the
// underlying i32-slot count.
#[test]
fn long_array_length_is_user_visible() {
    let code = &[
        0x10, 0x07, // bipush 7
        0xBC, 0x0B, // newarray long
        0xBE, // arraylength
        0xAC, // ireturn
    ];
    assert_eq!(run_code(1, 1, code).unwrap(), Some(Value::Int(7)));
}
