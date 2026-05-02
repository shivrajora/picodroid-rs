// SPDX-License-Identifier: GPL-3.0-only
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
        &mut crate::class_objects::ClassObjectCache::new(),
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
        &mut crate::class_objects::ClassObjectCache::new(),
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
        &mut crate::class_objects::ClassObjectCache::new(),
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
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        2,
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(1)));
}

// ── invokedynamic (lambda) tests ──────────────────────────────────────────

// Class "Target" extends Object, static method lambda$test$0()I → iconst_3, ireturn.
//
// CP (cp_count=8, entries #1..#7):
//   #1: Class -> #2 (Target)
//   #2: Utf8 "Target"
//   #3: Class -> #4 (java/lang/Object)
//   #4: Utf8 "java/lang/Object"
//   #5: Utf8 "lambda$test$0"
//   #6: Utf8 "()I"
//   #7: Utf8 "Code"
static CLASS_TARGET_LAMBDA: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, // cp_count=8
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x06, b'T', b'a', b'r', b'g', b'e', b't', // #2 Utf8 "Target"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x0D, b'l', b'a', b'm', b'b', b'd', b'a', b'$', b't', b'e', b's', b't', b'$',
    b'0', // #5 Utf8 "lambda$test$0"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: access=0x0008 (static), name=#5, desc=#6, attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code attr: name=#7, attr_len=14
    0x00, 0x07, 0x00, 0x00, 0x00, 0x0E, // max_stack=1, max_locals=0, code_len=2
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // iconst_3, ireturn
    0x06, 0xAC, // exc_table=0, code_attrs=0
    0x00, 0x00, 0x00, 0x00, // class attrs=0
    0x00, 0x00,
];

