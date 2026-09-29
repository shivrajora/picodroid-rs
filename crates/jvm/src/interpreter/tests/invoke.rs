// SPDX-License-Identifier: GPL-3.0-only
use super::asm::{Asm, Method};
use super::*;
use crate::class_file::Classes;
use crate::gc::GcState;
use crate::names::spelled;
use crate::names::{c, m};
use crate::resolve_cache::{flags, ResolveCache, SiteKey, Target, RECV_STRING};

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
pub(super) static CLASS_BASE_SPEAK: &[u8] = &[
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
pub(super) static CLASS_CHILD_SPEAK: &[u8] = &[
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
pub(super) static CLASS_CALLER_INVOKEVIRTUAL: &[u8] = &[
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

// ── invokestatic-walks-superclass tests ───────────────────────────────────

// Class "Base" extends Object, STATIC method get()I returns iconst_3, ireturn.
// CP layout matches CLASS_BASE_SPEAK above (cp_count=8), differs only in the
// method name ("get") and access flag (0x0009 = public|static, max_locals=0).
static CLASS_BASE_GET_STATIC: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00,
    0x02, // #1 Class -> #2
    0x01, 0x00, 0x04, b'B', b'a', b's', b'e', // #2 Utf8 "Base"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #5 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: access=0x0009 (public|static), name=#5, desc=#6, attrs=1
    0x00, 0x09, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, // Code attr name=#7
    0x00, 0x00, 0x00, 0x0E, // attr_len=14
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, // max_stack=1, max_locals=0, code_len=2
    0x06, 0xAC, // iconst_3, ireturn
    0x00, 0x00, 0x00, 0x00, // exc_table=0, code_attrs=0
    0x00, 0x00, // class_attrs=0
];

// Class "Caller" with STATIC method m()I that does invokestatic ChildNS.get()I.
// CP layout matches CLASS_CALLER_INVOKEVIRTUAL except:
//   - desc "()I" (no LBase; receiver),
//   - CP #11 names "ChildNS" (so the invokestatic CP entry refers to the subclass
//     which has no get() — the walk must reach Base.get() on the superclass),
//   - CP #12 is "get",
//   - bytecode is invokestatic #8 + ireturn (no aload_0),
//   - method m takes no args (max_locals=0).
static CLASS_CALLER_INVOKESTATIC_INHERITED: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0E, // cp_count=14
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x06, b'C', b'a', b'l', b'l', b'e', b'r', // #2 Utf8 "Caller"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #6 Utf8 "()I"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x0A, 0x00, 0x09, 0x00, 0x0A, // #8 Methodref -> #9, #10
    0x07, 0x00, 0x0B, // #9 Class -> #11
    0x0C, 0x00, 0x0C, 0x00, 0x0D, // #10 NameAndType -> #12, #13
    0x01, 0x00, 0x07, b'C', b'h', b'i', b'l', b'd', b'N', b'S', // #11 Utf8 "ChildNS"
    0x01, 0x00, 0x03, b'g', b'e', b't', // #12 Utf8 "get"
    0x01, 0x00, 0x03, b'(', b')', b'I', // #13 Utf8 "()I"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    // method: access=0x0009 (public|static), name=#5, desc=#6, attrs=1
    0x00, 0x09, 0x00, 0x05, 0x00, 0x06, 0x00, 0x01, 0x00, 0x07, // Code attr name=#7
    0x00, 0x00, 0x00, 0x10, // attr_len=16
    0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // max_stack=1, max_locals=0, code_len=4
    0xB8, 0x00, 0x08, 0xAC, // invokestatic #8, ireturn
    0x00, 0x00, 0x00, 0x00, // exc_table=0, code_attrs=0
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
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).expect("parse BASE failed");
    let cf_child = ClassFile::parse(spelled(CLASS_CHILD_SPEAK)).expect("parse CHILD failed");
    let cf_caller =
        ClassFile::parse(spelled(CLASS_CALLER_INVOKEVIRTUAL)).expect("parse CALLER failed");
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
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).expect("parse BASE failed");
    let cf_child_ns =
        ClassFile::parse(spelled(CLASS_CHILD_NO_SPEAK)).expect("parse CHILDNS failed");
    let cf_caller =
        ClassFile::parse(spelled(CLASS_CALLER_INVOKEVIRTUAL)).expect("parse CALLER failed");
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
fn invokestatic_walks_up_to_base_when_subclass_has_no_inherited_static() {
    // JVMS §5.4.3.3 method resolution: `invokestatic Subclass.parentStatic()` must walk
    // up to the superclass when the subclass doesn't declare the method.
    // Base declares `public static int get() { return 3; }`. ChildNS extends Base, has no get().
    // Caller.m() does `invokestatic ChildNS.get()I` and is expected to return 3.
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_GET_STATIC)).expect("parse BASE failed");
    let cf_child_ns =
        ClassFile::parse(spelled(CLASS_CHILD_NO_SPEAK)).expect("parse CHILDNS failed");
    let cf_caller = ClassFile::parse(spelled(CLASS_CALLER_INVOKESTATIC_INHERITED))
        .expect("parse CALLER failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf_base);
    classes.push(cf_child_ns);
    classes.push(cf_caller);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut handler = NoopHandler;
    let mut statics = StaticFieldStore::new();
    // Run Caller.m (class index 2, method index 0). Static, no args.
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
        &[],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(3)));
}

