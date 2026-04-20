use super::*;

// Minimal valid .class file:
//   class "TC" extends java/lang/Object
//   one method: public void run() { return; }
//
// Constant pool (8 entries, cp_count=8 means indices 1..7):
//   #1: Class        -> #2
//   #2: Utf8         "TC"
//   #3: Class        -> #4
//   #4: Utf8         "java/lang/Object"
//   #5: Utf8         "run"
//   #6: Utf8         "()V"
//   #7: Utf8         "Code"
static MINIMAL_CLASS: &[u8] = &[
    // Magic
    0xCA, 0xFE, 0xBA, 0xBE, // Minor version = 0, Major version = 52 (Java 8)
    0x00, 0x00, 0x00, 0x34, // cp_count = 8  (valid entries are indices 1..7)
    0x00, 0x08, // #1: Class -> #2
    0x07, 0x00, 0x02, // #2: Utf8 "TC"  (length=2)
    0x01, 0x00, 0x02, b'T', b'C', // #3: Class -> #4
    0x07, 0x00, 0x04, // #4: Utf8 "java/lang/Object"  (length=16)
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5: Utf8 "run"  (length=3)
    0x01, 0x00, 0x03, b'r', b'u', b'n', // #6: Utf8 "()V"  (length=3)
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7: Utf8 "Code"  (length=4)
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // access_flags = ACC_PUBLIC (0x0001)
    0x00, 0x01, // this_class = #1
    0x00, 0x01, // super_class = #3
    0x00, 0x03, // interfaces_count = 0
    0x00, 0x00, // fields_count = 0
    0x00, 0x00, // methods_count = 1
    0x00, 0x01,
    // Method: access_flags=0x0001, name_index=#5 ("run"), descriptor_index=#6 ("()V")
    0x00, 0x01, // access_flags
    0x00, 0x05, // name_index = #5
    0x00, 0x06, // descriptor_index = #6
    0x00, 0x01, // attributes_count = 1
    // Code attribute: attr_name_index=#7 ("Code")
    0x00, 0x07, // attr_name_index = #7
    // attr_length = 2(max_stack) + 2(max_locals) + 4(code_length) + 1(bytecode) + 2(exception_table_len) + 2(inner_attributes_count)
    //             = 13
    0x00, 0x00, 0x00, 0x0D, // attr_length = 13
    0x00, 0x01, // max_stack = 1
    0x00, 0x01, // max_locals = 1
    0x00, 0x00, 0x00, 0x01, // code_length = 1
    0xB1, // bytecode: return
    0x00, 0x00, // exception_table_length = 0
    0x00, 0x00, // Code inner attributes_count = 0
    // class attributes_count = 0
    0x00, 0x00,
];

// Class "Child" extends "Base" (non-Object super), no fields, method m()V.
//
// Constant pool (cp_count=8, entries #1..#7):
//   #1: Class  -> #2
//   #2: Utf8   "Child"
//   #3: Class  -> #4
//   #4: Utf8   "Base"
//   #5: Utf8   "m"
//   #6: Utf8   "()V"
//   #7: Utf8   "Code"
static CLASS_NONOBJECT_SUPER: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version
    0x00, 0x08, // cp_count=8
    // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "Child" (5)
    0x01, 0x00, 0x05, b'C', b'h', b'i', b'l', b'd', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "Base" (4)
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #5 Utf8 "m" (1)
    0x01, 0x00, 0x01, b'm', // #6 Utf8 "()V" (3)
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7 Utf8 "Code" (4)
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // access=1, this=#1, super=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces=0, fields=0, methods=1
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // method: access=1, name=#5, desc=#6, attrs=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code: name=#7, attr_len=13
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, // max_stack=1, max_locals=1, code_len=1
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // bytecode: return
    0xB1, // exc_table=0, code_attrs=0
    0x00, 0x00, 0x00, 0x00, // class attrs=0
    0x00, 0x00,
];

// Class "F" extends java/lang/Object, one instance field "x:I", method m()V.
//
// Constant pool (cp_count=10, entries #1..#9):
//   #1: Class  -> #2
//   #2: Utf8   "F"
//   #3: Class  -> #4
//   #4: Utf8   "java/lang/Object"
//   #5: Utf8   "m"
//   #6: Utf8   "()V"
//   #7: Utf8   "Code"
//   #8: Utf8   "x"
//   #9: Utf8   "I"
static CLASS_WITH_FIELD: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version
    0x00, 0x0A, // cp_count=10
    // #1 Class -> #2
    0x07, 0x00, 0x02, // #2 Utf8 "F" (1)
    0x01, 0x00, 0x01, b'F', // #3 Class -> #4
    0x07, 0x00, 0x04, // #4 Utf8 "java/lang/Object" (16)
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5 Utf8 "m" (1)
    0x01, 0x00, 0x01, b'm', // #6 Utf8 "()V" (3)
    0x01, 0x00, 0x03, b'(', b')', b'V', // #7 Utf8 "Code" (4)
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8 Utf8 "x" (1)
    0x01, 0x00, 0x01, b'x', // #9 Utf8 "I" (1)
    0x01, 0x00, 0x01, b'I', // access=1, this=#1, super=#3
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // interfaces=0
    0x00, 0x00, // fields=1: access=0, name=#8, desc=#9, attrs=0
    0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x09, 0x00, 0x00,
    // methods=1: access=1, name=#5, desc=#6, attrs=1
    0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code: name=#7, attr_len=13
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, // max_stack=1, max_locals=1, code_len=1
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // bytecode: return
    0xB1, // exc_table=0, code_attrs=0
    0x00, 0x00, 0x00, 0x00, // class_attrs=0
    0x00, 0x00,
];

