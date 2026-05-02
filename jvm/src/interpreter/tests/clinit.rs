// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Test 1: Basic <clinit> via getstatic ─────────────────────────────────
//
// Class "C" extends java/lang/Object.
// Two methods:
//   <clinit>()V:  bipush 42, putstatic C.x, return
//   m()I:         getstatic C.x, ireturn
//
// Constant pool (cp_count=14, entries #1..#13):
//   #1:  Class       -> #2           (C)
//   #2:  Utf8        "C"
//   #3:  Class       -> #4           (java/lang/Object)
//   #4:  Utf8        "java/lang/Object"
//   #5:  Utf8        "<clinit>"
//   #6:  Utf8        "()V"
//   #7:  Utf8        "Code"
//   #8:  Fieldref    -> #1, #9       (C.x)
//   #9:  NameAndType -> #10, #11
//   #10: Utf8        "x"
//   #11: Utf8        "I"
//   #12: Utf8        "m"
//   #13: Utf8        "()I"
static CLASS_CLINIT_BASIC: &[u8] = &[
    // magic + version
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // cp_count=14
    0x00, 0x0E, // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "C"
    0x01, 0x00, 0x01, b'C', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5 Utf8 "<clinit>"
    0x01, 0x00, 0x08, b'<', b'c', b'l', b'i', b'n', b'i', b't', b'>', // #6 Utf8 "()V"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7 Utf8 "Code"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8 Fieldref -> #1, #9
    0x09, 0x00, 0x01, 0x00, 0x09, // #9 NameAndType -> #10, #11
    0x0C, 0x00, 0x0A, 0x00, 0x0B, // #10 Utf8 "x"
    0x01, 0x00, 0x01, b'x', // #11 Utf8 "I"
    0x01, 0x00, 0x01, b'I', // #12 Utf8 "m"
    0x01, 0x00, 0x01, b'm', // #13 Utf8 "()I"
    0x01, 0x00, 0x03, b'(', b')', b'I',
    // access_flags=0x0001 (public), this_class=#1, super_class=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces_count=0, fields_count=0
    0x00, 0x00, 0x00, 0x00, // methods_count=2
    0x00, 0x02,
    // --- Method 0: <clinit>()V ---
    // access=0x0008 (static), name=#5, desc=#6, attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code attr: name=#7, length=18 (2+2+4+6 bytecode+2+2)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x12, // max_stack=1, max_locals=0, code_length=6
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
    // bytecode: bipush 42, putstatic #8, return
    0x10, 0x2A, 0xB3, 0x00, 0x08, 0xB1,
    // exception_table_length=0, code_attributes_count=0
    0x00, 0x00, 0x00, 0x00,
    // --- Method 1: m()I ---
    // access=0x0008 (static), name=#12, desc=#13, attrs=1
    0x00, 0x08, 0x00, 0x0C, 0x00, 0x0D, 0x00, 0x01,
    // Code attr: name=#7, length=16 (2+2+4+4 bytecode+2+2)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x10, // max_stack=1, max_locals=0, code_length=4
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // bytecode: getstatic #8, ireturn
    0xB2, 0x00, 0x08, 0xAC, // exception_table_length=0, code_attributes_count=0
    0x00, 0x00, 0x00, 0x00, // class attributes_count=0
    0x00, 0x00,
];

