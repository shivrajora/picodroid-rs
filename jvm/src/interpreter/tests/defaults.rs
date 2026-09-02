// SPDX-License-Identifier: GPL-3.0-only
//! Kotlin roadmap Session 4: interface default methods (JVMS §5.4.3.3
//! maximally-specific superinterface resolution), the
//! `StringBuilder.append(Object)` / `String.valueOf(Object)` `toString()`
//! trampoline, `Map.entrySet()` + `Map$Entry`, the `LinkedHashMap` /
//! `LinkedHashSet` aliases, `Collection.toArray` and the name-only
//! `java/util/Locale`.
use super::asm::{Asm, Method, ACC_INTERFACE};
use super::*;
use crate::names::{c, d, m};
use alloc::vec;

const OBJ: &str = c::java_lang_Object;
const F_DESC: &str = "()I";

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}

fn lo(i: u16) -> u8 {
    i as u8
}

/// `String.hashCode()` of an ASCII string, for asserting builder contents.
fn jhash(s: &str) -> i32 {
    s.bytes()
        .fold(0i32, |h, b| h.wrapping_mul(31).wrapping_add(b as i32))
}

/// `f()I` returning `n` (`iconst_n; ireturn`), or an abstract declaration.
fn f_method(body: Option<u8>) -> Method<'static> {
    let (access, code): (u16, &'static [u8]) = match body {
        Some(n) => (
            0x0001,
            alloc::boxed::Box::leak(vec![0x03 + n, 0xAC].into_boxed_slice()),
        ),
        None => (0x0401, &[]),
    };
    Method {
        access,
        name: "f",
        desc: F_DESC,
        max_stack: 1,
        max_locals: 1,
        code,
        exc: &[],
    }
}

/// Interface `name extends supers`, with `f()I` returning `body` when
/// `Some` (a default method), an abstract `f()I` when `None`.
fn iface_f(name: &str, supers: &[&str], body: Option<u8>) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let obj = a.class(OBJ);
    let sups: Vec<u16> = supers.iter().map(|s| a.class(s)).collect();
    a.finish_methods(ACC_INTERFACE, this, obj, &sups, &[f_method(body)])
}

/// Class `name extends sup implements ifaces`; `f()I` returning `body` when
/// `Some`, no `f` at all when `None`. `access` 0x0401 makes it abstract.
fn class_f(name: &str, sup: &str, ifaces: &[&str], access: u16, body: Option<u8>) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let sup = a.class(sup);
    let ifs: Vec<u16> = ifaces.iter().map(|s| a.class(s)).collect();
    match body {
        Some(n) => a.finish_methods(access, this, sup, &ifs, &[f_method(Some(n))]),
        None => a.finish_methods(access, this, sup, &ifs, &[]),
    }
}

/// `m()I`: `new C; invoke<opcode> owner.f()I; ireturn`.
fn call_f(opcode: u8, tag: u8, owner: &str) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let c = a.class("C");
    let owner = a.class(owner);
    let m = a.methodref(tag, owner, "f", F_DESC);
    let mut code = vec![0xBB, hi(c), lo(c), opcode, hi(m), lo(m)];
    if opcode == 0xB9 {
        code.extend_from_slice(&[1, 0]);
    }
    code.push(0xAC);
    a.finish(0x0001, this, obj, &[], Some((2, &code, &[])))
}

/// Run `T.m()` with the given classes loaded; `T` is appended last.
fn run_with(classes: &[&'static [u8]]) -> Result<Option<Value>, JvmError> {
    let mut all: Vec<&'static [u8]> = classes.to_vec();
    all.push(call_f(0xB9, 0x0B, "I"));
    run_multi(&all, all.len() - 1, &[])
}

// ── Interface default methods ─────────────────────────────────────────────

#[test]
fn default_method_resolves_when_class_has_no_override() {
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("C", OBJ, &["I"], 0x0001, None),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(1)));
}

#[test]
fn class_override_beats_interface_default() {
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("C", OBJ, &["I"], 0x0001, Some(2)),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(2)));
}

