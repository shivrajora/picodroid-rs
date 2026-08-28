// SPDX-License-Identifier: GPL-3.0-only
//! Runtime type information: the builtin class-hierarchy tables, transitive
//! superinterface walking, `checkcast`/`instanceof` on strings and arrays,
//! the catchable `ClassCastException`, and String receivers dispatching as
//! `java/lang/String` whatever the constant pool declared.
use super::*;
use crate::native::BUILTIN_CLASS_NAMES;
use alloc::vec;

// ── A small class-file assembler ──────────────────────────────────────────
//
// Builds a constant pool entry by entry (1-based indices, as the JVM sees
// them) and emits either a method-less class/interface or a class with the
// single static `m()I` that `run`/`run_multi` execute.

struct Asm {
    cp: Vec<Vec<u8>>,
}

impl Asm {
    fn new() -> Self {
        Self { cp: Vec::new() }
    }

    fn push(&mut self, e: Vec<u8>) -> u16 {
        self.cp.push(e);
        self.cp.len() as u16
    }

    fn utf8(&mut self, s: &str) -> u16 {
        let mut e = vec![0x01];
        e.extend_from_slice(&(s.len() as u16).to_be_bytes());
        e.extend_from_slice(s.as_bytes());
        self.push(e)
    }

    fn class(&mut self, name: &str) -> u16 {
        let u = self.utf8(name);
        self.push(vec![0x07, (u >> 8) as u8, u as u8])
    }

    fn string(&mut self, s: &str) -> u16 {
        let u = self.utf8(s);
        self.push(vec![0x08, (u >> 8) as u8, u as u8])
    }

    /// Methodref (tag 10) or InterfaceMethodref (tag 11).
    fn methodref(&mut self, tag: u8, class: u16, name: &str, desc: &str) -> u16 {
        let n = self.utf8(name);
        let d = self.utf8(desc);
        let nat = self.push(vec![0x0C, (n >> 8) as u8, n as u8, (d >> 8) as u8, d as u8]);
        self.push(vec![
            tag,
            (class >> 8) as u8,
            class as u8,
            (nat >> 8) as u8,
            nat as u8,
        ])
    }

    /// Emit the class file. `method` is `(max_stack, code, exception table
    /// rows as [start, end, handler, catch_type])` for a static `m()I`.
    fn finish(
        &mut self,
        access: u16,
        this: u16,
        sup: u16,
        ifaces: &[u16],
        method: Option<(u16, &[u8], &[[u16; 4]])>,
    ) -> &'static [u8] {
        let names = method.map(|_| (self.utf8("m"), self.utf8("()I"), self.utf8("Code")));
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34];
        out.extend_from_slice(&(self.cp.len() as u16 + 1).to_be_bytes());
        for e in &self.cp {
            out.extend_from_slice(e);
        }
        out.extend_from_slice(&access.to_be_bytes());
        out.extend_from_slice(&this.to_be_bytes());
        out.extend_from_slice(&sup.to_be_bytes());
        out.extend_from_slice(&(ifaces.len() as u16).to_be_bytes());
        for i in ifaces {
            out.extend_from_slice(&i.to_be_bytes());
        }
        out.extend_from_slice(&[0x00, 0x00]); // fields
        match (method, names) {
            (Some((max_stack, code, exc)), Some((m, desc, code_name))) => {
                out.extend_from_slice(&[0x00, 0x01]); // methods_count
                out.extend_from_slice(&0x0008u16.to_be_bytes()); // static
                out.extend_from_slice(&m.to_be_bytes());
                out.extend_from_slice(&desc.to_be_bytes());
                out.extend_from_slice(&[0x00, 0x01]); // attrs
                out.extend_from_slice(&code_name.to_be_bytes());
                let attr_len = 2 + 2 + 4 + code.len() + 2 + 8 * exc.len() + 2;
                out.extend_from_slice(&(attr_len as u32).to_be_bytes());
                out.extend_from_slice(&max_stack.to_be_bytes());
                out.extend_from_slice(&1u16.to_be_bytes()); // max_locals
                out.extend_from_slice(&(code.len() as u32).to_be_bytes());
                out.extend_from_slice(code);
                out.extend_from_slice(&(exc.len() as u16).to_be_bytes());
                for row in exc {
                    for v in row {
                        out.extend_from_slice(&v.to_be_bytes());
                    }
                }
                out.extend_from_slice(&[0x00, 0x00]); // code attrs
            }
            _ => out.extend_from_slice(&[0x00, 0x00]),
        }
        out.extend_from_slice(&[0x00, 0x00]); // class attrs
        alloc::boxed::Box::leak(out.into_boxed_slice())
    }
}

