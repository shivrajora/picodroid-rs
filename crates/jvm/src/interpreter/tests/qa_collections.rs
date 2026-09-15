// SPDX-License-Identifier: GPL-3.0-only
//! Regression guards for the collection, enum and builder fixes of the
//! 2026-09-13/14 QA round (`docs/qa-2026-09-13.md`).
//!
//! These need the interpreter, not a bare native dispatch: a user
//! `equals(Object)` runs through the native→Java upcall, `Enum.valueOf`
//! reads the enum's static fields, and `StringBuilder.append(CharSequence)`
//! resolves its argument at the invoke seam.
use super::asm::{Asm, Method};
use super::*;
use crate::array_heap::ArrayHeap;
use crate::class_objects::ClassObjectCache;
use crate::names::spelled;
use crate::names::{c, d, m};
use alloc::vec;

const OBJ: &str = c::java_lang_Object;

fn hi(i: u16) -> u8 {
    (i >> 8) as u8
}
fn lo(i: u16) -> u8 {
    i as u8
}

/// A heap and class set kept alive across `execute`, so a test can seed the
/// heap by hand and read it back afterwards.
struct Harness {
    classes: Vec<ClassFile>,
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    statics: StaticFieldStore,
    gc_state: GcState,
    class_objects: ClassObjectCache,
}

impl Harness {
    fn new(classes_data: &[&'static [u8]]) -> Self {
        let mut classes: Vec<ClassFile> = Vec::new();
        for &data in classes_data {
            classes.push(ClassFile::parse(spelled(data)).expect("parse failed"));
        }
        Self {
            classes,
            strings: StringTable::new(),
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
            statics: StaticFieldStore::new(),
            gc_state: GcState::new(),
            class_objects: ClassObjectCache::new(),
        }
    }

    fn execute(&mut self, class_idx: usize, args: &[Value]) -> Result<Option<Value>, JvmError> {
        let mut handler = NoopHandler;
        execute(
            &self.classes,
            &mut self.strings,
            &mut self.objects,
            &mut self.arrays,
            &mut self.statics,
            &mut self.gc_state,
            &mut self.class_objects,
            &mut handler,
            class_idx,
            0,
            args,
        )
    }

    /// A builtin `HashMap`, as its native `<init>` builds one: a plain object
    /// whose field 0 holds a `map_store` buffer index.
    fn new_map(&mut self) -> (Value, u16) {
        let obj = self.objects.alloc(c::java_util_HashMap).expect("alloc");
        let buf = self.objects.map_alloc().expect("map_alloc");
        self.objects.set_field(obj, 0, Value::Int(buf as i32));
        (Value::ObjectRef(obj), buf)
    }

    fn new_list(&mut self) -> (Value, u16) {
        let obj = self.objects.alloc(c::java_util_ArrayList).expect("alloc");
        let buf = self.objects.list_alloc().expect("list_alloc");
        self.objects.set_field(obj, 0, Value::Int(buf as i32));
        (Value::ObjectRef(obj), buf)
    }

    /// A `Key` instance carrying `v` in field 0.
    fn new_key(&mut self, v: i32) -> Value {
        let idx = self.objects.alloc("Key").expect("alloc");
        self.objects.set_field(idx, 0, Value::Int(v));
        Value::ObjectRef(idx)
    }
}

// ── J-fix 0d8c4247: a user equals(Object) decides collection membership ──

/// `Key`, the class every Java tutorial writes: one int field and an
/// `equals(Object)` that compares it. `hashCode` is deliberately absent —
/// the buffers are linear, so the fix does not consult one.
fn key_class() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Key");
    let obj = a.class(OBJ);
    a.field("v", "I");
    let v = a.fieldref(this, "v", "I");
    // return ((Key) o).v == this.v;
    let code = vec![
        0x2A, // aload_0 — this
        0xB4,
        hi(v),
        lo(v), // getfield this.v
        0x2B,  // aload_1 — the other object
        0xC0,
        hi(this),
        lo(this), // checkcast Key
        0xB4,
        hi(v),
        lo(v), // getfield other.v
        0xA0,
        0x00,
        0x07, // if_icmpne +7 -> iconst_0
        0x04, // iconst_1
        0xAC, // ireturn
        0x03, // iconst_0
        0xAC, // ireturn
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0001,
            name: m::equals,
            desc: d::Object__Z,
            max_stack: 2,
            max_locals: 2,
            code: &code,
            exc: &[],
        }],
    )
}