#[test]
fn invokeinterface_dispatches_to_runtime_class_override() {
    // Caller.m(LBase;)I calls speak() via invokeinterface on a Child object.
    // Child overrides speak() to return 2 → should return Int(2).
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).expect("parse BASE failed");
    let cf_child = ClassFile::parse(spelled(CLASS_CHILD_SPEAK)).expect("parse CHILD failed");
    let cf_caller =
        ClassFile::parse(spelled(CLASS_CALLER_INVOKEINTERFACE)).expect("parse CALLER failed");
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
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).expect("parse BASE failed");
    let cf_child_ns =
        ClassFile::parse(spelled(CLASS_CHILD_NO_SPEAK)).expect("parse CHILDNS failed");
    let cf_caller =
        ClassFile::parse(spelled(CLASS_CALLER_INVOKEINTERFACE)).expect("parse CALLER failed");
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

/// `CLASS_CALLER_INVOKEDYNAMIC` with a real bootstrap: appends
/// `#26 Utf8 "java/lang/invoke/LambdaMetafactory"`, `#27 Utf8 "metafactory"`,
/// `#28 Class→#26`, `#29 NameAndType→#27,#6`, `#30 Methodref→#28,#29`,
/// `#31 MethodHandle(6→#30)` to the constant pool, points the
/// BootstrapMethods entry at `bsm_ref` (#31 for the real thing; #22 is the
/// original bogus `Target.lambda$test$0` handle) and sets the implementation
/// handle's reference kind (#21) to `impl_kind`.
fn indy_caller(bsm_ref: u16, impl_kind: u8) -> &'static [u8] {
    let base = CLASS_CALLER_INVOKEDYNAMIC;
    let cp_end = base
        .windows(5)
        .position(|w| w == [0x12, 0x00, 0x00, 0x00, 0x10])
        .expect("InvokeDynamic entry")
        + 5;
    let mut out: Vec<u8> = Vec::with_capacity(base.len() + 80);
    out.extend_from_slice(&base[..8]);
    out.extend_from_slice(&32u16.to_be_bytes()); // cp_count: 31 entries
    out.extend_from_slice(&base[10..cp_end]);
    for s in [
        &c::java_lang_invoke_LambdaMetafactory.as_bytes()[..],
        b"metafactory",
    ] {
        out.push(0x01);
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    out.extend_from_slice(&[0x07, 0x00, 26]); // #28 Class → #26
    out.extend_from_slice(&[0x0C, 0x00, 27, 0x00, 0x06]); // #29 NameAndType
    out.extend_from_slice(&[0x0A, 0x00, 28, 0x00, 29]); // #30 Methodref
    out.extend_from_slice(&[0x0F, 0x06, 0x00, 30]); // #31 MethodHandle
    out.extend_from_slice(&base[cp_end..]);
    // Implementation handle #21 is the first `0F 06 00 13` in the pool.
    let impl_at = out
        .windows(4)
        .position(|w| w == [0x0F, 0x06, 0x00, 0x13])
        .expect("impl handle");
    out[impl_at + 1] = impl_kind;
    // BootstrapMethods entry: method_ref is 10 bytes from the end.
    let n = out.len();
    out[n - 10..n - 8].copy_from_slice(&bsm_ref.to_be_bytes());
    alloc::boxed::Box::leak(out.into_boxed_slice())
}