const ACC_INTERFACE: u16 = 0x0601; // PUBLIC | INTERFACE | ABSTRACT

fn iface(name: &str, supers: &[&str]) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class(name);
    let obj = a.class("java/lang/Object");
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
        .map(|d| ClassFile::parse(d).expect("parse"))
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
        "java/util/List",
        "java/util/Collection",
        "java/lang/Iterable",
        "java/lang/Object",
    ] {
        assert!(is("java/util/ArrayList", t), "ArrayList is a {t}");
    }
    assert!(!is("java/util/ArrayList", "java/util/Map"));
    assert!(!is("java/util/ArrayList", "java/util/Set"));
    assert!(is("java/util/HashMap", "java/util/Map"));
    assert!(is("java/util/HashMap$KeySet", "java/util/Set"));
    assert!(is("java/util/HashMap$Values", "java/lang/Iterable"));
    for t in [
        "java/lang/Number",
        "java/lang/Comparable",
        "java/lang/Object",
    ] {
        assert!(is("java/lang/Integer", t), "Integer is a {t}");
        assert!(is("java/lang/Float", t), "Float is a {t}");
    }
    assert!(!is("java/lang/Integer", "java/lang/Long"));
    assert!(!is("java/lang/Boolean", "java/lang/Number"));
    assert!(is("java/lang/String", "java/lang/CharSequence"));
    assert!(is("java/lang/String", "java/lang/Comparable"));
    assert!(is("java/lang/StringBuilder", "java/lang/CharSequence"));
    assert!(is(
        "java/util/NoSuchElementException",
        "java/lang/RuntimeException"
    ));
    assert!(is(
        "java/util/NoSuchElementException",
        "java/lang/Throwable"
    ));
    assert!(is(
        "java/lang/ClassCastException",
        "java/lang/RuntimeException"
    ));
}

/// `class Sub extends Base implements KList`, `interface KList extends
/// java/util/List`: the walk crosses a loaded interface class file and then
/// the classfile-less `List → Collection → Iterable` rows.
#[test]
fn superinterfaces_are_walked_transitively() {
    let classes = parse_all(&[
        iface("KList", &["java/util/List"]),
        plain_class("Base", "java/lang/Object", &[]),
        plain_class("Sub", "Base", &["KList"]),
    ]);
    let is = |rt: &str, t: &str| helpers::is_instance_of(&classes, rt, t);
    assert!(is("Sub", "KList"));
    assert!(is("Sub", "java/util/List"));
    assert!(is("Sub", "java/util/Collection"));
    assert!(is("Sub", "java/lang/Iterable"));
    assert!(is("Sub", "Base"));
    assert!(is("Sub", "java/lang/Object"));
    assert!(!is("Sub", "java/util/Map"));
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
            "java/lang/Object",
            &["Bottom", "kotlin/jvm/internal/markers/KMappedMarker"],
        ),
    ]);
    let is = |rt: &str, t: &str| helpers::is_instance_of(&classes, rt, t);
    assert!(is("Impl", "Top"));
    assert!(is("Impl", "Right"));
    assert!(is("Impl", "kotlin/jvm/internal/markers/KMappedMarker"));
    assert!(is("Impl", "java/lang/Object"));
    assert!(!is("Impl", "java/lang/Runnable"));
}