/// `static int m(HashMap map, Key a, Key b)`:
/// `map.put(a, Integer) ; return map.containsKey(b) ? 1 : 0`.
fn map_contains_caller() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let map = a.class(c::java_util_HashMap);
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let put = a.methodref(0x0A, map, m::put, d::Object_Object__Object);
    let contains = a.methodref(0x0A, map, m::containsKey, d::Object__Z);
    let code = vec![
        0x2A, // aload_0 — the map
        0x2B, // aload_1 — key a
        0x07, // iconst_4
        0xB8,
        hi(value_of),
        lo(value_of), // Integer.valueOf(4)
        0xB6,
        hi(put),
        lo(put), // map.put(a, 4)
        0x57,    // pop — the previous value
        0x2A,    // aload_0
        0x2C,    // aload_2 — key b
        0xB6,
        hi(contains),
        lo(contains), // map.containsKey(b)
        0xAC,         // ireturn
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(Ljava/util/HashMap;LKey;LKey;)I",
            max_stack: 4,
            max_locals: 3,
            code: &code,
            exc: &[],
        }],
    )
}

#[test]
fn hashmap_finds_an_entry_through_a_user_equals() {
    let mut h = Harness::new(&[map_contains_caller(), key_class()]);
    let (map, _buf) = h.new_map();
    let stored = h.new_key(7);
    let probe = h.new_key(7); // a different object, an equal key
    assert_eq!(
        h.execute(0, &[map, stored, probe]),
        Ok(Some(Value::Int(1))),
        "containsKey must call Key.equals, not compare identities"
    );
}

#[test]
fn hashmap_still_misses_when_the_user_equals_says_no() {
    let mut h = Harness::new(&[map_contains_caller(), key_class()]);
    let (map, _buf) = h.new_map();
    let stored = h.new_key(7);
    let probe = h.new_key(8);
    assert_eq!(h.execute(0, &[map, stored, probe]), Ok(Some(Value::Int(0))));
}

#[test]
fn hashmap_put_replaces_an_equal_key_instead_of_duplicating_it() {
    let mut h = Harness::new(&[map_put_twice_caller(), key_class()]);
    let (map, buf) = h.new_map();
    let a = h.new_key(3);
    let b = h.new_key(3);
    h.execute(0, &[map, a, b]).expect("put failed");
    assert_eq!(
        h.objects.map_len(buf),
        1,
        "two equal keys must be one entry"
    );
}

/// `static void m(HashMap map, Key a, Key b)`: puts under both keys.
fn map_put_twice_caller() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let map = a.class(c::java_util_HashMap);
    let integer = a.class(c::java_lang_Integer);
    let value_of = a.methodref(0x0A, integer, m::valueOf, d::I__Integer);
    let put = a.methodref(0x0A, map, m::put, d::Object_Object__Object);
    let code = vec![
        0x2A,
        0x2B,
        0x04, // aload_0, aload_1, iconst_1
        0xB8,
        hi(value_of),
        lo(value_of),
        0xB6,
        hi(put),
        lo(put),
        0x57, // put, pop
        0x2A,
        0x2C,
        0x05, // aload_0, aload_2, iconst_2
        0xB8,
        hi(value_of),
        lo(value_of),
        0xB6,
        hi(put),
        lo(put),
        0x57, // put, pop
        0xB1, // return
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(Ljava/util/HashMap;LKey;LKey;)V",
            max_stack: 4,
            max_locals: 3,
            code: &code,
            exc: &[],
        }],
    )
}

/// `static int m(ArrayList list, Key a, Key b)`:
/// `list.add(a) ; return list.contains(b) ? 1 : 0`.
fn list_contains_caller() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let list = a.class(c::java_util_ArrayList);
    let add = a.methodref(0x0A, list, m::add, d::Object__Z);
    let contains = a.methodref(0x0A, list, m::contains, d::Object__Z);
    let code = vec![
        0x2A,
        0x2B, // aload_0, aload_1
        0xB6,
        hi(add),
        lo(add),
        0x57, // list.add(a), pop
        0x2A,
        0x2C, // aload_0, aload_2
        0xB6,
        hi(contains),
        lo(contains),
        0xAC, // ireturn
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: "(Ljava/util/ArrayList;LKey;LKey;)I",
            max_stack: 3,
            max_locals: 3,
            code: &code,
            exc: &[],
        }],
    )
}