#[test]
fn invokedynamic_creates_lambda_proxy_and_dispatches() {
    // Target has static lambda$test$0()I → returns 3.
    // LambdaCaller.m()I uses invokedynamic to create a Func proxy,
    // calls Func.call()I on it via invokeinterface → should return 3.
    let result = run_multi(
        &[CLASS_TARGET_LAMBDA, indy_caller(31, 6)],
        1, // LambdaCaller is at index 1
        &[],
    );
    assert_eq!(result.unwrap(), Some(Value::Int(3)));
}

/// A bootstrap that is not `LambdaMetafactory.metafactory` (here the bogus
/// `Target.lambda$test$0` handle, standing in for `StringConcatFactory` /
/// `ObjectMethods`) must fail by name instead of having `arguments[1]`
/// misread as an implementation handle.
#[test]
fn invokedynamic_rejects_non_lambda_bootstrap() {
    let result = run_multi(&[CLASS_TARGET_LAMBDA, indy_caller(22, 6)], 1, &[]);
    assert_eq!(result, Err(JvmError::UnsupportedInvokeDynamic("Target")));
}

/// `REF_newInvokeSpecial` is a constructor reference (`Foo::new`, served
/// since QA 2026-09-13 — see `tests::lambdas`); one that names anything
/// but `<init>` (here the static `lambda$test$0`) is a malformed class file
/// and is rejected up front rather than run as a constructor.
#[test]
fn invokedynamic_rejects_constructor_reference_to_a_non_constructor() {
    let result = run_multi(&[CLASS_TARGET_LAMBDA, indy_caller(31, 8)], 1, &[]);
    assert!(
        matches!(result, Err(JvmError::UnsupportedInvokeDynamic(_))),
        "{result:?}"
    );
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
    let cf_iface = ClassFile::parse(spelled(CLASS_IFACE)).expect("parse IFace failed");
    let cf_anon = ClassFile::parse(spelled(CLASS_ANON1)).expect("parse Outer$1 failed");
    let cf_caller = ClassFile::parse(spelled(CLASS_ANON_CALLER_INVOKEINTERFACE))
        .expect("parse AnonCaller failed");
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
    let cf_iface = ClassFile::parse(spelled(CLASS_IFACE)).expect("parse IFace failed");
    let cf_anon = ClassFile::parse(spelled(CLASS_ANON1)).expect("parse Outer$1 failed");
    let cf_caller =
        ClassFile::parse(spelled(CLASS_INSTANCEOF_CALLER)).expect("parse InstanceOfCaller failed");
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
    let cf_iface = ClassFile::parse(spelled(CLASS_IFACE)).expect("parse IFace failed");
    let cf_anon1 = ClassFile::parse(spelled(CLASS_ANON1)).expect("parse Outer$1 failed");
    let cf_anon2 = ClassFile::parse(spelled(CLASS_ANON2)).expect("parse Outer$2 failed");
    let cf_caller = ClassFile::parse(spelled(CLASS_ANON_CALLER_INVOKEINTERFACE))
        .expect("parse AnonCaller failed");
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

/// QA 2026-09-13: `arr.getClass()` — `Object.getClass()` on an array
/// receiver resolves to the array class (`[I`), one Class object for every
/// `int[]` and a different one for a `byte[]`. It used to fall through to a
/// handler arm that does not exist and end the app with `NoSuchMethod`.
#[test]
fn get_class_on_an_array_receiver() {
    use super::asm::Asm;
    fn class_compare(first: u8, second: u8) -> &'static [u8] {
        let mut a = Asm::new();
        let this = a.class("T");
        let obj = a.class(c::java_lang_Object);
        let get_class = a.methodref(
            0x0A,
            obj,
            crate::names::m::getClass,
            crate::names::d::__Class,
        );
        let (hi, lo) = ((get_class >> 8) as u8, get_class as u8);
        let code = [
            0x05, 0xBC, first, 0xB6, hi, lo, // iconst_2; newarray <first>; getClass
            0x06, 0xBC, second, 0xB6, hi, lo, // iconst_3; newarray <second>; getClass
            0xA6, 0x00, 0x05, // if_acmpne → iconst_0
            0x04, 0xAC, // iconst_1; ireturn
            0x03, 0xAC, // iconst_0; ireturn
        ];
        a.finish(0x0001, this, obj, &[], Some((2, &code, &[])))
    }
    // T_INT = 10, T_BYTE = 8.
    assert_eq!(run(class_compare(10, 10)).unwrap(), Some(Value::Int(1)));
    assert_eq!(run(class_compare(10, 8)).unwrap(), Some(Value::Int(0)));
}