#[test]
fn subinterface_default_beats_superinterface_default() {
    let classes = [
        iface_f("I", &[], Some(1)),
        iface_f("J", &["I"], Some(3)),
        class_f("C", OBJ, &["J"], 0x0001, None),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(3)));
}

#[test]
fn most_specific_default_wins_whatever_the_implements_order() {
    // `C implements I, J` with `J extends I`: J.f overrides I.f, so J.f is
    // the maximally-specific method even though I is listed (and visited)
    // first.
    let classes = [
        iface_f("I", &[], Some(1)),
        iface_f("J", &["I"], Some(3)),
        class_f("C", OBJ, &["I", "J"], 0x0001, None),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(3)));
}

#[test]
fn default_found_through_abstract_superclass() {
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("A", OBJ, &["I"], 0x0401, None),
        class_f("C", "A", &[], 0x0001, None),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(1)));
}

#[test]
fn default_found_past_a_missing_interface_class_file() {
    // kotlinc adds `kotlin/jvm/internal/markers/KMappedMarker` to classes
    // implementing read-only collections; it ships no class file.
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f(
            "C",
            OBJ,
            &["kotlin/jvm/internal/markers/KMappedMarker", "I"],
            0x0001,
            None,
        ),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(1)));
}

#[test]
fn default_found_when_superclass_chain_leaves_the_loaded_set() {
    // A user class extending a classfile-less builtin still resolves its
    // interface defaults.
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("C", c::java_lang_RuntimeException, &["I"], 0x0001, None),
    ];
    assert_eq!(run_with(&classes).unwrap(), Some(Value::Int(1)));
}

#[test]
fn invokespecial_on_the_interface_reaches_its_default() {
    // `I.super.f()` / Kotlin `super<I>.f()`: invokespecial with an
    // InterfaceMethodref resolves on the CP class, i.e. the interface.
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("C", OBJ, &["I"], 0x0001, Some(2)),
        call_f(0xB7, 0x0B, "I"),
    ];
    assert_eq!(run_multi(&classes, 2, &[]).unwrap(), Some(Value::Int(1)));
}

#[test]
fn invokevirtual_on_the_class_reaches_an_inherited_default() {
    let classes = [
        iface_f("I", &[], Some(1)),
        class_f("C", OBJ, &["I"], 0x0001, None),
        call_f(0xB6, 0x0A, "C"),
    ];
    assert_eq!(run_multi(&classes, 2, &[]).unwrap(), Some(Value::Int(1)));
}

#[test]
fn abstract_interface_declaration_is_not_a_default() {
    let classes = [
        iface_f("I", &[], None),
        class_f("C", OBJ, &["I"], 0x0001, None),
    ];
    assert_eq!(run_with(&classes), Err(JvmError::NoSuchMethod));
}

// ── append(Object) / valueOf(Object) trampoline ───────────────────────────

/// Class `name` whose Java `toString()` returns the literal `s`.
fn class_with_to_string(name: &str, s: &str) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let obj = a.class(OBJ);
    let lit = a.string(s);
    let code = [0x12, lo(lit), 0xB0]; // ldc; areturn
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0001,
            name: m::toString,
            desc: d::__String,
            max_stack: 1,
            max_locals: 1,
            code: &code,
            exc: &[],
        }],
    )
}

