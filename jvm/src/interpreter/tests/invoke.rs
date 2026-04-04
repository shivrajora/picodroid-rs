use super::*;
use crate::gc::GcState;

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

// Class "ChildNS" extends "Base", no speak() method (only m()V returning void).
// Used to test that invokevirtual walks up to Base.speak() when ChildNS has none.
static CLASS_CHILD_NO_SPEAK: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x07, b'C', b'h', b'i', b'l', b'd', b'N', b'S', // #2 Utf8 "ChildNS"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #4 Utf8 "Base"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x03, b'(', b')', b'V', // #6 Utf8 "()V"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05,
    0x00, 0x06, 0x00, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x01, 0xB1, // return (void)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// Class "Caller" extends Object, STATIC method m(LBase;)I.
// Bytecode: aload_0, invokevirtual Base.speak()I, ireturn.
//
// CP (cp_count=14, entries #1..#13):
//   #1: Class      -> #2        (Caller)
//   #2: Utf8       "Caller"
//   #3: Class      -> #4        (java/lang/Object)
//   #4: Utf8       "java/lang/Object"
//   #5: Utf8       "m"
//   #6: Utf8       "(LBase;)I"
//   #7: Utf8       "Code"
//   #8: Methodref  -> #9, #10   (Base.speak()I)
//   #9: Class      -> #11       (Base)
//   #10: NameAndType -> #12, #13 (speak : ()I)
//   #11: Utf8      "Base"
//   #12: Utf8      "speak"
//   #13: Utf8      "()I"
static CLASS_CALLER_INVOKEVIRTUAL: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, // cp_count=14
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x06, b'C', b'a', b'l', b'l', b'e', b'r', // #2 Utf8 "Caller"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x09, b'(', b'L', b'B', b'a', b's', b'e', b';', b')',
    b'I', // #6 Utf8 "(LBase;)I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x0A, 0x00, 0x09, 0x00, 0x0A, // #8 Methodref -> #9, #10
    0x07, 0x00, 0x0B, // #9 Class -> #11
    0x0C, 0x00, 0x0C, 0x00, 0x0D, // #10 NameAndType -> #12, #13
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #11 Utf8 "Base"
    0x01, 0x00, 0x05, b's', b'p', b'e', b'a', b'k', // #12 Utf8 "speak"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #13 Utf8 "()I"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: access=0x0008 (static), name=#5, desc=#6, attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code: name=#7, attr_len=17
    0x00, 0x07, 0x00, 0x00, 0x00, 0x11, // max_stack=2, max_locals=1, code_len=5
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, // aload_0, invokevirtual #8, ireturn
    0x2A, 0xB6, 0x00, 0x08, 0xAC, 0x00, 0x00, 0x00, 0x00, // exc_table=0, code_attrs=0
    0x00, 0x00, // class_attrs=0
];

// ── invokeinterface tests ─────────────────────────────────────────────────