/// M8: bootstrap methods are decoded from the `BootstrapMethods` attribute
/// in the class bytes; the record keeps only its offset.
#[test]
fn bootstrap_method_decodes_from_flash() {
    let cf = ClassFile::parse(spelled(CLASS_CALLER_INVOKEDYNAMIC)).unwrap();
    let bsm = cf.bootstrap_method(0).expect("one bootstrap entry");
    assert_eq!(bsm.method_ref, 22);
    assert_eq!(bsm.num_args, 3);
    let args: Vec<u16> = (0..3)
        .map(|k| cf.bootstrap_argument(&bsm, k).unwrap())
        .collect();
    assert_eq!(args, [23, 21, 24]);
    assert!(cf.bootstrap_argument(&bsm, 3).is_none());
    assert!(cf.bootstrap_method(1).is_none());

    // A class without the attribute has no entries at all.
    let cf = ClassFile::parse(spelled(CLASS_TARGET_LAMBDA)).unwrap();
    assert!(cf.bootstrap_method(0).is_none());
}

/// M8: an upcall from native code resolves under a hashed site (sixteen
/// bits of the name and descriptor), so a hit is trusted only after the
/// resolved method's own name and descriptor are checked. Two pairs forced
/// onto one key must not answer for each other.
#[test]
fn a_hashed_site_hit_is_verified_against_the_method_it_names() {
    use crate::resolve_cache::{ResolveCache, SiteKey};
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).unwrap();
    let cf_child = ClassFile::parse(spelled(CLASS_CHILD_SPEAK)).unwrap();
    let cf_ns = ClassFile::parse(spelled(CLASS_CHILD_NO_SPEAK)).unwrap();
    let classes = alloc::vec![cf_base, cf_child, cf_ns];
    let mut cache = ResolveCache::new();
    let key = SiteKey::hashed("speak", "()I").with_recv(SiteKey::recv_object(0));
    // Plant ChildNS.m (spells `m()V`) under speak's key — what a colliding
    // pair would leave — then ask for speak: the entry fails the check, the
    // walk resolves Child.speak, and the entry is replaced.
    cache.insert_method(key, 2, 0);
    assert!(key.is_hashed());
    assert!(!helpers::method_matches(
        Classes::linear(&classes),
        2,
        0,
        "speak",
        "()I"
    ));
    assert!(helpers::method_matches(
        Classes::linear(&classes),
        1,
        0,
        "speak",
        "()I"
    ));
    assert_eq!(cache.method(key).and_then(|h| h.java()), Some((2, 0)));
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            key,
            "Child",
            "speak",
            "()I",
            0,
        )
        .java(),
        Some((1, 0))
    );
    assert_eq!(cache.method(key).and_then(|h| h.java()), Some((1, 0)));
    // A pair no class declares finds nothing through the same key — a
    // native target, which a hashed site never remembers (a hit must be
    // verifiable against a method) — and does not disturb what is there.
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            key,
            "Child",
            "shout",
            "()I",
            0,
        )
        .target,
        Target::Native { hint: None }
    );
    assert_eq!(cache.method(key).and_then(|h| h.java()), Some((1, 0)));
    // An exact site never verifies: it is exact by construction.
    let exact = SiteKey::cp(0, 5).with_recv(SiteKey::recv_object(0));
    cache.insert_method(exact, 2, 0);
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            exact,
            "Child",
            "speak",
            "()I",
            0,
        )
        .java(),
        Some((2, 0))
    );
}