// Class "LambdaCaller" extends Object, static method m()I.
// Bytecode: invokedynamic → create lambda proxy, astore_0, aload_0,
//           invokeinterface Func.call()I, ireturn.
//
// CP (cp_count=26, entries #1..#25):
//   #1:  Class -> #2 (LambdaCaller)
//   #2:  Utf8 "LambdaCaller"
//   #3:  Class -> #4 (java/lang/Object)
//   #4:  Utf8 "java/lang/Object"
//   #5:  Utf8 "m"
//   #6:  Utf8 "()I"
//   #7:  Utf8 "Code"
//   #8:  Utf8 "call"
//   #9:  Utf8 "()LFunc;"
//   #10: Utf8 "Target"
//   #11: Utf8 "lambda$test$0"
//   #12: Utf8 "Func"
//   #13: Utf8 "BootstrapMethods"
//   #14: Class -> #10 (Target)
//   #15: Class -> #12 (Func)
//   #16: NameAndType -> #8, #9 (call:()LFunc;)
//   #17: NameAndType -> #11, #6 (lambda$test$0:()I)
//   #18: NameAndType -> #8, #6 (call:()I)
//   #19: Methodref -> #14, #17 (Target.lambda$test$0:()I)
//   #20: InterfaceMethodref -> #15, #18 (Func.call:()I)
//   #21: MethodHandle ref_kind=6, ref_idx=#19 (impl method)
//   #22: MethodHandle ref_kind=6, ref_idx=#19 (BSM, unused)
//   #23: MethodType -> #6 (samMethodType)
//   #24: MethodType -> #6 (instantiatedMethodType)
//   #25: InvokeDynamic -> bsm_idx=0, nat_idx=#16
//
// BootstrapMethods: 1 entry → method_ref=#22, args=[#23, #21, #24]
static CLASS_CALLER_INVOKEDYNAMIC: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // cp_count = 26
    0x00, 0x1A, // #1: Class -> #2
    0x07, 0x00, 0x02, // #2: Utf8 "LambdaCaller" (12)
    0x01, 0x00, 0x0C, b'L', b'a', b'm', b'b', b'd', b'a', b'C', b'a', b'l', b'l', b'e', b'r',
    // #3: Class -> #4
    0x07, 0x00, 0x04, // #4: Utf8 "java/lang/Object" (16)
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #5: Utf8 "m" (1)
    0x01, 0x00, 0x01, b'm', // #6: Utf8 "()I" (3)
    0x01, 0x00, 0x03, b'(', b')', b'I', // #7: Utf8 "Code" (4)
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #8: Utf8 "call" (4)
    0x01, 0x00, 0x04, b'c', b'a', b'l', b'l', // #9: Utf8 "()LFunc;" (8)
    0x01, 0x00, 0x08, b'(', b')', b'L', b'F', b'u', b'n', b'c', b';',
    // #10: Utf8 "Target" (6)
    0x01, 0x00, 0x06, b'T', b'a', b'r', b'g', b'e', b't',
    // #11: Utf8 "lambda$test$0" (13)
    0x01, 0x00, 0x0D, b'l', b'a', b'm', b'b', b'd', b'a', b'$', b't', b'e', b's', b't', b'$', b'0',
    // #12: Utf8 "Func" (4)
    0x01, 0x00, 0x04, b'F', b'u', b'n', b'c', // #13: Utf8 "BootstrapMethods" (16)
    0x01, 0x00, 0x10, b'B', b'o', b'o', b't', b's', b't', b'r', b'a', b'p', b'M', b'e', b't', b'h',
    b'o', b'd', b's', // #14: Class -> #10 (Target)
    0x07, 0x00, 0x0A, // #15: Class -> #12 (Func)
    0x07, 0x00, 0x0C, // #16: NameAndType -> #8, #9 (call : ()LFunc;)
    0x0C, 0x00, 0x08, 0x00, 0x09, // #17: NameAndType -> #11, #6 (lambda$test$0 : ()I)
    0x0C, 0x00, 0x0B, 0x00, 0x06, // #18: NameAndType -> #8, #6 (call : ()I)
    0x0C, 0x00, 0x08, 0x00, 0x06,
    // #19: Methodref -> #14, #17 (Target.lambda$test$0:()I)
    0x0A, 0x00, 0x0E, 0x00, 0x11, // #20: InterfaceMethodref -> #15, #18 (Func.call:()I)
    0x0B, 0x00, 0x0F, 0x00, 0x12,
    // #21: MethodHandle ref_kind=6 (REF_invokeStatic), ref_idx=#19
    0x0F, 0x06, 0x00, 0x13,
    // #22: MethodHandle ref_kind=6, ref_idx=#19 (BSM handle, unused by our impl)
    0x0F, 0x06, 0x00, 0x13, // #23: MethodType -> #6 (()I)
    0x10, 0x00, 0x06, // #24: MethodType -> #6 (()I)
    0x10, 0x00, 0x06, // #25: InvokeDynamic -> bsm_idx=0, nat_idx=#16
    0x12, 0x00, 0x00, 0x00, 0x10, // access_flags = 0x0001
    0x00, 0x01, // this_class = #1
    0x00, 0x01, // super_class = #3
    0x00, 0x03, // interfaces_count = 0
    0x00, 0x00, // fields_count = 0
    0x00, 0x00, // methods_count = 1
    0x00, 0x01, // method: access=0x0008 (static), name=#5 (m), desc=#6 (()I), attrs=1
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, // Code attr: name=#7, attr_length=25
    0x00, 0x07, 0x00, 0x00, 0x00, 0x19, // max_stack=1, max_locals=1, code_length=13
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0D, // bytecode:
    0xBA, 0x00, 0x19, 0x00, 0x00, // invokedynamic #25, 0, 0
    0x3B, // astore_0
    0x2A, // aload_0
    0xB9, 0x00, 0x14, 0x01, 0x00, // invokeinterface #20, count=1, 0
    0xAC, // ireturn
    // exception_table_length = 0
    0x00, 0x00, // Code inner attributes_count = 0
    0x00, 0x00, // class attributes_count = 1
    0x00, 0x01, // BootstrapMethods attr: name_index=#13, attr_length=12
    0x00, 0x0D, 0x00, 0x00, 0x00, 0x0C, // num_bootstrap_methods = 1
    0x00, 0x01, // BSM entry 0: method_ref=#22, num_args=3, args=[#23, #21, #24]
    0x00, 0x16, 0x00, 0x03, 0x00, 0x17, 0x00, 0x15, 0x00, 0x18,
];