/// `m()I`: `new StringBuilder("x="); <push arg>; append(Object);
/// toString(); String.<tail>()I; ireturn`, run with `extra` loaded.
fn append_then(
    extra: &[&'static [u8]],
    tail: &str,
    push_arg: impl FnOnce(&mut Asm) -> Vec<u8>,
) -> i32 {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let sb = a.class(c::java_lang_StringBuilder);
    let seed = a.string("x=");
    let init = a.methodref(0x0A, sb, "<init>", d::String__V);
    let append = a.methodref(0x0A, sb, m::append, d::Object__StringBuilder);
    let to_s = a.methodref(0x0A, sb, m::toString, d::__String);
    let string = a.class(c::java_lang_String);
    let tail = a.methodref(0x0A, string, tail, "()I");
    let arg = push_arg(&mut a);
    let mut code = vec![
        0xBB,
        hi(sb),
        lo(sb),
        0x59,
        0x12,
        lo(seed),
        0xB7,
        hi(init),
        lo(init),
    ];
    code.extend(arg);
    code.extend_from_slice(&[
        0xB6,
        hi(append),
        lo(append),
        0xB6,
        hi(to_s),
        lo(to_s),
        0xB6,
        hi(tail),
        lo(tail),
        0xAC,
    ]);
    let t = a.finish(0x0001, this, obj, &[], Some((4, &code, &[])));
    let mut classes: Vec<&'static [u8]> = extra.to_vec();
    classes.push(t);
    match run_multi(&classes, classes.len() - 1, &[]).unwrap() {
        Some(Value::Int(v)) => v,
        other => panic!("expected int, got {other:?}"),
    }
}

fn push_new(name: &'static str) -> impl FnOnce(&mut Asm) -> Vec<u8> {
    move |a| {
        let c = a.class(name);
        vec![0xBB, hi(c), lo(c)]
    }
}

fn push_boxed_12345(a: &mut Asm) -> Vec<u8> {
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    vec![0x11, 0x30, 0x39, 0xB8, hi(value_of), lo(value_of)] // sipush 12345
}

#[test]
fn append_object_runs_a_java_to_string() {
    // The receiver survives the frame push + re-execution: the seeded
    // prefix is still there and the override's text follows it.
    let p = class_with_to_string("P", "hey");
    assert_eq!(
        append_then(&[p], m::hashCode, push_new("P")),
        jhash("x=hey")
    );
}

#[test]
fn append_object_formats_boxed_null_and_identity() {
    assert_eq!(
        append_then(&[], m::hashCode, push_boxed_12345),
        jhash("x=12345")
    );
    assert_eq!(
        append_then(&[], m::hashCode, |_| vec![0x01]),
        jhash("x=null")
    );
    // No override: the identity `P2@hhhh` form (7 chars) is appended.
    let p2 = class_f("P2", OBJ, &[], 0x0001, None);
    assert_eq!(append_then(&[p2], m::length, push_new("P2")), 2 + 7);
}

/// `m()I`: `<push arg>; String.valueOf(Object); String.hashCode(); ireturn`.
fn value_of_hash(extra: &[&'static [u8]], push_arg: impl FnOnce(&mut Asm) -> Vec<u8>) -> i32 {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let string = a.class(c::java_lang_String);
    let value_of = a.methodref(0x0A, string, m::valueOf, d::Object__String);
    let hash = a.methodref(0x0A, string, m::hashCode, "()I");
    let mut code = push_arg(&mut a);
    code.extend_from_slice(&[
        0xB8,
        hi(value_of),
        lo(value_of),
        0xB6,
        hi(hash),
        lo(hash),
        0xAC,
    ]);
    let t = a.finish(0x0001, this, obj, &[], Some((2, &code, &[])));
    let mut classes: Vec<&'static [u8]> = extra.to_vec();
    classes.push(t);
    match run_multi(&classes, classes.len() - 1, &[]).unwrap() {
        Some(Value::Int(v)) => v,
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn value_of_object_runs_to_string_and_passes_strings_through() {
    let p = class_with_to_string("P", "hey");
    assert_eq!(value_of_hash(&[p], push_new("P")), jhash("hey"));
    assert_eq!(value_of_hash(&[], push_boxed_12345), jhash("12345"));
    assert_eq!(value_of_hash(&[], |_| vec![0x01]), jhash("null"));
    assert_eq!(
        value_of_hash(&[], |a| {
            let s = a.string("ab");
            vec![0x12, lo(s)]
        }),
        jhash("ab")
    );
}

// ── Locale ────────────────────────────────────────────────────────────────

#[test]
fn locale_root_reads_null_and_is_ignored_by_to_upper_case() {
    // `getstatic` on a class with no class file reads the unset static as
    // null; `toUpperCase(Locale)` ignores its argument.
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let s = a.string("ab");
    let locale = a.class(c::java_util_Locale);
    let root = a.fieldref(locale, m::ROOT, d::t_Locale);
    let string = a.class(c::java_lang_String);
    let upper = a.methodref(0x0A, string, m::toUpperCase, d::Locale__String);
    let hash = a.methodref(0x0A, string, m::hashCode, "()I");
    let code = [
        0x12,
        lo(s),
        0xB2,
        hi(root),
        lo(root),
        0xB6,
        hi(upper),
        lo(upper),
        0xB6,
        hi(hash),
        lo(hash),
        0xAC,
    ];
    let t = a.finish(0x0001, this, obj, &[], Some((2, &code, &[])));
    assert_eq!(run(t).unwrap(), Some(Value::Int(jhash("AB"))));
}

// ── entrySet / LinkedHash* / toArray end to end ───────────────────────────

/// `<new map>; dup; <init>; astore_0; aload_0; ldc "k"; Integer.valueOf(7);
/// put; pop` for `map_class`, then `rest` (which may use `aload_0`).
fn map_with_k7(map_class: &str, build_rest: impl FnOnce(&mut Asm, u16) -> Vec<u8>) -> i32 {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let map = a.class(map_class);
    let init = a.methodref(0x0A, map, "<init>", "()V");
    let k = a.string("k");
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let put = a.methodref(0x0A, map, m::put, d::Object_Object__Object);
    let mut code = vec![
        0xBB,
        hi(map),
        lo(map),
        0x59,
        0xB7,
        hi(init),
        lo(init),
        0x4B, // astore_0
        0x2A, // aload_0
        0x12,
        lo(k),
        0x10,
        7, // bipush 7
        0xB8,
        hi(value_of),
        lo(value_of),
        0xB6,
        hi(put),
        lo(put),
        0x57, // pop
    ];
    code.extend(build_rest(&mut a, map));
    code.push(0xAC);
    let t = a.finish(0x0001, this, obj, &[], Some((5, &code, &[])));
    match run(t).unwrap() {
        Some(Value::Int(v)) => v,
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn entry_set_iteration_yields_map_entries() {
    // key.hashCode() + value.intValue() = 'k' + 7.
    let got = map_with_k7(c::java_util_HashMap, |a, map| {
        let entry_set = a.methodref(0x0A, map, m::entrySet, d::__Set);
        let set = a.class(c::java_util_Set);
        let iterator = a.methodref(0x0B, set, m::iterator, d::__Iterator);
        let iter = a.class(c::java_util_Iterator);
        let next = a.methodref(0x0B, iter, m::next, d::__Object);
        let entry = a.class(c::java_util_Map_Entry);
        let get_key = a.methodref(0x0B, entry, m::getKey, d::__Object);
        let get_value = a.methodref(0x0B, entry, m::getValue, d::__Object);
        let string = a.class(c::java_lang_String);
        let hash = a.methodref(0x0A, string, m::hashCode, "()I");
        let integer = a.class(c::java_lang_Integer);
        let int_value = a.methodref(0x0A, integer, m::intValue, "()I");
        vec![
            0x2A, // aload_0
            0xB6,
            hi(entry_set),
            lo(entry_set),
            0xB9,
            hi(iterator),
            lo(iterator),
            1,
            0,
            0xB9,
            hi(next),
            lo(next),
            1,
            0,
            0xC0,
            hi(entry),
            lo(entry), // checkcast Map$Entry
            0x59,      // dup
            0xB9,
            hi(get_key),
            lo(get_key),
            1,
            0,
            0xC0,
            hi(string),
            lo(string),
            0xB6,
            hi(hash),
            lo(hash),
            0x5F, // swap
            0xB9,
            hi(get_value),
            lo(get_value),
            1,
            0,
            0xC0,
            hi(integer),
            lo(integer),
            0xB6,
            hi(int_value),
            lo(int_value),
            0x60, // iadd
        ]
    });
    assert_eq!(got, i32::from(b'k') + 7);
}

#[test]
fn linked_hash_map_is_a_map_backed_by_the_hash_map_dispatcher() {
    // get("k").intValue() + (map instanceof Map) = 7 + 1.
    let got = map_with_k7(c::java_util_LinkedHashMap, |a, map| {
        let k = a.string("k");
        let get = a.methodref(0x0A, map, m::get, d::Object__Object);
        let integer = a.class(c::java_lang_Integer);
        let int_value = a.methodref(0x0A, integer, m::intValue, "()I");
        let map_iface = a.class(c::java_util_Map);
        vec![
            0x2A,
            0x12,
            lo(k),
            0xB6,
            hi(get),
            lo(get),
            0xC0,
            hi(integer),
            lo(integer),
            0xB6,
            hi(int_value),
            lo(int_value),
            0x2A,
            0xC1,
            hi(map_iface),
            lo(map_iface), // instanceof Map
            0x60,
        ]
    });
    assert_eq!(got, 8);
}

#[test]
fn aliases_sit_in_the_builtin_hierarchy() {
    let is = |c, t| helpers::is_instance_of(&[], c, t);
    assert!(is(c::java_util_LinkedHashMap, c::java_util_HashMap));
    assert!(is(c::java_util_LinkedHashMap, c::java_util_Map));
    assert!(is(c::java_util_LinkedHashSet, c::java_util_Set));
    assert!(is(c::java_util_LinkedHashSet, c::java_lang_Iterable));
    assert!(is(c::java_util_HashMap_EntrySet, c::java_util_Set));
    assert!(is(c::java_util_HashMap_EntrySet, c::java_lang_Iterable));
    assert!(is(c::java_util_Map_Entry, c::java_util_Map_Entry));
    assert!(!is(c::java_util_LinkedHashMap, c::java_util_Set));
}

#[test]
fn to_array_returns_a_fresh_object_array() {
    // list ["a"].toArray(new String[0]) → length 1 + "a".length() 1.
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let list = a.class(c::java_util_ArrayList);
    let init = a.methodref(0x0A, list, "<init>", "()V");
    let s = a.string("a");
    let add = a.methodref(0x0A, list, m::add, d::Object__Z);
    let string = a.class(c::java_lang_String);
    let to_array = a.methodref(0x0A, list, m::toArray, d::aObject__aObject);
    let string_arr = a.class(d::t_aString);
    let length = a.methodref(0x0A, string, m::length, "()I");
    let code = [
        0xBB,
        hi(list),
        lo(list),
        0x59,
        0xB7,
        hi(init),
        lo(init),
        0x59,
        0x12,
        lo(s),
        0xB6,
        hi(add),
        lo(add),
        0x57,
        0x03, // iconst_0
        0xBD,
        hi(string),
        lo(string), // anewarray String
        0xB6,
        hi(to_array),
        lo(to_array),
        0xC0,
        hi(string_arr),
        lo(string_arr),
        0x59,
        0xBE, // arraylength
        0x5F, // swap
        0x03,
        0x32, // aaload
        0xB6,
        hi(length),
        lo(length),
        0x60,
        0xAC,
    ];
    let t = a.finish(0x0001, this, obj, &[], Some((4, &code, &[])));
    assert_eq!(run(t).unwrap(), Some(Value::Int(2)));
}

// ── Object.clone() on an array receiver ───────────────────────────────────

#[test]
fn object_clone_on_an_array_dispatches_by_array_class() {
    // kotlinc's enum `values()`: `getstatic $VALUES; invokevirtual
    // java/lang/Object.clone()`. `iconst_2; newarray int; invokevirtual
    // Object.clone(); checkcast [I; arraylength` → 2.
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJ);
    let clone = a.methodref(0x0A, obj, m::clone, d::__Object);
    let int_arr = a.class("[I");
    let code = [
        0x05, // iconst_2
        0xBC,
        10, // newarray int
        0xB6,
        hi(clone),
        lo(clone),
        0xC0,
        hi(int_arr),
        lo(int_arr),
        0xBE, // arraylength
        0xAC,
    ];
    let t = a.finish(0x0001, this, obj, &[], Some((2, &code, &[])));
    assert_eq!(run(t).unwrap(), Some(Value::Int(2)));
}