/// The class set holds `Base.speak` — a walk *would* find it — but the
/// site is cached as a native target, so the cache answers and no walk
/// happens. A miss that finds nothing is cached the same way, once.
#[test]
fn a_cached_native_target_is_answered_without_a_walk() {
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).unwrap();
    let classes = alloc::vec![cf_base];
    let mut cache = ResolveCache::new();
    let recv = SiteKey::recv_object(0);
    let key = SiteKey::cp(0, 5).with_recv(recv);
    cache.insert_target(key, Target::Native { hint: None }, 0);
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            key,
            "Base",
            "speak",
            "()I",
            0,
        )
        .target,
        Target::Native { hint: None }
    );
    // A miss walks, finds bytecode, and caches a Java target.
    let other = SiteKey::cp(0, 6).with_recv(recv);
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            other,
            "Base",
            "speak",
            "()I",
            0,
        )
        .java(),
        Some((0, 0))
    );
    assert_eq!(cache.method(other).unwrap().java(), Some((0, 0)));
    // A miss that finds nothing caches a native target.
    let none = SiteKey::cp(0, 7).with_recv(recv);
    assert_eq!(
        helpers::find_method_walking_cached(
            &mut cache,
            Classes::linear(&classes),
            none,
            "Base",
            "shout",
            "()I",
            0,
        )
        .target,
        Target::Native { hint: None }
    );
    assert_eq!(
        cache.method(none).unwrap().target,
        Target::Native { hint: None }
    );
    // The static / special resolver does the same for a class with no
    // class file at all.
    let st = SiteKey::cp(0, 8);
    assert_eq!(
        helpers::find_method_cached(
            &mut cache,
            Classes::linear(&classes),
            st,
            "Nowhere",
            "m",
            "()V",
            0,
        )
        .target,
        Target::Native { hint: None }
    );
    assert!(cache.method(st).is_some());
}

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}
fn lo(i: u16) -> u8 {
    i as u8
}

/// Run method 0 of class 0 of `classes` on `heap` with `handler`.
fn run_on<H: NativeMethodHandler>(
    classes: &[ClassFile],
    heap: &mut crate::SharedJvmHeap,
    handler: &mut H,
) -> Option<Value> {
    execute(
        classes,
        &mut heap.strings,
        &mut heap.objects,
        &mut heap.arrays,
        &mut heap.statics,
        &mut heap.gc_state,
        &mut heap.class_objects,
        handler,
        0,
        0,
        &[],
    )
    .unwrap()
}

/// A call on a class with no class file — `String.length()` here — walked
/// the class table three times on *every* call (the named class, its
/// chain, its interfaces) because only a found method was remembered. The
/// site now caches its native target: the second execution finds it.
#[test]
fn a_builtin_virtual_call_is_cached_as_a_native_target() {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(c::java_lang_Object);
    let s = a.string("abc");
    let string = a.class(c::java_lang_String);
    let len = a.methodref(0x0A, string, m::length, "()I");
    // "abc".length() + "abc".length()
    let code = alloc::vec![
        0x12,
        lo(s),
        0xB6,
        hi(len),
        lo(len),
        0x12,
        lo(s),
        0xB6,
        hi(len),
        lo(len),
        0x60,
        0xAC,
    ];
    let class = a.finish(0x0001, this, obj, &[], Some((2, &code, &[])));
    let classes = alloc::vec![ClassFile::parse(spelled(class)).unwrap()];
    let mut heap = crate::SharedJvmHeap::new();
    let mut h = NoopHandler;
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(6)));
    let hit = heap
        .class_objects
        .resolve
        .method(SiteKey::cp(0, len).with_recv(RECV_STRING))
        .expect("the site is cached");
    assert_eq!(hit.target, Target::Native { hint: None });
    assert_eq!(
        hit.flags & flags::PRECHECK,
        0,
        "String.length needs no pre-check"
    );
    // The cached target answers the second run.
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(6)));
}