#[test]
fn arraylist_contains_honours_a_user_equals() {
    let mut h = Harness::new(&[list_contains_caller(), key_class()]);
    let (list, _buf) = h.new_list();
    let stored = h.new_key(5);
    let probe = h.new_key(5);
    assert_eq!(
        h.execute(0, &[list, stored, probe]),
        Ok(Some(Value::Int(1)))
    );

    let mut h = Harness::new(&[list_contains_caller(), key_class()]);
    let (list, _buf) = h.new_list();
    let stored = h.new_key(5);
    let probe = h.new_key(6);
    assert_eq!(
        h.execute(0, &[list, stored, probe]),
        Ok(Some(Value::Int(0)))
    );
}

/// A class with no `equals` of its own keeps identity comparison.
fn plain_key_class() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Key");
    let obj = a.class(OBJ);
    a.field("v", "I");
    a.finish_methods(0x0001, this, obj, &[], &[])
}

#[test]
fn a_class_without_equals_still_compares_by_identity() {
    let mut h = Harness::new(&[map_contains_caller(), plain_key_class()]);
    let (map, _buf) = h.new_map();
    let stored = h.new_key(7);
    let probe = h.new_key(7);
    assert_eq!(
        h.execute(0, &[map, stored, probe]),
        Ok(Some(Value::Int(0))),
        "no equals override means identity, as before the fix"
    );
}

// ── J-fix 711226ef: Enum.valueOf(Class, String) ─────────────────────────

/// `Color`, an enum as javac emits one: two constants as `public static`
/// fields of its own type, built in `<clinit>`, and a `valueOf(String)` that
/// delegates to `Enum.valueOf(Color.class, name)`.
fn color_enum_class() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Color");
    let sup = a.class(c::java_lang_Enum);
    a.static_field("RED", "LColor;");
    a.static_field("BLUE", "LColor;");
    let red = a.fieldref(this, "RED", "LColor;");
    let blue = a.fieldref(this, "BLUE", "LColor;");
    let enum_init = a.methodref(0x0A, sup, "<init>", d::String_I__V);
    let enum_value_of = a.methodref(0x0A, sup, m::valueOf, d::Class_String__Enum);
    let red_s = a.string("RED");
    let blue_s = a.string("BLUE");

    let clinit = vec![
        0xBB,
        hi(this),
        lo(this), // new Color
        0x59,     // dup
        0x13,
        hi(red_s),
        lo(red_s), // ldc_w "RED"
        0x03,      // iconst_0
        0xB7,
        hi(enum_init),
        lo(enum_init), // invokespecial Enum.<init>
        0xB3,
        hi(red),
        lo(red), // putstatic Color.RED
        0xBB,
        hi(this),
        lo(this), // new Color
        0x59,     // dup
        0x13,
        hi(blue_s),
        lo(blue_s), // ldc_w "BLUE"
        0x04,       // iconst_1
        0xB7,
        hi(enum_init),
        lo(enum_init), // invokespecial Enum.<init>
        0xB3,
        hi(blue),
        lo(blue), // putstatic Color.BLUE
        0xB1,     // return
    ];
    // static Color valueOf(String n) { return (Color) Enum.valueOf(Color.class, n); }
    let value_of = vec![
        0x13,
        hi(this),
        lo(this), // ldc_w Color.class
        0x2A,     // aload_0 — the name
        0xB8,
        hi(enum_value_of),
        lo(enum_value_of), // invokestatic Enum.valueOf
        0xC0,
        hi(this),
        lo(this), // checkcast Color
        0xB0,     // areturn
    ];
    a.finish_methods(
        0x4001, // public | enum
        this,
        sup,
        &[],
        &[
            Method {
                access: 0x0009,
                name: m::valueOf,
                desc: "(Ljava/lang/String;)LColor;",
                max_stack: 2,
                max_locals: 1,
                code: &value_of,
                exc: &[],
            },
            Method {
                access: 0x0008,
                name: "<clinit>",
                desc: "()V",
                max_stack: 4,
                max_locals: 0,
                code: &clinit,
                exc: &[],
            },
        ],
    )
}

/// `static int m(String name)` = `Color.valueOf(name).ordinal()`.
fn enum_value_of_caller() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let color = a.class("Color");
    let enum_cls = a.class(c::java_lang_Enum);
    let value_of = a.methodref(0x0A, color, m::valueOf, "(Ljava/lang/String;)LColor;");
    let ordinal = a.methodref(0x0A, enum_cls, m::ordinal, "()I");
    let code = vec![
        0x2A, // aload_0
        0xB8,
        hi(value_of),
        lo(value_of), // Color.valueOf(name)
        0xB6,
        hi(ordinal),
        lo(ordinal), // .ordinal()
        0xAC,        // ireturn
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: d::String__I,
            max_stack: 2,
            max_locals: 1,
            code: &code,
            exc: &[],
        }],
    )
}