// Wrong magic bytes — should trigger "bad magic" error.
static BAD_MAGIC: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x34];

// Only the magic — version/cp bytes are missing, should trigger "truncated".
static TRUNCATED: &[u8] = &[0xCA, 0xFE, 0xBA, 0xBE];

#[test]
fn parse_minimal_class_succeeds() {
    let result = ClassFile::parse(MINIMAL_CLASS);
    assert!(result.is_ok(), "expected Ok but got {:?}", result.err());
}

#[test]
fn class_name_is_tc() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    // class_name_index resolves to CP #2, the Utf8 entry for "TC"
    assert_eq!(cf.class_name(), Some(b"TC" as &[u8]));
}

#[test]
fn one_method_parsed() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    assert_eq!(cf.methods().len(), 1);
}

#[test]
fn method_name_is_run() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    // methods[0].name_index = #5 ("run")
    assert_eq!(
        cf.cp_utf8(cf.methods()[0].name_index),
        Some(b"run" as &[u8])
    );
}

#[test]
fn method_code_is_return() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    // The only bytecode instruction is 0xB1 (return)
    assert_eq!(cf.method_code(&cf.methods()[0]), &[0xB1u8]);
}

#[test]
fn bad_magic_returns_error() {
    let result = ClassFile::parse(BAD_MAGIC);
    assert_eq!(result.unwrap_err(), "bad magic");
}

#[test]
fn truncated_returns_error() {
    let result = ClassFile::parse(TRUNCATED);
    assert!(result.is_err(), "expected Err for truncated input");
}

#[test]
fn cp_utf8_wrong_tag_returns_none() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    // CP #1 is a Class entry (tag=7), not a Utf8 entry — must return None
    assert_eq!(cf.cp_utf8(1), None);
}

#[test]
fn super_class_name_is_none_for_object_parent() {
    // MINIMAL_CLASS extends java/lang/Object — should return None
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    assert_eq!(cf.super_class_name(), None);
}

#[test]
fn super_class_name_returns_bytes_for_nonobject_parent() {
    let cf = ClassFile::parse(CLASS_NONOBJECT_SUPER).unwrap();
    assert_eq!(cf.super_class_name(), Some(b"Base" as &[u8]));
}

#[test]
fn class_name_is_child_for_nonobject_super() {
    let cf = ClassFile::parse(CLASS_NONOBJECT_SUPER).unwrap();
    assert_eq!(cf.class_name(), Some(b"Child" as &[u8]));
}

#[test]
fn field_count_one_for_class_with_field() {
    let cf = ClassFile::parse(CLASS_WITH_FIELD).unwrap();
    assert_eq!(cf.fields().len(), 1);
}

#[test]
fn field_name_is_x_for_class_with_field() {
    let cf = ClassFile::parse(CLASS_WITH_FIELD).unwrap();
    assert_eq!(cf.field_name(0), Some(b"x" as &[u8]));
}

#[test]
fn field_count_zero_for_minimal_class() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    assert_eq!(cf.fields().len(), 0);
}

// MINIMAL_CLASS with access_flags=0x0200 (ACC_INTERFACE).
// Identical to MINIMAL_CLASS but bytes [59..61] = 0x02, 0x00.
static CLASS_INTERFACE_FLAG: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x02,
    b'T', b'C', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n',
    b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x03, b'r', b'u', b'n', 0x01, 0x00,
    0x03, b'(', b')', b'V', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e',
    // access_flags = ACC_INTERFACE (0x0200)
    0x02, 0x00, 0x00, 0x01, // this_class = #1
    0x00, 0x03, // super_class = #3
    0x00, 0x00, // interfaces_count = 0
    0x00, 0x00, // fields_count = 0
    0x00, 0x01, // methods_count = 1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0xB1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// MINIMAL_CLASS with access_flags=0x0400 (ACC_ABSTRACT).
static CLASS_ABSTRACT_FLAG: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00, 0x02,
    b'T', b'C', 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n',
    b'g', b'/', b'O', b'b', b'j', b'e', b'c', b't', 0x01, 0x00, 0x03, b'r', b'u', b'n', 0x01, 0x00,
    0x03, b'(', b')', b'V', 0x01, 0x00, 0x04, b'C', b'o', b'd', b'e',
    // access_flags = ACC_ABSTRACT (0x0400)
    0x04, 0x00, 0x00, 0x01, // this_class = #1
    0x00, 0x03, // super_class = #3
    0x00, 0x00, // interfaces_count = 0
    0x00, 0x00, // fields_count = 0
    0x00, 0x01, // methods_count = 1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0xB1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// Class "TC" implementing interface "Runnable".