/// `Math.abs(-5)`: Math has no class file in this set, so the first call
/// probes the initialised set (a class-table scan) and then marks the site
/// initialised; the second call skips the probe. Before the site could
/// hold a native target there was nothing to mark, so the probe ran on
/// every call.
#[test]
fn invokestatic_on_a_builtin_marks_the_site_initialised() {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(c::java_lang_Object);
    let math = a.class(c::java_lang_Math);
    let abs = a.methodref(0x0A, math, m::abs, "(I)I");
    let code = alloc::vec![0x10, 0xFB, 0xB8, hi(abs), lo(abs), 0xAC]; // bipush -5; invokestatic; ireturn
    let class = a.finish(0x0001, this, obj, &[], Some((1, &code, &[])));
    let classes = alloc::vec![ClassFile::parse(spelled(class)).unwrap()];
    let mut heap = crate::SharedJvmHeap::new();
    let mut h = NoopHandler;
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(5)));
    let hit = heap
        .class_objects
        .resolve
        .method(SiteKey::cp(0, abs))
        .expect("the site is cached");
    assert_eq!(hit.target, Target::Native { hint: None });
    assert!(
        hit.init,
        "a builtin counts as initialised, and the site remembers it"
    );
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(5)));
    assert!(
        heap.class_objects
            .resolve
            .method(SiteKey::cp(0, abs))
            .unwrap()
            .init
    );
}

/// Serves `Clock.clockNow()`: the shape of a framework native.
struct ClockHandler;
impl NativeMethodHandler for ClockHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        // Names no SDK member shares: `spelled()` rewrites SDK names for the
        // shrink lane, and a fixture name that collided would no longer match.
        (class_name == "Clock" && method_name == "clockNow").then_some(Ok(Some(Value::Int(7))))
    }
}

/// `Clock.clockNow()` is declared `native`: its class file has the method (no
/// `Code` attribute), so the walk finds it — and the site remembers
/// "native", not a `(class, method)` whose bytecode is re-checked on every
/// call. The framework-side twin of the builtin case: a `SystemClock`
/// static behaves exactly like this.
#[test]
fn a_native_method_with_a_class_file_is_cached_as_a_native_target() {
    let mut a = Asm::new();
    let this = a.class("Clock");
    let obj = a.class(c::java_lang_Object);
    let now = a.methodref(0x0A, this, "clockNow", "()I");
    let code = alloc::vec![0xB8, hi(now), lo(now), 0xB8, hi(now), lo(now), 0x60, 0xAC];
    let class = a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[
            Method {
                access: 0x0009,
                name: "m",
                desc: "()I",
                max_stack: 2,
                max_locals: 0,
                code: &code,
                exc: &[],
            },
            Method {
                access: 0x0109, // public static native
                name: "clockNow",
                desc: "()I",
                max_stack: 0,
                max_locals: 0,
                code: &[],
                exc: &[],
            },
        ],
    );
    let classes = alloc::vec![ClassFile::parse(spelled(class)).unwrap()];
    assert_eq!(classes[0].methods()[1].code_offset, 0);
    let mut heap = crate::SharedJvmHeap::new();
    let mut h = ClockHandler;
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(14)));
    let hit = heap
        .class_objects
        .resolve
        .method(SiteKey::cp(0, now))
        .expect("the site is cached");
    assert_eq!(hit.target, Target::Native { hint: None });
    assert!(
        hit.init,
        "Clock has a class file: initialised on the first call"
    );
    assert_eq!(run_on(&classes, &mut heap, &mut h), Some(Value::Int(14)));
}

/// The same `invokevirtual` site with two receiver classes resolves each
/// to its own override: the receiver is part of the key.
#[test]
fn one_site_two_receiver_classes_resolves_each() {
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).unwrap();
    let cf_child = ClassFile::parse(spelled(CLASS_CHILD_SPEAK)).unwrap();
    let cf_caller = ClassFile::parse(spelled(CLASS_CALLER_INVOKEVIRTUAL)).unwrap();
    let classes = alloc::vec![cf_base, cf_child, cf_caller];
    let mut heap = crate::SharedJvmHeap::new();
    let mut h = NoopHandler;
    let mut call = |heap: &mut crate::SharedJvmHeap, name: &'static str| {
        let obj = Value::ObjectRef(heap.objects.alloc(name).unwrap());
        execute(
            &classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            &mut heap.gc_state,
            &mut heap.class_objects,
            &mut h,
            2,
            0,
            &[obj],
        )
        .unwrap()
    };
    assert_eq!(call(&mut heap, "Child"), Some(Value::Int(2)));
    assert_eq!(call(&mut heap, "Base"), Some(Value::Int(1)));
    assert_eq!(call(&mut heap, "Child"), Some(Value::Int(2)));
    assert_eq!(call(&mut heap, "Base"), Some(Value::Int(1)));
    // One site, two entries, keyed by the receiver's class id: Child's
    // points at Child.speak, Base's at Base.speak, and nothing sits under
    // the receiver-less key.
    let site = SiteKey::cp(2, 8);
    let id_of = |heap: &mut crate::SharedJvmHeap, name: &'static str| {
        let obj = heap.objects.alloc(name).unwrap();
        heap.objects.class_id(obj).unwrap()
    };
    let child = id_of(&mut heap, "Child");
    let base = id_of(&mut heap, "Base");
    let slot = |recv: u16| {
        heap.class_objects
            .resolve
            .method(site.with_recv(SiteKey::recv_object(recv)))
            .map(|h| h.target)
    };
    assert_eq!(slot(child), Some(Target::Java { ci: 1, mi: 0 }));
    assert_eq!(slot(base), Some(Target::Java { ci: 0, mi: 0 }));
    assert!(heap.class_objects.resolve.method(site).is_none());
}