#[test]
fn enum_value_of_returns_the_named_constant() {
    for (name, want) in [("RED", 0), ("BLUE", 1)] {
        let mut h = Harness::new(&[enum_value_of_caller(), color_enum_class()]);
        let n = Value::Reference(h.strings.intern(name.as_bytes()).unwrap());
        assert_eq!(
            h.execute(0, &[n]),
            Ok(Some(Value::Int(want))),
            "Color.valueOf({name:?})"
        );
    }
}

#[test]
fn enum_value_of_throws_illegal_argument_for_an_unknown_name() {
    let mut h = Harness::new(&[enum_value_of_caller(), color_enum_class()]);
    let n = Value::Reference(h.strings.intern(b"GREEN").unwrap());
    match h.execute(0, &[n]) {
        Err(JvmError::UncaughtException {
            exception_class, ..
        }) => assert_eq!(exception_class, c::java_lang_IllegalArgumentException),
        other => panic!("expected IllegalArgumentException, got {other:?}"),
    }
}

// ── J-fix aa0b1e38: sb.append(otherBuilder) appends its content ─────────

/// `static String m()`:
/// `a = new SB("ab"); b = new SB("xy"); a.append((CharSequence) b); return a.toString();`
///
/// `append(CharSequence)` is the overload javac picks for a StringBuilder
/// argument; it was not one of the descriptors stringified before the native
/// arm, so the argument appended nothing.
fn append_builder_caller(self_append: bool) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("Caller");
    let obj = a.class(OBJ);
    let sb = a.class(c::java_lang_StringBuilder);
    let init = a.methodref(0x0A, sb, "<init>", "()V");
    let app_str = a.methodref(0x0A, sb, m::append, d::String__StringBuilder);
    let app_cs = a.methodref(0x0A, sb, m::append, d::CharSequence__StringBuilder);
    let to_string = a.methodref(0x0A, sb, m::toString, d::__String);
    let ab = a.string("ab");
    let xy = a.string("xy");

    let mut code = vec![
        0xBB,
        hi(sb),
        lo(sb),
        0x59, // new StringBuilder, dup
        0xB7,
        hi(init),
        lo(init), // <init>
        0x3A,
        0x00, // astore 0
        0x19,
        0x00, // aload 0
        0x13,
        hi(ab),
        lo(ab), // ldc_w "ab"
        0xB6,
        hi(app_str),
        lo(app_str),
        0x57, // append("ab"), pop
    ];
    if self_append {
        // a.append(a) -- the self-append, which must double the content.
        code.extend_from_slice(&[0x19, 0x00, 0x19, 0x00]);
    } else {
        code.extend_from_slice(&[
            0xBB,
            hi(sb),
            lo(sb),
            0x59, // new StringBuilder, dup
            0xB7,
            hi(init),
            lo(init), // <init>
            0x3A,
            0x01, // astore 1
            0x19,
            0x01, // aload 1
            0x13,
            hi(xy),
            lo(xy), // ldc_w "xy"
            0xB6,
            hi(app_str),
            lo(app_str),
            0x57, // append("xy"), pop
            0x19,
            0x00,
            0x19,
            0x01, // aload 0, aload 1
        ]);
    }
    code.extend_from_slice(&[
        0xB6,
        hi(app_cs),
        lo(app_cs),
        0x57, // append((CharSequence) b), pop
        0x19,
        0x00, // aload 0
        0xB6,
        hi(to_string),
        lo(to_string), // toString()
        0xB0,          // areturn
    ]);
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[Method {
            access: 0x0009,
            name: "m",
            desc: d::__String,
            max_stack: 4,
            max_locals: 2,
            code: &code,
            exc: &[],
        }],
    )
}

#[test]
fn string_builder_append_char_sequence_appends_a_builders_content() {
    let mut h = Harness::new(&[append_builder_caller(false)]);
    let r = h.execute(0, &[]).expect("append failed");
    let Some(Value::Reference(idx)) = r else {
        panic!("expected a String, got {r:?}");
    };
    assert_eq!(h.strings.resolve(idx), Some("abxy"));
}

#[test]
fn string_builder_self_append_doubles_its_content() {
    let mut h = Harness::new(&[append_builder_caller(true)]);
    let r = h.execute(0, &[]).expect("append failed");
    let Some(Value::Reference(idx)) = r else {
        panic!("expected a String, got {r:?}");
    };
    assert_eq!(h.strings.resolve(idx), Some("abab"));
}
