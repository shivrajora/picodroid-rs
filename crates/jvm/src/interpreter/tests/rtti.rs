// SPDX-License-Identifier: GPL-3.0-only
//! Runtime type information: the builtin class-hierarchy tables, transitive
//! superinterface walking, `checkcast`/`instanceof` on strings and arrays,
//! the catchable `ClassCastException`, and String receivers dispatching as
//! `java/lang/String` whatever the constant pool declared.
use super::asm::{Asm, ACC_INTERFACE};
use super::*;
use crate::names::{c, d};
use crate::names::{m, spelled};
use crate::native::BUILTIN_CLASS_NAMES;
use alloc::vec;

fn iface(name: &str, supers: &[&str]) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let obj = a.class(c::java_lang_Object);
    let sups: Vec<u16> = supers.iter().map(|s| a.class(s)).collect();
    a.finish(ACC_INTERFACE, this, obj, &sups, None)
}

fn plain_class(name: &str, sup: &str, ifaces: &[&str]) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let sup = a.class(sup);
    let ifs: Vec<u16> = ifaces.iter().map(|s| a.class(s)).collect();
    a.finish(0x0001, this, sup, &ifs, None)
}

fn parse_all(data: &[&'static [u8]]) -> Vec<ClassFile> {
    data.iter()
        .map(|d| ClassFile::parse(spelled(d)).expect("parse"))
        .collect()
}

// ── Table hygiene ─────────────────────────────────────────────────────────

/// Every class the hierarchy tables name must canonicalise, or a `new` of it
/// produces an `"unknown"` object that no `catch`/`checkcast` ever matches.
#[test]
fn builtin_hierarchy_names_are_registered() {
    let registered = |n: &str| BUILTIN_CLASS_NAMES.contains(&n);
    for (k, s) in helpers::BUILTIN_SUPER {
        assert!(
            registered(k),
            "{k} (BUILTIN_SUPER key) not in BUILTIN_CLASS_NAMES"
        );
        assert!(
            registered(s),
            "{s} (BUILTIN_SUPER value) not in BUILTIN_CLASS_NAMES"
        );
    }
    for (k, ifs) in helpers::BUILTIN_INTERFACES {
        assert!(
            registered(k),
            "{k} (BUILTIN_INTERFACES key) not in BUILTIN_CLASS_NAMES"
        );
        for i in *ifs {
            assert!(
                registered(i),
                "{i} (interface of {k}) not in BUILTIN_CLASS_NAMES"
            );
        }
    }
}

// ── is_instance_of over the builtin tables ────────────────────────────────

#[test]
fn builtin_collections_and_boxes_implement_their_interfaces() {
    let classes: Vec<ClassFile> = Vec::new();
    let is = |rt: &str, t: &str| helpers::is_instance_of(&classes, rt, t);
    for t in [
        c::java_util_List,
        c::java_util_Collection,
        c::java_lang_Iterable,
        c::java_lang_Object,
    ] {
        assert!(is(c::java_util_ArrayList, t), "ArrayList is a {t}");
    }
    assert!(!is(c::java_util_ArrayList, c::java_util_Map));
    assert!(!is(c::java_util_ArrayList, c::java_util_Set));
    assert!(is(c::java_util_HashMap, c::java_util_Map));
    assert!(is(c::java_util_HashMap_KeySet, c::java_util_Set));
    assert!(is(c::java_util_HashMap_Values, c::java_lang_Iterable));
    for t in [
        c::java_lang_Number,
        c::java_lang_Comparable,
        c::java_lang_Object,
    ] {
        assert!(is(c::java_lang_Integer, t), "Integer is a {t}");
        assert!(is(c::java_lang_Float, t), "Float is a {t}");
    }
    assert!(!is(c::java_lang_Integer, c::java_lang_Long));
    assert!(!is(c::java_lang_Boolean, c::java_lang_Number));
    assert!(is(c::java_lang_String, c::java_lang_CharSequence));
    assert!(is(c::java_lang_String, c::java_lang_Comparable));
    assert!(is(c::java_lang_StringBuilder, c::java_lang_CharSequence));
    assert!(is(
        c::java_util_NoSuchElementException,
        c::java_lang_RuntimeException
    ));
    assert!(is(
        c::java_util_NoSuchElementException,
        c::java_lang_Throwable
    ));
    assert!(is(
        c::java_lang_ClassCastException,
        c::java_lang_RuntimeException
    ));
}

/// `class Sub extends Base implements KList`, `interface KList extends
/// java/util/List`: the walk crosses a loaded interface class file and then
/// the classfile-less `List → Collection → Iterable` rows.
#[test]
fn superinterfaces_are_walked_transitively() {
    let classes = parse_all(&[
        iface("KList", &[c::java_util_List]),
        plain_class("Base", c::java_lang_Object, &[]),
        plain_class("Sub", "Base", &["KList"]),
    ]);
    let is = |rt: &str, t: &str| helpers::is_instance_of(&classes, rt, t);
    assert!(is("Sub", "KList"));
    assert!(is("Sub", c::java_util_List));
    assert!(is("Sub", c::java_util_Collection));
    assert!(is("Sub", c::java_lang_Iterable));
    assert!(is("Sub", "Base"));
    assert!(is("Sub", c::java_lang_Object));
    assert!(!is("Sub", c::java_util_Map));
    assert!(!is("Base", "KList"));
}

/// A diamond of interfaces with a superinterface three levels up, and an
/// interface that has no class file at all (Kotlin's `KMappedMarker`).
#[test]
fn deep_interface_chains_and_missing_interfaces_are_tolerated() {
    let classes = parse_all(&[
        iface("Top", &[]),
        iface("Left", &["Top"]),
        iface("Right", &["Top"]),
        iface("Bottom", &["Left", "Right"]),
        plain_class(
            "Impl",
            c::java_lang_Object,
            &["Bottom", "kotlin/jvm/internal/markers/KMappedMarker"],
        ),
    ]);
    let is = |rt: &str, t: &str| helpers::is_instance_of(&classes, rt, t);
    assert!(is("Impl", "Top"));
    assert!(is("Impl", "Right"));
    assert!(is("Impl", "kotlin/jvm/internal/markers/KMappedMarker"));
    assert!(is("Impl", c::java_lang_Object));
    assert!(!is("Impl", c::java_lang_Runnable));
}

/// Hand-assembled cycles must terminate (valid class files cannot cycle).
#[test]
fn interface_cycle_terminates() {
    let classes = parse_all(&[
        iface("A", &["B"]),
        iface("B", &["A"]),
        plain_class("C", c::java_lang_Object, &["A"]),
    ]);
    assert!(helpers::is_instance_of(&classes, "C", "B"));
    assert!(!helpers::is_instance_of(&classes, "C", "Nope"));
}

// ── value_is_instance: strings, arrays, null ──────────────────────────────

#[test]
fn strings_and_arrays_have_runtime_classes() {
    let classes: Vec<ClassFile> = Vec::new();
    let objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let ints = Value::ArrayRef(arrays.alloc(crate::array_heap::ATYPE_INT, 2).unwrap());
    let refs = Value::ArrayRef(arrays.alloc(crate::array_heap::ATYPE_REF, 2).unwrap());
    let s = Value::Reference(0);
    let is = |v: Value, t: &str| helpers::value_is_instance(&classes, &objects, &arrays, v, t);
    for t in [
        c::java_lang_String,
        c::java_lang_CharSequence,
        c::java_lang_Comparable,
        c::java_lang_Object,
    ] {
        assert!(is(s, t), "String is a {t}");
    }
    assert!(!is(s, c::java_util_ArrayList));
    assert!(!is(s, c::java_lang_Integer));
    assert!(is(ints, "[I"));
    assert!(!is(ints, "[F"));
    assert!(!is(ints, d::t_aObject));
    assert!(is(ints, c::java_lang_Object));
    assert!(is(ints, c::java_lang_Cloneable));
    assert!(!is(ints, c::java_lang_String));
    assert!(is(refs, d::t_aObject));
    assert!(is(refs, d::t_aString)); // element class not recorded
    assert!(is(refs, "[[I"));
    assert!(!is(refs, "[I"));
    assert!(!is(Value::Null, c::java_lang_Object));
    assert!(!is(Value::Int(1), c::java_lang_Integer));
}

// ── End to end: checkcast / instanceof / ClassCastException ───────────────

/// `m()I`: `ldc "str"; checkcast <target>` inside a try region, then
/// `iconst_1; ireturn`; handler `pop; bipush 7; ireturn` catching `catch`.
fn cast_class(target: &str, catch: Option<&str>) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(c::java_lang_Object);
    let s = a.string("str");
    let t = a.class(target);
    let c = catch.map(|c| a.class(c));
    let code = [
        0x12,
        s as u8, // ldc
        0xC0,
        (t >> 8) as u8,
        t as u8, // checkcast
        0x57,    // pop
        0x04,    // iconst_1
        0xAC,    // ireturn
        0x57,    // (8) pop — handler
        0x10,
        0x07, // bipush 7
        0xAC, // ireturn
    ];
    let exc: Vec<[u16; 4]> = c.map(|c| vec![[0, 8, 8, c]]).unwrap_or_default();
    a.finish(0x0001, this, obj, &[], Some((2, &code, &exc)))
}