/// JVMS §6.5: an invoke on a null receiver throws NullPointerException
/// before anything else — before resolution, so the site keeps no entry
/// for the null (which has no class), and a later real call resolves as
/// if the null had never happened.
#[test]
fn a_null_receiver_throws_npe_and_leaves_the_slot_untouched() {
    let cf_base = ClassFile::parse(spelled(CLASS_BASE_SPEAK)).unwrap();
    let cf_child = ClassFile::parse(spelled(CLASS_CHILD_SPEAK)).unwrap();
    let cf_caller = ClassFile::parse(spelled(CLASS_CALLER_INVOKEVIRTUAL)).unwrap();
    let classes = alloc::vec![cf_base, cf_child, cf_caller];
    let mut heap = crate::SharedJvmHeap::new();
    let mut h = NoopHandler;
    let mut call = |heap: &mut crate::SharedJvmHeap, recv: Value| {
        execute(
            &classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            &mut heap.gc_state,
            &mut heap.class_objects,
            &mut h,
            2,
            0,
            &[recv],
        )
    };
    match call(&mut heap, Value::Null) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, c::java_lang_NullPointerException),
        other => panic!("expected NullPointerException, got {other:?}"),
    }
    assert!(heap
        .class_objects
        .resolve
        .method(SiteKey::cp(2, 8))
        .is_none());
    let base = Value::ObjectRef(heap.objects.alloc("Base").unwrap());
    assert_eq!(call(&mut heap, base), Ok(Some(Value::Int(1))));
}

/// One `Methodref` shared by an `invokespecial` and an `invokevirtual`
/// (`super.speak()` next to `this.speak()`, javac's usual output): the
/// non-virtual site resolves under the receiver-less key to the declared
/// class's method, the virtual one under the receiver's class to the
/// override, and neither answer leaks into the other — in either order of
/// first use.
#[test]
fn shared_methodref_used_by_both_invokespecial_and_invokevirtual_resolves_each() {
    // Kid extends Base; Kid.speak()I = 2 (Base.speak()I = 1).
    // m1(Kid)I = 10 * special + virtual = 12; m2(Kid)I = 10 * virtual + special = 21.
    let mut a = Asm::new();
    let this = a.class("Kid");
    let sup = a.class("Base");
    let speak = a.methodref(0x0A, sup, "speak", "()I");
    let (sh, sl) = ((speak >> 8) as u8, speak as u8);
    let m1 = [
        0x2A, 0xB7, sh, sl, 0x10, 10, 0x68, 0x2A, 0xB6, sh, sl, 0x60, 0xAC,
    ];
    let m2 = [
        0x2A, 0xB6, sh, sl, 0x10, 10, 0x68, 0x2A, 0xB7, sh, sl, 0x60, 0xAC,
    ];
    let method = |name: &'static str, code: &'static [u8], access: u16, max_locals: u16| Method {
        access,
        name,
        desc: if access & 0x0008 != 0 {
            "(LKid;)I"
        } else {
            "()I"
        },
        max_stack: 2,
        max_locals,
        code,
        exc: &[],
    };
    let m1: &'static [u8] = alloc::boxed::Box::leak(alloc::boxed::Box::new(m1));
    let m2: &'static [u8] = alloc::boxed::Box::leak(alloc::boxed::Box::new(m2));
    let kid = a.finish_methods(
        0x0021,
        this,
        sup,
        &[],
        &[
            method("speak", &[0x05, 0xAC], 0x0001, 1),
            method("m1", m1, 0x0008, 1),
            method("m2", m2, 0x0008, 1),
        ],
    );
    let classes = alloc::vec![
        ClassFile::parse(spelled(CLASS_BASE_SPEAK)).unwrap(),
        ClassFile::parse(kid).unwrap(),
    ];
    let mut h = NoopHandler;
    for (mi, expect) in [(1, 12), (2, 21)] {
        let mut heap = crate::SharedJvmHeap::new();
        for _ in 0..2 {
            let obj = Value::ObjectRef(heap.objects.alloc("Kid").unwrap());
            let r = execute(
                &classes,
                &mut heap.strings,
                &mut heap.objects,
                &mut heap.arrays,
                &mut heap.statics,
                &mut heap.gc_state,
                &mut heap.class_objects,
                &mut h,
                1,
                mi,
                &[obj],
            );
            assert_eq!(r, Ok(Some(Value::Int(expect))), "m{mi}");
        }
    }
}