/// Hand-assembled cycles must terminate (valid class files cannot cycle).
#[test]
fn interface_cycle_terminates() {
    let classes = parse_all(&[
        iface("A", &["B"]),
        iface("B", &["A"]),
        plain_class("C", "java/lang/Object", &["A"]),
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
        "java/lang/String",
        "java/lang/CharSequence",
        "java/lang/Comparable",
        "java/lang/Object",
    ] {
        assert!(is(s, t), "String is a {t}");
    }
    assert!(!is(s, "java/util/ArrayList"));
    assert!(!is(s, "java/lang/Integer"));
    assert!(is(ints, "[I"));
    assert!(!is(ints, "[F"));
    assert!(!is(ints, "[Ljava/lang/Object;"));
    assert!(is(ints, "java/lang/Object"));
    assert!(is(ints, "java/lang/Cloneable"));
    assert!(!is(ints, "java/lang/String"));
    assert!(is(refs, "[Ljava/lang/Object;"));
    assert!(is(refs, "[Ljava/lang/String;")); // element class not recorded
    assert!(is(refs, "[[I"));
    assert!(!is(refs, "[I"));
    assert!(!is(Value::Null, "java/lang/Object"));
    assert!(!is(Value::Int(1), "java/lang/Integer"));
}

// ── End to end: checkcast / instanceof / ClassCastException ───────────────

/// `m()I`: `ldc "str"; checkcast <target>` inside a try region, then
/// `iconst_1; ireturn`; handler `pop; bipush 7; ireturn` catching `catch`.
fn cast_class(target: &str, catch: Option<&str>) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class("java/lang/Object");
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
        "java/lang/String",
        "java/lang/CharSequence",
        "java/lang/Object",
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
        "java/util/ArrayList",
        Some("java/lang/ClassCastException"),
    ));
    assert_eq!(r.unwrap(), Some(Value::Int(7)));
    // Superclass catches match too (builtin_super chain).
    let r = run(cast_class(
        "java/util/ArrayList",
        Some("java/lang/RuntimeException"),
    ));
    assert_eq!(r.unwrap(), Some(Value::Int(7)));
}

#[test]
fn uncaught_class_cast_exception_names_its_class() {
    let r = run(cast_class("java/util/ArrayList", None));
    match r {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, "java/lang/ClassCastException"),
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
    };
    let JvmError::Exception(e) = ex.class_cast_exception() else {
        panic!("expected an Exception");
    };
    assert_eq!(
        ex.objects.class_name(e),
        Some("java/lang/ClassCastException")
    );
}

/// `m()I`: `<load>; instanceof <target>; ireturn`.
fn instanceof_class(load: &[u8], target: &str) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class("java/lang/Object");
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
        run(instanceof_class(ldc_str, "java/lang/String")).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        run(instanceof_class(ldc_str, "java/lang/Comparable")).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        run(instanceof_class(ldc_str, "java/util/List")).unwrap(),
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
        run(instanceof_class(int_array, "java/lang/Object")).unwrap(),
        Some(Value::Int(1))
    );
    let null: &[u8] = &[0x01];
    assert_eq!(
        run(instanceof_class(null, "java/lang/Object")).unwrap(),
        Some(Value::Int(0))
    );
}

#[test]
fn checkcast_null_passes() {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class("java/lang/Object");
    let t = a.class("java/util/ArrayList");
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
    let obj = a.class("java/lang/Object");
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
        "java/lang/Comparable",
        "compareTo",
        "(Ljava/lang/Object;)I",
    );
    assert_eq!(run(cls).unwrap(), Some(Value::Int(-1)));
}

#[test]
fn char_sequence_length_on_a_string() {
    let cls = string_call(false, 0xB9, 0x0B, "java/lang/CharSequence", "length", "()I");
    assert_eq!(run(cls).unwrap(), Some(Value::Int(1)));
}

#[test]
fn object_hash_code_and_equals_on_a_string() {
    let cls = string_call(false, 0xB6, 0x0A, "java/lang/Object", "hashCode", "()I");
    assert_eq!(run(cls).unwrap(), Some(Value::Int(97)));
    let cls = string_call(
        true,
        0xB6,
        0x0A,
        "java/lang/Object",
        "equals",
        "(Ljava/lang/Object;)Z",
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
    let obj = a.class("java/lang/Object");
    let p = a.class("P");
    let m = a.methodref(0x0A, p, name, desc);
    let mut code = vec![0xBB, (p >> 8) as u8, p as u8, 0x59];
    if two_args {
        code.push(0x59);
    }
    code.extend_from_slice(&[0xB6, (m >> 8) as u8, m as u8, 0xAC]);
    let t = a.finish(0x0001, this, obj, &[], Some((3, &code, &[])));
    vec![plain_class("P", "java/lang/Object", &[]), t]
}

#[test]
fn identity_equals_and_hash_code_on_a_plain_user_class() {
    let classes = identity_call("equals", "(Ljava/lang/Object;)Z", true);
    assert_eq!(run_multi(&classes, 1, &[]).unwrap(), Some(Value::Int(1)));
    let classes = identity_call("hashCode", "()I", false);
    assert!(matches!(
        run_multi(&classes, 1, &[]).unwrap(),
        Some(Value::Int(_))
    ));
}