#[test]
fn checkcast_passes_for_string_targets() {
    for t in [
        c::java_lang_String,
        c::java_lang_CharSequence,
        c::java_lang_Object,
    ] {
        assert_eq!(
            run(cast_class(t, None)).unwrap(),
            Some(Value::Int(1)),
            "{t}"
        );
    }
}

#[test]
fn failed_checkcast_throws_catchable_class_cast_exception() {
    let r = run(cast_class(
        c::java_util_ArrayList,
        Some(c::java_lang_ClassCastException),
    ));
    assert_eq!(r.unwrap(), Some(Value::Int(7)));
    // Superclass catches match too (builtin_super chain).
    let r = run(cast_class(
        c::java_util_ArrayList,
        Some(c::java_lang_RuntimeException),
    ));
    assert_eq!(r.unwrap(), Some(Value::Int(7)));
}

#[test]
fn uncaught_class_cast_exception_names_its_class() {
    let r = run(cast_class(c::java_util_ArrayList, None));
    match r {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, c::java_lang_ClassCastException),
        other => panic!("expected uncaught ClassCastException, got {other:?}"),
    }
}

#[test]
fn class_cast_exception_is_alloc_by_name() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut class_objects = crate::class_objects::ClassObjectCache::new();
    let mut handler = NoopHandler;
    let classes: Vec<ClassFile> = Vec::new();
    let mut ex = Executor {
        classes: &classes,
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        statics: &mut statics,
        gc_state: &mut gc_state,
        class_objects: &mut class_objects,
        handler: &mut handler,
        field_cache: Vec::new(),
        method_cache: Vec::new(),
        static_field_cache: Vec::new(),
        pending_frame: None,
        pending_clinit_frames: Vec::new(),
        insn_count: 0,
        upcall_depth: 0,
    };
    let JvmError::Exception(e) = ex.class_cast_exception() else {
        panic!("expected an Exception");
    };
    assert_eq!(
        ex.objects.class_name(e),
        Some(c::java_lang_ClassCastException)
    );
}