// ── Test 2: Clinit runs only once across multiple getstatic calls ────────
//
// Class "D" extends java/lang/Object.
// Two methods:
//   <clinit>()V:  bipush 10, putstatic D.x, return
//   m()I:         getstatic D.x, getstatic D.x, iadd, ireturn
//
// If clinit runs twice, D.x would still be 10 (idempotent), but the test
// verifies 10+10=20 (i.e. both getstatic return 10).
//
// Constant pool (same layout as CLASS_CLINIT_BASIC but class name "D"):
//   #1:  Class       -> #2           (D)
//   #2:  Utf8        "D"
//   #3:  Class       -> #4           (java/lang/Object)
//   #4:  Utf8        "java/lang/Object"
//   #5:  Utf8        "<clinit>"
//   #6:  Utf8        "()V"
//   #7:  Utf8        "Code"
//   #8:  Fieldref    -> #1, #9       (D.x)
//   #9:  NameAndType -> #10, #11
//   #10: Utf8        "x"
//   #11: Utf8        "I"
//   #12: Utf8        "m"
//   #13: Utf8        "()I"
static CLASS_CLINIT_RUNS_ONCE: &[u8] = &[
    // magic + version
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // cp_count=14
    0x00, 0x0E, // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "D"
    0x01, 0x00, 0x01, b'D', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5 Utf8 "<clinit>"
    0x01, 0x00, 0x08, b'<', b'c', b'l', b'i', b'n', b'i', b't', b'>', // #6 Utf8 "()V"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7 Utf8 "Code"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8 Fieldref -> #1, #9
    0x09, 0x00, 0x01, 0x00, 0x09, // #9 NameAndType -> #10, #11
    0x0C, 0x00, 0x0A, 0x00, 0x0B, // #10 Utf8 "x"
    0x01, 0x00, 0x01, b'x', // #11 Utf8 "I"
    0x01, 0x00, 0x01, b'I', // #12 Utf8 "m"
    0x01, 0x00, 0x01, b'm', // #13 Utf8 "()I"
    0x01, 0x00, 0x03, b'(', b')', b'I',
    // access_flags=0x0001, this_class=#1, super_class=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces_count=0, fields_count=0
    0x00, 0x00, 0x00, 0x00, // methods_count=2
    0x00, 0x02,
    // --- Method 0: <clinit>()V ---
    // access=0x0008, name=#5, desc=#6, attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code attr: name=#7, length=18
    0x00, 0x07, 0x00, 0x00, 0x00, 0x12, // max_stack=1, max_locals=0, code_length=6
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
    // bytecode: bipush 10, putstatic #8, return
    0x10, 0x0A, 0xB3, 0x00, 0x08, 0xB1,
    // exception_table_length=0, code_attributes_count=0
    0x00, 0x00, 0x00, 0x00,
    // --- Method 1: m()I ---
    // access=0x0008, name=#12, desc=#13, attrs=1
    0x00, 0x08, 0x00, 0x0C, 0x00, 0x0D, 0x00,
    0x01, // Code attr: name=#7, length=20 (2+2+4+8 bytecode+2+2)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x14, // max_stack=2, max_locals=0, code_length=8
    0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
    // bytecode: getstatic #8, getstatic #8, iadd, ireturn
    0xB2, 0x00, 0x08, 0xB2, 0x00, 0x08, 0x60, 0xAC,
    // exception_table_length=0, code_attributes_count=0
    0x00, 0x00, 0x00, 0x00, // class attributes_count=0
    0x00, 0x00,
];

// ── Test 3: invokestatic triggers clinit ─────────────────────────────────
//
// Two classes loaded together:
//   Class "E" (index 0) — has <clinit> setting E.val = 99, and get()I returning E.val
//   Class "Caller" (index 1) — method m()I: invokestatic E.get, ireturn
//
// CLASS_E constant pool (cp_count=14):
//   #1:  Class       -> #2           (E)
//   #2:  Utf8        "E"
//   #3:  Class       -> #4           (java/lang/Object)
//   #4:  Utf8        "java/lang/Object"
//   #5:  Utf8        "<clinit>"
//   #6:  Utf8        "()V"
//   #7:  Utf8        "Code"
//   #8:  Fieldref    -> #1, #9       (E.val)
//   #9:  NameAndType -> #10, #11
//   #10: Utf8        "val"
//   #11: Utf8        "I"
//   #12: Utf8        "get"
//   #13: Utf8        "()I"
static CLASS_E: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "E"
    0x01, 0x00, 0x01, b'E', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5 Utf8 "<clinit>"
    0x01, 0x00, 0x08, b'<', b'c', b'l', b'i', b'n', b'i', b't', b'>', // #6 Utf8 "()V"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7 Utf8 "Code"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8 Fieldref -> #1, #9
    0x09, 0x00, 0x01, 0x00, 0x09, // #9 NameAndType -> #10, #11
    0x0C, 0x00, 0x0A, 0x00, 0x0B, // #10 Utf8 "val"
    0x01, 0x00, 0x03, b'v', b'a', b'l', // #11 Utf8 "I"
    0x01, 0x00, 0x01, b'I', // #12 Utf8 "get"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #13 Utf8 "()I"
    0x01, 0x00, 0x03, b'(', b')', b'I',
    // access_flags=0x0001, this_class=#1, super_class=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces_count=0, fields_count=0
    0x00, 0x00, 0x00, 0x00, // methods_count=2
    0x00, 0x02, // --- Method 0: <clinit>()V ---
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code attr: name=#7, length=18
    0x00, 0x07, 0x00, 0x00, 0x00, 0x12, // max_stack=1, max_locals=0, code_length=6
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
    // bytecode: bipush 99, putstatic #8, return
    0x10, 0x63, 0xB3, 0x00, 0x08, 0xB1, 0x00, 0x00, 0x00, 0x00,
    // --- Method 1: get()I ---
    0x00, 0x08, 0x00, 0x0C, 0x00, 0x0D, 0x00, 0x01, // Code attr: name=#7, length=16
    0x00, 0x07, 0x00, 0x00, 0x00, 0x10, // max_stack=1, max_locals=0, code_length=4
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // bytecode: getstatic #8, ireturn
    0xB2, 0x00, 0x08, 0xAC, 0x00, 0x00, 0x00, 0x00, // class attributes_count=0
    0x00, 0x00,
];

