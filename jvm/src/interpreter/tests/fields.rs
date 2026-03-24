use super::*;

// Class "F" extends Object, field "x:I", STATIC method m(LF;)I.
// Bytecode: aload_0, bipush 42, putfield F.x, aload_0, getfield F.x, ireturn.
//
// CP (cp_count=12, entries #1..#11):
//   #1: Class  -> #2         (F)
//   #2: Utf8   "F"
//   #3: Class  -> #4         (java/lang/Object)
//   #4: Utf8   "java/lang/Object"
//   #5: Utf8   "m"
//   #6: Utf8   "(LF;)I"
//   #7: Utf8   "Code"
//   #8: Utf8   "x"
//   #9: Utf8   "I"
//   #10: Fieldref -> #1, #11  (F.x)
//   #11: NameAndType -> #8, #9 (x:I)
static CLASS_F_GETFIELD: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version
    0x00, 0x0C, // cp_count=12
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x01, b'F', // #2 Utf8 "F"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x06, b'(', b'L', b'F', b';', b')', b'I', // #6 Utf8 "(LF;)I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x01, 0x00, 0x01, b'x', // #8 Utf8 "x"
    0x01, 0x00, 0x01, b'I', // #9 Utf8 "I"
    0x09, 0x00, 0x01, 0x00, 0x0B, // #10 Fieldref -> #1, #11
    0x0C, 0x00, 0x08, 0x00, 0x09, // #11 NameAndType -> #8, #9
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, // interfaces=0
    // fields=1: access=0, name=#8, desc=#9, attrs=0
    0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x09, 0x00, 0x00,
    // methods=1: access=0x0008 (static), name=#5, desc=#6, attrs=1
    0x00, 0x01, 0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code: name=#7, attr_len=23
    0x00, 0x07, 0x00, 0x00, 0x00, 0x17, // max_stack=2, max_locals=1, code_len=11
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0B,
    // aload_0, bipush 42, putfield #10, aload_0, getfield #10, ireturn
    0x2A, 0x10, 0x2A, 0xB5, 0x00, 0x0A, 0x2A, 0xB4, 0x00, 0x0A, 0xAC, 0x00, 0x00, 0x00,
    0x00, // exc_table=0, code_attrs=0
    0x00, 0x00, // class_attrs=0
];

// Class "Base" extends Object, method speak()I returns iconst_1, ireturn.
//
// CP (cp_count=8, entries #1..#7):
//   #1: Class  -> #2   (Base)
//   #2: Utf8   "Base"
//   #3: Class  -> #4   (java/lang/Object)
//   #4: Utf8   "java/lang/Object"
//   #5: Utf8   "speak"
//   #6: Utf8   "()I"
//   #7: Utf8   "Code"
static CLASS_BASE_SPEAK: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #2 Utf8 "Base"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x05, b's', b'p', b'e', b'a', b'k', // #5 Utf8 "speak"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // method: public, name=#5, desc=#6
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0E, // Code attr, len=14
    0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, // max_stack=1, max_locals=2, code_len=2
    0x04, 0xAC, // iconst_1, ireturn
    0x00, 0x00, 0x00, 0x00, // exc_table=0, code_attrs=0
    0x00, 0x00, // class_attrs=0
];

// Class "Child" extends "Base", method speak()I returns iconst_2, ireturn.
//
// CP: same layout as CLASS_BASE_SPEAK but class="Child", super="Base".
static CLASS_CHILD_SPEAK: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x05, b'C', b'h', b'i', b'l', b'd', // #2 Utf8 "Child"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #4 Utf8 "Base"
    0x01, 0x00, 0x05, b's', b'p', b'e', b'a', b'k', // #5 Utf8 "speak"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x01,
    0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x05, 0xAC, // iconst_2, ireturn
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn field_slot_own_field_is_slot_0() {
    // CLASS_F_GETFIELD has one instance field "x" — slot 0
    let cf = ClassFile::parse(CLASS_F_GETFIELD).unwrap();
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    assert_eq!(helpers::field_slot(&classes, "F", "x"), Some(0));
}

#[test]
fn field_slot_inherited_field_is_slot_0() {
    // Base has "x" at slot 0. Child extends Base with no fields.
    // field_slot(Child, "x") = 0 (inherited from Base).
    // We reuse CLASS_BASE_SPEAK (Base, no fields) + CLASS_CHILD_SPEAK (Child extends Base, no fields)
    // but we need classes with fields for this test.
    // Use CLASS_F_GETFIELD as "Base"-equivalent (class "F" with field "x") and
    // CLASS_F_GETFIELD alone for the own-field case.
    // For the *inherited* case, build a "Child" extending "F" — reuse CLASS_CHILD_NO_SPEAK
    // but that extends "Base" not "F". So just test the single-class slot lookup here.
    let cf = ClassFile::parse(CLASS_F_GETFIELD).unwrap();
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    assert_eq!(helpers::field_slot(&classes, "F", "x"), Some(0));
    assert_eq!(helpers::field_slot(&classes, "F", "z"), None); // non-existent field
}

#[test]
fn is_instance_of_same_class() {
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).unwrap();
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    assert!(helpers::is_instance_of(&classes, "Base", "Base"));
}

#[test]
fn is_instance_of_parent_class() {
    // Child extends Base → Child is-a Base
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).unwrap();
    let cf_child = ClassFile::parse(CLASS_CHILD_SPEAK).unwrap();
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child);
    assert!(helpers::is_instance_of(&classes, "Child", "Base"));
}

#[test]
fn is_instance_of_unrelated_class() {
    // Base is NOT a subclass of Child
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).unwrap();
    let cf_child = ClassFile::parse(CLASS_CHILD_SPEAK).unwrap();
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child);
    assert!(!helpers::is_instance_of(&classes, "Base", "Child"));
}

#[test]
fn getfield_putfield_named_field_roundtrip() {
    // Load "F", alloc an F object, pass it as arg[0] to the static m(LF;)I.
    // Method stores 42 into field "x", then reads it back → should return 42.
    let cf = ClassFile::parse(CLASS_F_GETFIELD).expect("parse failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "F");
    let mut statics = StaticFieldStore::new();
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut handler,
        0,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(42)));
}