/// The argument count the packer stores in a `Methodref`'s descriptor
/// record is what the interpreter pops: one per parameter whatever its
/// width or shape, `this` excluded.
#[test]
fn pack_time_argc_matches_the_interpreter_count() {
    for desc in [
        "()V",
        "(I)V",
        "(JD)V",
        "(Ljava/lang/String;[I[[Ljava/lang/Object;J)I",
        "([J)V",
        "(BCSZFD)Ljava/lang/String;",
        "([[[Ljava/util/List;IJ)V",
    ] {
        assert_eq!(
            class_link::count_args(desc.as_bytes()).map(|n| n as usize),
            Some(helpers::count_args(desc)),
            "{desc}"
        );
    }
}

/// Step 4 of docs/designs/class-link-2026-09.md: a resolution walk compares
/// signature hashes and reads a method's name only to confirm the one hash
/// match — a three-deep chain with six methods a class costs one
/// `method_name` read on a hit and none on a miss, however many methods the
/// chain holds. (`TextView → View → Object` is the framework shape this
/// stands in for; `View` alone has ~150 methods.)
#[test]
fn a_hash_walk_reads_one_name() {
    use crate::class_file::METHOD_NAME_READS;
    use core::sync::atomic::Ordering::Relaxed;
    fn class(name: &str, sup: &str, prefix: &str) -> &'static [u8] {
        let mut a = Asm::new();
        let this = a.class(name);
        let sup = a.class(sup);
        let names: alloc::vec::Vec<alloc::string::String> =
            (0..6).map(|i| alloc::format!("{prefix}{i}")).collect();
        let methods: alloc::vec::Vec<Method<'_>> = names
            .iter()
            .map(|n| Method {
                access: 0x0401, // public abstract: a declaration is enough
                name: n,
                desc: "()I",
                max_stack: 0,
                max_locals: 0,
                code: &[],
                exc: &[],
            })
            .collect();
        a.finish_methods(0x0021, this, sup, &[], &methods)
    }
    let root = ClassFile::parse(class("Root", c::java_lang_Object, "r")).unwrap();
    let mid = ClassFile::parse(class("Mid", "Root", "m")).unwrap();
    let leaf = ClassFile::parse(class("Leaf", "Mid", "l")).unwrap();
    let classes = alloc::vec![root, mid, leaf];
    let classes = Classes::linear(&classes);

    // A hit two levels up: eighteen methods scanned, one name read.
    METHOD_NAME_READS.store(0, Relaxed);
    assert_eq!(
        helpers::find_method_walking(classes, "Leaf", "r3", "()I"),
        Some((0, 3))
    );
    assert_eq!(METHOD_NAME_READS.load(Relaxed), 1);

    // A miss: the whole chain (and the empty default-method search), no reads.
    METHOD_NAME_READS.store(0, Relaxed);
    assert_eq!(
        helpers::find_method_walking(classes, "Leaf", "nope", "()I"),
        None
    );
    assert_eq!(METHOD_NAME_READS.load(Relaxed), 0);

    // Same name, other descriptor: the hash differs, so no read either.
    METHOD_NAME_READS.store(0, Relaxed);
    assert_eq!(
        helpers::find_method_walking(classes, "Leaf", "r3", "()V"),
        None
    );
    assert_eq!(METHOD_NAME_READS.load(Relaxed), 0);
}
