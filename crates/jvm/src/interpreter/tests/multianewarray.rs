// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// Hand-crafted class file with an extra Class/Utf8 CP entry for descriptor
// "[[I". build_class's fixed CP doesn't have this entry.
//
// CP layout (10 entries incl. sentinel):
//   #1 Class -> #2      #6 Utf8 "()I"
//   #2 Utf8  "T"        #7 Utf8 "Code"
//   #3 Class -> #4      #8 Class -> #9
//   #4 Utf8  "java/lang/Object"
//   #5 Utf8  "m"        #9 Utf8 "[[I"
//
// Bytecode:
//   iconst_2, iconst_3, multianewarray #8 dim=2, astore_1,
//   aload_1, iconst_0, aaload, iconst_1, bipush 42, iastore,
//   aload_1, iconst_0, aaload, iconst_1, iaload, ireturn
// Writes 42 to grid[0][1], reads it back. Expected: 42.
static CLASS_MULTIANEWARRAY: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, // header + cp_count=10
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x01, 0x54, // #2 "T"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, 0x6A, 0x61, 0x76, 0x61, 0x2F, 0x6C, 0x61, 0x6E, 0x67, 0x2F, 0x4F, 0x62, 0x6A,
    0x65, 0x63, 0x74, // #4 "java/lang/Object"
    0x01, 0x00, 0x01, 0x6D, // #5 "m"
    0x01, 0x00, 0x03, 0x28, 0x29, 0x49, // #6 "()I"
    0x01, 0x00, 0x04, 0x43, 0x6F, 0x64, 0x65, // #7 "Code"
    0x07, 0x00, 0x09, // #8 Class -> #9
    0x01, 0x00, 0x03, 0x5B, 0x5B, 0x49, // #9 "[[I"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00,
    0x01, // method: access=1, name=#5, desc=#6, attrs=1
    0x00, 0x07, 0x00, 0x00, 0x00, 0x20, // Code attr name=#7, length=32
    0x00, 0x03, 0x00, 0x02, 0x00, 0x00, 0x00, 0x14, // max_stack=3, max_locals=2, code_len=20
    0x05, // iconst_2
    0x06, // iconst_3
    0xC5, 0x00, 0x08, 0x02, // multianewarray #8 dims=2
    0x4C, // astore_1
    0x2B, // aload_1
    0x03, // iconst_0
    0x32, // aaload
    0x04, // iconst_1
    0x10, 0x2A, // bipush 42
    0x4F, // iastore
    0x2B, // aload_1
    0x03, // iconst_0
    0x32, // aaload
    0x04, // iconst_1
    0x2E, // iaload
    0xAC, // ireturn
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // ex_table=0, code_attrs=0, class_attrs=0
];

#[test]
fn multianewarray_2d_int_write_read() {
    assert_eq!(run(CLASS_MULTIANEWARRAY).unwrap(), Some(Value::Int(42)));
}

// Multianewarray with only outer-dim allocated — descriptor "[[I" with
// dims=1 leaves the inner int[] refs as null. We verify the outer length.
// Bytecode: iconst_4, multianewarray #8 dim=1, arraylength, ireturn
static CLASS_MULTIANEWARRAY_PARTIAL: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, 0x07, 0x00, 0x02, 0x01, 0x00, 0x01,
    0x54, 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, 0x6A, 0x61, 0x76, 0x61, 0x2F, 0x6C, 0x61, 0x6E, 0x67,
    0x2F, 0x4F, 0x62, 0x6A, 0x65, 0x63, 0x74, 0x01, 0x00, 0x01, 0x6D, 0x01, 0x00, 0x03, 0x28, 0x29,
    0x49, 0x01, 0x00, 0x04, 0x43, 0x6F, 0x64, 0x65, 0x07, 0x00, 0x09, 0x01, 0x00, 0x03, 0x5B, 0x5B,
    0x49, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
    0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x13, // Code attr_len=19
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, // max_stack=1, max_locals=1, code_len=7
    0x07, // iconst_4
    0xC5, 0x00, 0x08, 0x01, // multianewarray #8 dims=1
    0xBE, // arraylength
    0xAC, // ireturn
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// `CLASS_MULTIANEWARRAY` with `iconst_2, iconst_3` replaced by `with`
/// (bytes that leave two ints), the Code lengths adjusted to match.
fn multianewarray_of(with: &[u8]) -> &'static [u8] {
    let mut class = CLASS_MULTIANEWARRAY.to_vec();
    let at = class
        .windows(4)
        .position(|w| w == [0x05, 0x06, 0xC5, 0x00])
        .unwrap();
    class.splice(at..at + 2, with.iter().copied());
    let attr = class
        .windows(6)
        .position(|w| w == [0x00, 0x07, 0x00, 0x00, 0x00, 0x20])
        .unwrap();
    let grow = with.len() as u8 - 2;
    class[attr + 5] += grow; // Code attribute length
    class[attr + 13] += grow; // code_length
    alloc::boxed::Box::leak(class.into_boxed_slice())
}

/// `new int[200][1000]` is 800 KB of ints on a 64 KB heap. Each attempt
/// fails part way through and the emergency GC used to free the partial
/// tree, so "the GC freed something" let the retry run again, for ever. The
/// tree is handed back as soon as it fails now, the GC frees nothing, and
/// the program gets its OutOfMemoryError.
#[test]
fn multianewarray_too_big_for_the_heap_throws_instead_of_spinning() {
    let class = multianewarray_of(&[0x11, 0x00, 0xC8, 0x11, 0x03, 0xE8]);
    let r = crate::test_alloc::with_budget(64 * 1024, || run(class));
    assert!(r.is_err(), "{r:?}");
}

/// `new int[65535][1]` needs 65,536 arrays, one more than there are array
/// references. It is an OutOfMemoryError at once, as `newarray` past `u16`
/// is: allocated array by array it used to wrap the 65,536th reference to
/// the first, and refused array by array it made GC-and-retry spin for
/// ever, each retry freeing the partial tree the last one built.
#[test]
fn multianewarray_past_the_reference_space_is_oom_not_a_spin() {
    // sipush 255, sipush 257, imul, iconst_1: counts 65535 and 1.
    let class = multianewarray_of(&[0x11, 0x00, 0xFF, 0x11, 0x01, 0x01, 0x68, 0x04]);
    assert!(run(class).is_err());
}

#[test]
fn multianewarray_outer_dim_only() {
    assert_eq!(
        run(CLASS_MULTIANEWARRAY_PARTIAL).unwrap(),
        Some(Value::Int(4))
    );
}