#[test]
fn invokedynamic_creates_lambda_proxy_and_dispatches() {
    // Target has static lambda$test$0()I → returns 3.
    // LambdaCaller.m()I uses invokedynamic to create a Func proxy,
    // calls Func.call()I on it via invokeinterface → should return 3.
    let result = run_multi(
        &[CLASS_TARGET_LAMBDA, CLASS_CALLER_INVOKEDYNAMIC],
        1, // LambdaCaller is at index 1
        &[],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(3)));
}

// ── anonymous class tests ────────────────────────────────────────────────

// Interface "IFace" with abstract method get()I.
//
// CP (cp_count=7, entries #1..#6):
//   #1: Class -> #2   (IFace)
//   #2: Utf8  "IFace"
//   #3: Class -> #4   (java/lang/Object)
//   #4: Utf8  "java/lang/Object"
//   #5: Utf8  "get"
//   #6: Utf8  "()I"
static CLASS_IFACE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x07, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x05, b'I', b'F', b'a', b'c', b'e', // #2 Utf8 "IFace"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #5 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x06, 0x01, // access_flags = ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT
    0x00, 0x01, 0x00, 0x03, // this=#1, super=#3
    0x00, 0x00, // interfaces_count=0
    0x00, 0x00, // fields_count=0
    0x00, 0x01, // methods_count=1
    0x04, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00,
    0x00, // method: abstract public, name=#5, desc=#6, 0 attrs
    0x00, 0x00, // class_attributes_count=0
];

// Class "Outer$1" extends Object, implements IFace, method get()I returns iconst_3.
//
// CP (cp_count=10, entries #1..#9):
//   #1: Class -> #2   (Outer$1)
//   #2: Utf8  "Outer$1"
//   #3: Class -> #4   (java/lang/Object)
//   #4: Utf8  "java/lang/Object"
//   #5: Class -> #6   (IFace)
//   #6: Utf8  "IFace"
//   #7: Utf8  "get"
//   #8: Utf8  "()I"
//   #9: Utf8  "Code"
static CLASS_ANON1: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x07, b'O', b'u', b't', b'e', b'r', b'$', b'1', // #2 Utf8 "Outer$1"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x07, 0x00, 0x06, // #5 Class -> #6
    0x01, 0x00, 0x05, b'I', b'F', b'a', b'c', b'e', // #6 Utf8 "IFace"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #7 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #8 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #9 Utf8 "Code"
    0x00, 0x01, // access_flags = ACC_PUBLIC
    0x00, 0x01, 0x00, 0x03, // this=#1, super=#3
    0x00, 0x01, 0x00, 0x05, // interfaces_count=1, interface=#5
    0x00, 0x00, // fields_count=0
    0x00, 0x01, // methods_count=1
    0x00, 0x01, 0x00, 0x07, 0x00, 0x08, 0x00,
    0x01, // method: public, name=#7, desc=#8, 1 attr
    0x00, 0x09, 0x00, 0x00, 0x00, 0x0E, // Code attr, name=#9, len=14
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, // max_stack=1, max_locals=1, code_len=2
    0x06, 0xAC, // iconst_3, ireturn
    0x00, 0x00, // exception_table_length=0
    0x00, 0x00, // code_attributes_count=0
    0x00, 0x00, // class_attributes_count=0
];