/// `m()I`: `<load>; instanceof <target>; ireturn`.
fn instanceof_class(load: &[u8], target: &str) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(c::java_lang_Object);
    let s = a.string("str");
    let t = a.class(target);
    let mut code: Vec<u8> = Vec::new();
    // `load` uses 0xFF as a placeholder for the string CP index.
    code.extend(load.iter().map(|&b| if b == 0xFF { s as u8 } else { b }));
    code.extend_from_slice(&[0xC1, (t >> 8) as u8, t as u8, 0xAC]);
    a.finish(0x0001, this, obj, &[], Some((1, &code, &[])))
}

#[test]
fn instanceof_sees_strings_arrays_and_null() {
    let ldc_str: &[u8] = &[0x12, 0xFF];
    assert_eq!(
        run(instanceof_class(ldc_str, c::java_lang_String)).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        run(instanceof_class(ldc_str, c::java_lang_Comparable)).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        run(instanceof_class(ldc_str, c::java_util_List)).unwrap(),
        Some(Value::Int(0))
    );
    // iconst_2; newarray int (0xBC 0x0A)
    let int_array: &[u8] = &[0x05, 0xBC, 0x0A];
    assert_eq!(
        run(instanceof_class(int_array, "[I")).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        run(instanceof_class(int_array, "[F")).unwrap(),
        Some(Value::Int(0))
    );
    assert_eq!(
        run(instanceof_class(int_array, c::java_lang_Object)).unwrap(),
        Some(Value::Int(1))
    );
    let null: &[u8] = &[0x01];
    assert_eq!(
        run(instanceof_class(null, c::java_lang_Object)).unwrap(),
        Some(Value::Int(0))
    );
}