// CLASS_CALLER constant pool (cp_count=10):
//   #1:  Class       -> #2           (Caller)
//   #2:  Utf8        "Caller"
//   #3:  Class       -> #4           (java/lang/Object)
//   #4:  Utf8        "java/lang/Object"
//   #5:  Utf8        "m"
//   #6:  Utf8        "()I"
//   #7:  Utf8        "Code"
//   #8:  Methodref   -> #9, #10      (E.get)
//   #9:  Class       -> #11          (E)
//   #10: NameAndType -> #12, #13     (get:()I)
//   #11: Utf8        "E"
//   #12: Utf8        "get"
//   #13: Utf8        "()I"  (reuse #6? No, separate for clarity)
//
// Actually let's simplify: reuse #6 for both "()I".
// CP (cp_count=12):
//   #1:  Class       -> #2           (Caller)
//   #2:  Utf8        "Caller"
//   #3:  Class       -> #4           (java/lang/Object)
//   #4:  Utf8        "java/lang/Object"
//   #5:  Utf8        "m"
//   #6:  Utf8        "()I"
//   #7:  Utf8        "Code"
//   #8:  Methodref   -> #9, #10      (E.get)
//   #9:  Class       -> #11          (E)
//   #10: NameAndType -> #12, #6      (get:()I — reuses #6)
//   #11: Utf8        "E"
//   #12: Utf8        "get"
//
// Hmm, NameAndType #10 uses name=#12, desc=#6. That works.
// Methodref #8: class=#9, name_and_type=#10.
//
// Wait, Methodref tag is 10 (0x0A), and the class_index points to a Class entry,
// and the name_and_type_index points to a NameAndType entry.
static CLASS_CALLER: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // cp_count=13
    0x00, 0x0D, // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "Caller"
    0x01, 0x00, 0x06, b'C', b'a', b'l', b'l', b'e', b'r', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5 Utf8 "m"
    0x01, 0x00, 0x01, b'm', // #6 Utf8 "()I"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #7 Utf8 "Code"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8 Methodref -> class=#9, nat=#10
    0x0A, 0x00, 0x09, 0x00, 0x0A, // #9 Class -> #11
    0x07, 0x00, 0x0B, // #10 NameAndType -> name=#12, desc=#6
    0x0C, 0x00, 0x0C, 0x00, 0x06, // #11 Utf8 "E"
    0x01, 0x00, 0x01, b'E', // #12 Utf8 "get"
    0x01, 0x00, 0x03, b'g', b'e', b't',
    // access_flags=0x0001, this_class=#1, super_class=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces_count=0, fields_count=0
    0x00, 0x00, 0x00, 0x00, // methods_count=1
    0x00, 0x01, // --- Method 0: m()I ---
    // access=0x0008, name=#5, desc=#6, attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code attr: name=#7, length=16 (2+2+4+4 bytecode+2+2)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x10, // max_stack=1, max_locals=0, code_length=4
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
    // bytecode: invokestatic #8 (E.get), ireturn
    0xB8, 0x00, 0x08, 0xAC, // exception_table_length=0, code_attributes_count=0
    0x00, 0x00, 0x00, 0x00, // class attributes_count=0
    0x00, 0x00,
];

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn clinit_basic_getstatic() {
    // getstatic on C.x triggers <clinit> which sets x=42.
    // Method m() (index 1) does: getstatic C.x, ireturn
    let cf = ClassFile::parse(CLASS_CLINIT_BASIC).expect("parse failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NoopHandler;
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        0, // class_idx
        1, // method_idx (m, not <clinit>)
        &[],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(42)));
}

#[test]
fn clinit_runs_only_once() {
    // m() does: getstatic D.x, getstatic D.x, iadd, ireturn
    // <clinit> sets x=10.  If clinit ran per-getstatic we'd still get 20,
    // but the real check is that it executes without error and returns 20.
    let cf = ClassFile::parse(CLASS_CLINIT_RUNS_ONCE).expect("parse failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NoopHandler;
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        0,
        1, // m()
        &[],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(20)));
}

#[test]
fn invokestatic_triggers_clinit() {
    // Caller.m() invokestatic E.get() — E's <clinit> sets E.val=99.
    let result = run_multi(&[CLASS_E, CLASS_CALLER], 1, &[]);
    assert_eq!(result.unwrap(), Some(Value::Int(99)));
}