// Class "Outer$2" extends Object, implements IFace, method get()I returns bipush 7.
//
// Same layout as CLASS_ANON1 but name="Outer$2" and returns 7.
static CLASS_ANON2: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x07, b'O', b'u', b't', b'e', b'r', b'$', b'2', // #2 Utf8 "Outer$2"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x07, 0x00, 0x06, // #5 Class -> #6
    0x01, 0x00, 0x05, b'I', b'F', b'a', b'c', b'e', // #6 Utf8 "IFace"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #7 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #8 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #9 Utf8 "Code"
    0x00, 0x01, // access_flags = ACC_PUBLIC
    0x00, 0x01, 0x00, 0x03, // this=#1, super=#3
    0x00, 0x01, 0x00, 0x05, // interfaces_count=1, interface=#5
    0x00, 0x00, // fields_count=0
    0x00, 0x01, // methods_count=1
    0x00, 0x01, 0x00, 0x07, 0x00, 0x08, 0x00,
    0x01, // method: public, name=#7, desc=#8, 1 attr
    0x00, 0x09, 0x00, 0x00, 0x00, 0x0F, // Code attr, name=#9, len=15
    0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, // max_stack=1, max_locals=1, code_len=3
    0x10, 0x07, 0xAC, // bipush 7, ireturn
    0x00, 0x00, // exception_table_length=0
    0x00, 0x00, // code_attributes_count=0
    0x00, 0x00, // class_attributes_count=0
];

// Caller with STATIC m(LIFace;)I that calls get() via invokeinterface.
//
// CP (cp_count=14, entries #1..#13):
//   #1: Class -> #2                  (AnonCaller)
//   #2: Utf8  "AnonCaller"
//   #3: Class -> #4                  (java/lang/Object)
//   #4: Utf8  "java/lang/Object"
//   #5: Utf8  "m"
//   #6: Utf8  "(LIFace;)I"
//   #7: Utf8  "Code"
//   #8: InterfaceMethodref -> #9, #10
//   #9: Class -> #11                 (IFace)
//   #10: NameAndType -> #12, #13
//   #11: Utf8 "IFace"
//   #12: Utf8 "get"
//   #13: Utf8 "()I"
static CLASS_ANON_CALLER_INVOKEINTERFACE: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x0A, b'A', b'n', b'o', b'n', b'C', b'a', b'l', b'l', b'e',
    b'r', // #2 Utf8 "AnonCaller"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x0A, b'(', b'L', b'I', b'F', b'a', b'c', b'e', b';', b')',
    b'I', // #6 Utf8 "(LIFace;)I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x0B, 0x00, 0x09, 0x00, 0x0A, // #8 InterfaceMethodref -> #9, #10
    0x07, 0x00, 0x0B, // #9 Class -> #11
    0x0C, 0x00, 0x0C, 0x00, 0x0D, // #10 NameAndType -> #12, #13
    0x01, 0x00, 0x05, b'I', b'F', b'a', b'c', b'e', // #11 Utf8 "IFace"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #12 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #13 Utf8 "()I"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: static (0x0008), name=#5, desc=#6, 1 attr
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code attr: name=#7, attr_len=19 (code_len=7)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x13, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x07, // max_stack=2, max_locals=1, code_len=7
    // aload_0, invokeinterface #8 count=1 0x00, ireturn
    0x2A, 0xB9, 0x00, 0x08, 0x01, 0x00, 0xAC, 0x00, 0x00, // exception_table_length=0
    0x00, 0x00, // code_attributes_count=0
    0x00, 0x00, // class_attributes_count=0
];