#[test]
fn checkcast_null_passes() {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(c::java_lang_Object);
    let t = a.class(c::java_util_ArrayList);
    let code = [0x01, 0xC0, (t >> 8) as u8, t as u8, 0x57, 0x04, 0xAC];
    let cls = a.finish(0x0001, this, obj, &[], Some((1, &code, &[])));
    assert_eq!(run(cls).unwrap(), Some(Value::Int(1)));
}

// ── String receivers dispatch as java/lang/String ─────────────────────────

/// `m()I`: `ldc "a"; [ldc "b";] invoke<kind> <owner>.<name><desc>; ireturn`.
fn string_call(
    two_args: bool,
    opcode: u8,
    tag: u8,
    owner: &str,
    name: &str,
    desc: &str,
) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(c::java_lang_Object);
    let sa = a.string("a");
    let sb = a.string("b");
    let owner = a.class(owner);
    let m = a.methodref(tag, owner, name, desc);
    let mut code = vec![0x12, sa as u8];
    if two_args {
        code.extend_from_slice(&[0x12, sb as u8]);
    }
    code.extend_from_slice(&[opcode, (m >> 8) as u8, m as u8]);
    if opcode == 0xB9 {
        code.extend_from_slice(&[if two_args { 2 } else { 1 }, 0]);
    }
    code.push(0xAC);
    a.finish(0x0001, this, obj, &[], Some((2, &code, &[])))
}

#[test]
fn comparable_compare_to_on_a_string_reaches_the_string_dispatcher() {
    let cls = string_call(
        true,
        0xB9,
        0x0B,
        c::java_lang_Comparable,
        m::compareTo,
        d::Object__I,
    );
    assert_eq!(run(cls).unwrap(), Some(Value::Int(-1)));
}

#[test]
fn char_sequence_length_on_a_string() {
    let cls = string_call(
        false,
        0xB9,
        0x0B,
        c::java_lang_CharSequence,
        m::length,
        "()I",
    );
    assert_eq!(run(cls).unwrap(), Some(Value::Int(1)));
}

#[test]
fn object_hash_code_and_equals_on_a_string() {
    let cls = string_call(false, 0xB6, 0x0A, c::java_lang_Object, m::hashCode, "()I");
    assert_eq!(run(cls).unwrap(), Some(Value::Int(97)));
    let cls = string_call(
        true,
        0xB6,
        0x0A,
        c::java_lang_Object,
        m::equals,
        d::Object__Z,
    );
    assert_eq!(run(cls).unwrap(), Some(Value::Int(0)));
}

// ── Identity methods reach java/lang/Object for user classes ──────────────

/// `m()I`: `new P; dup; dup; invokevirtual P.<name><desc>; ireturn` for a
/// class `P` that declares nothing — the call must fall through to
/// `Object`'s identity arms, not die as a NoSuchMethod at `P` (whose
/// `super_class_name()` is `None` because the parent is Object).
fn identity_call(name: &str, desc: &str, two_args: bool) -> Vec<&'static [u8]> {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(c::java_lang_Object);
    let p = a.class("P");
    let m = a.methodref(0x0A, p, name, desc);
    let mut code = vec![0xBB, (p >> 8) as u8, p as u8, 0x59];
    if two_args {
        code.push(0x59);
    }
    code.extend_from_slice(&[0xB6, (m >> 8) as u8, m as u8, 0xAC]);
    let t = a.finish(0x0001, this, obj, &[], Some((3, &code, &[])));
    vec![plain_class("P", c::java_lang_Object, &[]), t]
}

#[test]
fn identity_equals_and_hash_code_on_a_plain_user_class() {
    let classes = identity_call(m::equals, d::Object__Z, true);
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(1)));
    let classes = identity_call(m::hashCode, "()I", false);
    assert!(matches!(
        run_multi(&classes, 1, &[]).unwrap(),
        Some(Value::Int(_))
    ));
}
