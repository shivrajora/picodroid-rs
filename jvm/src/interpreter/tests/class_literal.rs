use super::*;
use crate::class_objects::ClassObjectCache;
use crate::gc::GcState;

// ── Class file used for `ldc CONSTANT_Class` testing ─────────────────────
//
// Class "T" extends Object. CP layout matches the standard test class in
// `mod.rs::build_class` so we can borrow the same shared header. Method m
// returns Z and exercises ldc CONSTANT_Class for #1 (T).
//
// CP (cp_count = 8):
//   #1: Class       -> #2
//   #2: Utf8        "T"        ← target of CONSTANT_Class lookup
//   #3: Class       -> #4
//   #4: Utf8        "java/lang/Object"
//   #5: Utf8        "m"
//   #6: Utf8        "()Z"      ← booleanReturn
//   #7: Utf8        "Code"
//
// Bytecode:
//   ldc #1            (0x12 0x01)  → push T.class
//   ldc #1            (0x12 0x01)  → push T.class again
//   if_acmpne L_FAIL  (0xA6 0x00 0x05)  → branch +5 if differing references
//   iconst_1          (0x04)       ← reached when refs are identical
//   ireturn           (0xAC)
// L_FAIL:
//   iconst_0          (0x03)
//   ireturn           (0xAC)
//
// `if_acmpne` offset is relative to the instruction start (PC=4). To skip
// past `iconst_1, ireturn` (PC=7,8) and land on `iconst_0` (PC=9), offset = 5.

static CLASS_T_LDC_IDENTITY: &[u8] = &[
    0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, // magic + version
    0x00, 0x08, // cp_count=8
    0x07, 0x00, 0x02, // #1 Class -> #2
    0x01, 0x00, 0x01, b'T', // #2 Utf8 "T"
    0x07, 0x00, 0x04, // #3 Class -> #4
    0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j',
    b'e', b'c', b't', // #4 Utf8 "java/lang/Object"
    0x01, 0x00, 0x01, b'm', // #5 Utf8 "m"
    0x01, 0x00, 0x03, b'(', b')', b'Z', // #6 Utf8 "()Z"
    0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // #7 Utf8 "Code"
    0x00, 0x01, 0x00, 0x01, 0x00, 0x03, // access=1, this=#1, super=#3
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // ifaces=0, fields=0, methods=1
    0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00,
    0x01, // method: public, name=#5, desc=#6, attrs=1
    // Code attr: name=#7, attr_len = 12 + code_len(11) = 23 = 0x17
    0x00, 0x07, 0x00, 0x00, 0x00, 0x17, // max_stack=2, max_locals=1
    0x00, 0x02, 0x00, 0x01, // code_len=11
    0x00, 0x00, 0x00, 0x0B, // bytecode (11 bytes)
    0x12, 0x01, // ldc #1
    0x12, 0x01, // ldc #1
    0xA6, 0x00, 0x05, // if_acmpne +5
    0x04, // iconst_1
    0xAC, // ireturn
    0x03, // iconst_0
    0xAC, // ireturn
    // exception_table_length=0, code_attrs=0
    0x00, 0x00, 0x00, 0x00, // class attrs=0
    0x00, 0x00,
];

#[test]
fn ldc_class_literal_pushes_object_ref() {
    // Method m()I that does `ldc #1; ireturn` — but ireturn requires int, so we
    // reuse CLASS_T_LDC_IDENTITY's bytecode (returns boolean, encoded as int).
    let cf = ClassFile::parse(CLASS_T_LDC_IDENTITY).expect("parse failed");
    let mut classes = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut class_objects = ClassObjectCache::new();
    let mut handler = NoopHandler;

    let result = crate::interpreter::execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut class_objects,
        &mut handler,
        0,
        0,
        &[],
    );

    // Two ldc CONSTANT_Class for "T" yield the same ObjectRef → if_acmpne does
    // NOT branch → we hit iconst_1, ireturn → returns 1.
    assert_eq!(result, Ok(Some(Value::Int(1))));

    // Verify the cache holds an entry for T pointing at a real Class object.
    let name_idx = strings.intern(b"T").unwrap();
    let class_obj = class_objects
        .lookup(name_idx)
        .expect("cache must have an entry for T after ldc");
    assert_eq!(objects.class_name(class_obj), Some("java/lang/Class"));

    // Slot 0 holds the class name as a Reference into the StringTable.
    match objects.get_field(class_obj, 0) {
        Some(Value::Reference(idx)) => {
            assert_eq!(strings.resolve(idx), Some("T"));
        }
        other => panic!("expected Value::Reference for slot 0, got {:?}", other),
    }
}

#[test]
fn ldc_class_for_unknown_class_errors() {
    // CP entry #1 references a class named "T" — but we never load any class.
    // resolve_class_literal must return ClassNotFound (not panic, not InvalidBytecode).
    let cf = ClassFile::parse(CLASS_T_LDC_IDENTITY).expect("parse failed");
    let mut classes = Vec::new();
    // Override: pretend T isn't loaded. Easiest way is to load a *different*
    // class — but we only have one bytestring. So strip the test class entirely
    // and run a snippet that ldcs an unknown CP class index. For this test we
    // skip the orchestration and call resolve_ldc directly.
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut class_objects = ClassObjectCache::new();

    // CP index #3 is `Class -> #4 = "java/lang/Object"`. That class is not
    // loaded, so the resolver must return ClassNotFound.
    let cf_ref = &classes[0];
    let result = crate::interpreter::helpers::resolve_ldc(
        cf_ref,
        &classes,
        &mut strings,
        &mut objects,
        &mut class_objects,
        3,
    );
    assert_eq!(result, Err(JvmError::ClassNotFound));
}