// Class "Caller" with STATIC m(LBase;)I that calls speak() via invokeinterface.
//
// Identical to CLASS_CALLER_INVOKEVIRTUAL except:
//   - CP entry #8 tag: 0x0B (InterfaceMethodref) instead of 0x0A (Methodref)
//   - bytecode: invokeinterface (0xB9) with count=1 and 0x00 padding bytes
//   - code_len: 7 (was 5)  →  attr_len: 19 (was 17)
//
// CP (cp_count=14, entries #1..#13):
//   #1: Class -> #2 (Caller)           #8: InterfaceMethodref -> #9, #10
//   #2: Utf8 "Caller"                  #9: Class -> #11
//   #3: Class -> #4                    #10: NameAndType -> #12, #13
//   #4: Utf8 "java/lang/Object"        #11: Utf8 "Base"
//   #5: Utf8 "m"                       #12: Utf8 "speak"
//   #6: Utf8 "(LBase;)I"              #13: Utf8 "()I"
//   #7: Utf8 "Code"
static CLASS_CALLER_INVOKEINTERFACE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, // cp_count=14
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x06, b'C', b'a', b'l', b'l', b'e', b'r', // #2 Utf8 "Caller"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x09, b'(', b'L', b'B', b'a', b's', b'e', b';', b')', b'I', // #6 "(LBase;)I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x0B, 0x00, 0x09, 0x00, 0x0A, // #8 InterfaceMethodref -> #9, #10
    0x07, 0x00, 0x0B, // #9 Class -> #11
    0x0C, 0x00, 0x0C, 0x00, 0x0D, // #10 NameAndType -> #12, #13
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #11 Utf8 "Base"
    0x01, 0x00, 0x05, b's', b'p', b'e', b'a', b'k', // #12 Utf8 "speak"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #13 Utf8 "()I"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: static (0x0008), name=#5, desc=#6, 1 Code attr
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code attr: name=#7, attr_len=19 (code_len=7)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x13, // max_stack=2, max_locals=1, code_len=7
    0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07,
    // aload_0, invokeinterface #8 count=1 0x00, ireturn
    0x2A, 0xB9, 0x00, 0x08, 0x01, 0x00, 0xAC, 0x00, 0x00, // exc_table_len=0
    0x00, 0x00, // code_attrs=0
    0x00, 0x00, // class_attrs=0
];

#[test]
fn invokevirtual_uses_override_in_subclass() {
    // Child overrides speak() → returns 2.
    // Caller.m(LBase;)I does invokevirtual Base.speak()I on a Child object.
    // Expected: Child.speak() is dispatched → Value::Int(2).
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).expect("parse BASE failed");
    let cf_child = ClassFile::parse(CLASS_CHILD_SPEAK).expect("parse CHILD failed");
    let cf_caller = ClassFile::parse(CLASS_CALLER_INVOKEVIRTUAL).expect("parse CALLER failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "Child");
    // Run Caller.m (class index 2, method index 0)
    let mut statics = StaticFieldStore::new();
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut handler,
        2,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(2)));
}

#[test]
fn invokevirtual_walks_up_to_base_when_subclass_has_no_override() {
    // ChildNS extends Base but has no speak() → invokevirtual must walk to Base.speak() → returns 1.
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).expect("parse BASE failed");
    let cf_child_ns = ClassFile::parse(CLASS_CHILD_NO_SPEAK).expect("parse CHILDNS failed");
    let cf_caller = ClassFile::parse(CLASS_CALLER_INVOKEVIRTUAL).expect("parse CALLER failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child_ns);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "ChildNS");
    // Run Caller.m (class index 2, method index 0)
    let mut statics = StaticFieldStore::new();
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut handler,
        2,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(1)));
}

#[test]
fn invokeinterface_dispatches_to_runtime_class_override() {
    // Caller.m(LBase;)I calls speak() via invokeinterface on a Child object.
    // Child overrides speak() to return 2 → should return Int(2).
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).expect("parse BASE failed");
    let cf_child = ClassFile::parse(CLASS_CHILD_SPEAK).expect("parse CHILD failed");
    let cf_caller = ClassFile::parse(CLASS_CALLER_INVOKEINTERFACE).expect("parse CALLER failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "Child");
    // Run Caller.m (class index 2, method index 0)
    let mut statics = StaticFieldStore::new();
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut handler,
        2,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(2)));
}

#[test]
fn invokeinterface_walks_up_to_base_when_subclass_has_no_override() {
    // ChildNS has no speak() → invokeinterface must walk to Base.speak() → returns 1.
    let cf_base = ClassFile::parse(CLASS_BASE_SPEAK).expect("parse BASE failed");
    let cf_child_ns = ClassFile::parse(CLASS_CHILD_NO_SPEAK).expect("parse CHILDNS failed");
    let cf_caller = ClassFile::parse(CLASS_CALLER_INVOKEINTERFACE).expect("parse CALLER failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child_ns);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "ChildNS");
    let mut statics = StaticFieldStore::new();
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut handler,
        2,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(1)));
}