// Caller with STATIC m(LIFace;)I that does instanceof IFace on the argument.
//
// CP (cp_count=10, entries #1..#9):
//   #1: Class -> #2   (InstanceOfCaller)
//   #2: Utf8  "InstanceOfCaller"
//   #3: Class -> #4   (java/lang/Object)
//   #4: Utf8  "java/lang/Object"
//   #5: Utf8  "m"
//   #6: Utf8  "(LIFace;)I"
//   #7: Utf8  "Code"
//   #8: Class -> #9   (IFace)
//   #9: Utf8  "IFace"
static CLASS_INSTANCEOF_CALLER: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0A, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x10, b'I', b'n', b's', b't', b'a', b'n', b'c', b'e', b'O', b'f', b'C', b'a', b'l',
    b'l', b'e', b'r', // #2 Utf8 "InstanceOfCaller"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x0A, b'(', b'L', b'I', b'F', b'a', b'c', b'e', b';', b')',
    b'I', // #6 Utf8 "(LIFace;)I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x07, 0x00, 0x09, // #8 Class -> #9
    0x01, 0x00, 0x05, b'I', b'F', b'a', b'c', b'e', // #9 Utf8 "IFace"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: static (0x0008), name=#5, desc=#6, 1 attr
    0x00, 0x08, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01,
    // Code attr: name=#7, attr_len=17 (code_len=5)
    0x00, 0x07, 0x00, 0x00, 0x00, 0x11, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x05, // max_stack=1, max_locals=1, code_len=5
    // aload_0, instanceof #8, ireturn
    0x2A, 0xC1, 0x00, 0x08, 0xAC, 0x00, 0x00, // exception_table_length=0
    0x00, 0x00, // code_attributes_count=0
    0x00, 0x00, // class_attributes_count=0
];

#[test]
fn invokeinterface_dispatches_on_anonymous_class() {
    // Outer$1 implements IFace.get()I → returns 3.
    // AnonCaller.m(LIFace;)I calls invokeinterface IFace.get() on an Outer$1 object.
    let cf_iface = ClassFile::parse(CLASS_IFACE).expect("parse IFace failed");
    let cf_anon = ClassFile::parse(CLASS_ANON1).expect("parse Outer$1 failed");
    let cf_caller =
        ClassFile::parse(CLASS_ANON_CALLER_INVOKEINTERFACE).expect("parse AnonCaller failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_iface);
    classes.push(cf_anon);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "Outer$1");
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        2, // AnonCaller
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(3)));
}

#[test]
fn instanceof_anonymous_class_against_interface() {
    // Outer$1 implements IFace. instanceof IFace on an Outer$1 object should return 1.
    let cf_iface = ClassFile::parse(CLASS_IFACE).expect("parse IFace failed");
    let cf_anon = ClassFile::parse(CLASS_ANON1).expect("parse Outer$1 failed");
    let cf_caller =
        ClassFile::parse(CLASS_INSTANCEOF_CALLER).expect("parse InstanceOfCaller failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_iface);
    classes.push(cf_anon);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut handler = NoopHandler;
    let obj = alloc_object(&mut objects, "Outer$1");
    let result = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        2, // InstanceOfCaller
        0,
        &[obj],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(1)));
}

#[test]
fn multiple_anonymous_classes_dispatch_independently() {
    // Outer$1.get()I returns 3, Outer$2.get()I returns 7.
    // Invoke both via invokeinterface and verify distinct results.
    let cf_iface = ClassFile::parse(CLASS_IFACE).expect("parse IFace failed");
    let cf_anon1 = ClassFile::parse(CLASS_ANON1).expect("parse Outer$1 failed");
    let cf_anon2 = ClassFile::parse(CLASS_ANON2).expect("parse Outer$2 failed");
    let cf_caller =
        ClassFile::parse(CLASS_ANON_CALLER_INVOKEINTERFACE).expect("parse AnonCaller failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_iface);
    classes.push(cf_anon1);
    classes.push(cf_anon2);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut handler = NoopHandler;

    // Dispatch on Outer$1 → 3
    let obj1 = alloc_object(&mut objects, "Outer$1");
    let result1 = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        3, // AnonCaller
        0,
        &[obj1],
    );
    assert_eq!(result1.unwrap(), Some(Value::Int(3)));

    // Dispatch on Outer$2 → 7
    let obj2 = alloc_object(&mut objects, "Outer$2");
    let result2 = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut GcState::new(),
        &mut crate::class_objects::ClassObjectCache::new(),
        &mut handler,
        3, // AnonCaller
        0,
        &[obj2],
    );
    assert_eq!(result2.unwrap(), Some(Value::Int(7)));
}