// CP (cp_count=10, entries #1..#9):
//   #1: Class -> #2  (TC)
//   #2: Utf8 "TC"
//   #3: Class -> #4  (java/lang/Object)
//   #4: Utf8 "java/lang/Object"
//   #5: Utf8 "run"
//   #6: Utf8 "()V"
//   #7: Utf8 "Code"
//   #8: Class -> #9  (Runnable)
//   #9: Utf8 "Runnable"
// interfaces_count=1, interface[0]=#8
static CLASS_WITH_IFACE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, // cp_count=10
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x02, b'T', b'C', // #2 Utf8 "TC"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x03, b'r', b'u', b'n', // #5 Utf8 "run"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #6 Utf8 "()V"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x07, 0x00, 0x09, // #8 Class -> #9
    0x01, 0x00, 0x08, b'R', b'u', b'n', b'n', b'a', b'b', b'l', b'e', // #9 Utf8 "Runnable"
    0x00, 0x01, // access_flags = 0x0001
    0x00, 0x01, // this_class = #1
    0x00, 0x03, // super_class = #3
    0x00, 0x01, // interfaces_count = 1
    0x00, 0x08, // interface[0] = #8
    0x00, 0x00, // fields_count = 0
    0x00, 0x01, // methods_count = 1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // method: public, name=#5, desc=#6
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, // Code attr: name=#7, attr_len=13
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // max_stack=1, max_locals=1, code_len=1
    0xB1, // return
    0x00, 0x00, // exc_table_len=0
    0x00, 0x00, // code_attrs=0
    0x00, 0x00, // class_attrs_count=0
];

#[test]
fn is_interface_returns_true_for_acc_interface() {
    let cf = ClassFile::parse(CLASS_INTERFACE_FLAG).unwrap();
    assert!(cf.is_interface());
    assert!(!cf.is_abstract());
}

#[test]
fn is_abstract_returns_true_for_acc_abstract() {
    let cf = ClassFile::parse(CLASS_ABSTRACT_FLAG).unwrap();
    assert!(cf.is_abstract());
    assert!(!cf.is_interface());
}

#[test]
fn normal_class_is_neither_interface_nor_abstract() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    assert!(!cf.is_interface());
    assert!(!cf.is_abstract());
}

#[test]
fn interface_name_resolves_for_class_with_one_interface() {
    let cf = ClassFile::parse(CLASS_WITH_IFACE).unwrap();
    assert_eq!(cf.interfaces().len(), 1);
    assert_eq!(cf.interface_name(0), Some(b"Runnable" as &[u8]));
    assert_eq!(cf.interface_name(1), None);
}

#[test]
fn no_interfaces_for_minimal_class() {
    let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
    assert_eq!(cf.interfaces().len(), 0);
}

// ── Lazy-load regression guard ────────────────────────────────────────────────
//
// `ClassFile::register` must stay lazy: only constant-pool + class-name scan
// is allowed at registration. A full parse must not fire until an accessor
// that actually needs parsed state is called. The project's memory-footprint
// claims (~40 KB → ~3–4 KB baseline) rely on this.

#[test]
fn register_does_not_trigger_full_parse() {
    let cf = ClassFile::register(MINIMAL_CLASS).unwrap();
    assert!(
        !cf.is_parsed(),
        "ClassFile::register must stay lazy — a full parse here defeats the \
         startup-RAM win documented in the lazy-load milestone"
    );
    // class_name() reads the eagerly-scanned name slice — still lazy.
    assert_eq!(cf.class_name(), Some(b"TC" as &[u8]));
    assert!(
        !cf.is_parsed(),
        "class_name() must not trigger a full parse"
    );
}

#[test]
fn accessor_access_triggers_parse() {
    let cf = ClassFile::register(MINIMAL_CLASS).unwrap();
    assert!(!cf.is_parsed());
    let _ = cf.methods();
    assert!(
        cf.is_parsed(),
        "accessors beyond class_name() are expected to force a full parse"
    );
}

#[test]
fn untouched_registered_classes_stay_unparsed() {
    let touched = ClassFile::register(MINIMAL_CLASS).unwrap();
    let untouched = ClassFile::register(CLASS_NONOBJECT_SUPER).unwrap();

    // Simulate a run that references only `touched`. `class_name()` on both
    // must stay lazy (it's the most common lookup path).
    assert_eq!(touched.class_name(), Some(b"TC" as &[u8]));
    assert_eq!(untouched.class_name(), Some(b"Child" as &[u8]));
    let _ = touched.methods();

    assert!(touched.is_parsed());
    assert!(
        !untouched.is_parsed(),
        "untouched classes must stay unparsed — this is the whole point of \
         the lazy-load architecture"
    );
}
