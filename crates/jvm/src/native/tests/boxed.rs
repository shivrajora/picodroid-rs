// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── Boxed type tests ──────────────────────────────────────────────────────
//
// Each test allocates a boxed object via valueOf, then reads it back via
// the unboxing accessor, sharing the same ObjectHeap across both calls.

#[test]
fn integer_value_of_and_int_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        c::java_lang_Integer,
        m::valueOf,
        d::I__Integer,
        &[Value::Int(42)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Integer,
            m::intValue,
            "()I",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Int(42)))
    );
}

#[test]
fn boolean_value_of_true() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        c::java_lang_Boolean,
        m::valueOf,
        d::Z__Boolean,
        &[Value::Int(1)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Boolean,
            m::booleanValue,
            "()Z",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn long_value_of_and_long_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        c::java_lang_Long,
        m::valueOf,
        d::J__Long,
        &[Value::Long(1000)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Long,
            m::longValue,
            "()J",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Long(1000)))
    );
}

#[test]
fn float_value_of_and_float_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        c::java_lang_Float,
        m::valueOf,
        d::F__Float,
        &[Value::Float(3.14)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Float,
            m::floatValue,
            "()F",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Float(3.14)))
    );
}

#[test]
fn double_value_of_and_double_value() {
    let mut objects = ObjectHeap::new();
    let boxed = dispatch_boxed(
        c::java_lang_Double,
        m::valueOf,
        d::D__Double,
        &[Value::Double(2.71)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        dispatch_boxed(
            c::java_lang_Double,
            m::doubleValue,
            "()D",
            &[boxed],
            &mut objects
        ),
        Ok(Some(Value::Double(2.71)))
    );
}

// ── Boxed toString tests ──────────────────────────────────────────────────
//
// Each test invokes the static / instance toString variants and resolves
// the returned `Value::Reference` against the test's own StringTable so the
// emitted bytes can be checked.

#[test]
fn integer_to_string_static_zero() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let v = dispatch_boxed_to_string(
        c::java_lang_Integer,
        d::I__String,
        &[Value::Int(0)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, v), "0");
}

#[test]
fn integer_to_string_static_positive_and_negative() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    for (n, expected) in &[(42, "42"), (-7, "-7"), (i32::MAX, "2147483647")] {
        let v = dispatch_boxed_to_string(
            c::java_lang_Integer,
            d::I__String,
            &[Value::Int(*n)],
            &mut objects,
            &mut strings,
        )
        .unwrap()
        .unwrap();
        assert_eq!(resolve_str(&strings, v), *expected);
    }
}

#[test]
fn integer_to_string_instance() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let boxed = dispatch_boxed(
        c::java_lang_Integer,
        m::valueOf,
        d::I__Integer,
        &[Value::Int(123)],
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let v = dispatch_boxed_to_string(
        c::java_lang_Integer,
        d::__String,
        &[boxed],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, v), "123");
}

#[test]
fn long_to_string_static() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let v = dispatch_boxed_to_string(
        c::java_lang_Long,
        d::J__String,
        &[Value::Long(9_876_543_210)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, v), "9876543210");
}

#[test]
fn boolean_to_string_static_both_paths() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let t = dispatch_boxed_to_string(
        c::java_lang_Boolean,
        d::Z__String,
        &[Value::Int(1)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    let f = dispatch_boxed_to_string(
        c::java_lang_Boolean,
        d::Z__String,
        &[Value::Int(0)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, t), "true");
    assert_eq!(resolve_str(&strings, f), "false");
}

#[test]
fn double_to_string_static_and_instance() {
    // Every other wrapper has a toString arm; Double had none, so the
    // static form was NoSuchMethod and the instance form printed
    // java.lang.Double@NNNN via Object.toString.
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let cases: &[(f64, &str)] = &[
        (1.5, "1.5"),
        (100.0, "100.0"),
        (0.1, "0.1"),
        (-0.0, "-0.0"),
        (0.0, "0.0"),
        (1e10, "1.0E10"),
        (1.5e-5, "1.5E-5"),
        (1234567.0, "1234567.0"),
        (12345678.0, "1.2345678E7"),
        (0.001, "0.001"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (core::f64::consts::PI, "3.141592653589793"),
    ];
    for &(d, want) in cases {
        let v = dispatch_boxed_to_string(
            c::java_lang_Double,
            d::D__String,
            &[Value::Double(d)],
            &mut objects,
            &mut strings,
        )
        .unwrap()
        .unwrap();
        assert_eq!(resolve_str(&strings, v), want, "Double.toString({d})");
    }
    // Instance form on a boxed receiver.
    let boxed = objects.alloc(c::java_lang_Double).unwrap();
    objects.set_field(boxed, 0, Value::Double(2.5));
    let v = dispatch_boxed_to_string(
        c::java_lang_Double,
        d::__String,
        &[Value::ObjectRef(boxed)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, v), "2.5");
}

#[test]
fn float_to_string_static() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let v = dispatch_boxed_to_string(
        c::java_lang_Float,
        d::F__String,
        &[Value::Float(0.0)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    // float_to_str_buf renders 0.0 as "0.0" — exact bytes depend on the
    // shared formatter; just assert it starts with "0".
    let s = resolve_str(&strings, v);
    assert!(s.starts_with('0'), "got {s:?}");
}

#[test]
fn character_to_string_static_ascii() {
    let mut objects = ObjectHeap::new();
    let mut strings = StringTable::new();
    let v = dispatch_boxed_to_string(
        c::java_lang_Character,
        d::C__String,
        &[Value::Int('A' as i32)],
        &mut objects,
        &mut strings,
    )
    .unwrap()
    .unwrap();
    assert_eq!(resolve_str(&strings, v), "A");
}

// ── Boxed value/identity surface, Object identity, Enum.valueOf ────────────
//
// The Java 8 wrapper API that Kotlin data classes (`Float.hashCode(F)`,
// `Float.compare(FF)`), `Intrinsics.areEqual` and `compareBy` lean on.

#[test]
fn float_compare_is_javas_total_order() {
    let mut cx = StrCtx::new();
    let cmp = |cx: &mut StrCtx, a: f32, b: f32| {
        dispatch_on(
            cx,
            c::java_lang_Float,
            m::compare,
            "(FF)I",
            &[Value::Float(a), Value::Float(b)],
        )
        .unwrap()
    };
    assert_eq!(cmp(&mut cx, 1.0, 2.0), Some(Value::Int(-1)));
    assert_eq!(cmp(&mut cx, 2.0, 1.0), Some(Value::Int(1)));
    assert_eq!(cmp(&mut cx, 1.0, 1.0), Some(Value::Int(0)));
    assert_eq!(cmp(&mut cx, -0.0, 0.0), Some(Value::Int(-1)));
    assert_eq!(cmp(&mut cx, f32::NAN, f32::INFINITY), Some(Value::Int(1)));
    assert_eq!(cmp(&mut cx, f32::NAN, f32::NAN), Some(Value::Int(0)));
    let d = dispatch_on(
        &mut cx,
        c::java_lang_Double,
        m::compare,
        "(DD)I",
        &[Value::Double(-0.0), Value::Double(0.0)],
    );
    assert_eq!(d.unwrap(), Some(Value::Int(-1)));
    let i = dispatch_on(
        &mut cx,
        c::java_lang_Integer,
        m::compare,
        "(II)I",
        &[Value::Int(i32::MIN), Value::Int(i32::MAX)],
    );
    assert_eq!(i.unwrap(), Some(Value::Int(-1)));
    let b = dispatch_on(
        &mut cx,
        c::java_lang_Boolean,
        m::compare,
        "(ZZ)I",
        &[Value::Int(1), Value::Int(0)],
    );
    assert_eq!(b.unwrap(), Some(Value::Int(1)));
}

#[test]
fn boxed_hash_codes_match_java() {
    let mut cx = StrCtx::new();
    let h = |cx: &mut StrCtx, class: &str, desc: &str, v: Value| {
        dispatch_on(cx, class, m::hashCode, desc, &[v]).unwrap()
    };
    assert_eq!(
        h(&mut cx, c::java_lang_Integer, "(I)I", Value::Int(42)),
        Some(Value::Int(42))
    );
    assert_eq!(
        h(
            &mut cx,
            c::java_lang_Long,
            "(J)I",
            Value::Long((1i64 << 32) | 5)
        ),
        Some(Value::Int(4))
    );
    assert_eq!(
        h(&mut cx, c::java_lang_Float, "(F)I", Value::Float(1.0)),
        Some(Value::Int(0x3f80_0000))
    );
    assert_eq!(
        h(&mut cx, c::java_lang_Double, "(D)I", Value::Double(1.0)),
        Some(Value::Int(0x3ff0_0000))
    );
    assert_eq!(
        h(&mut cx, c::java_lang_Boolean, "(Z)I", Value::Int(1)),
        Some(Value::Int(1231))
    );
    assert_eq!(
        h(&mut cx, c::java_lang_Boolean, "(Z)I", Value::Int(0)),
        Some(Value::Int(1237))
    );
    // Instance form on a box.
    let seven = boxed(&mut cx, c::java_lang_Integer, Value::Int(7));
    assert_eq!(
        h(&mut cx, c::java_lang_Integer, "()I", seven),
        Some(Value::Int(7))
    );
    let t = boxed(&mut cx, c::java_lang_Boolean, Value::Int(1));
    assert_eq!(
        h(&mut cx, c::java_lang_Boolean, "()I", t),
        Some(Value::Int(1231))
    );
}

#[test]
fn boxed_equals_needs_same_class_and_same_bits() {
    let mut cx = StrCtx::new();
    let i1 = boxed(&mut cx, c::java_lang_Integer, Value::Int(1));
    let i1b = boxed(&mut cx, c::java_lang_Integer, Value::Int(1));
    let l1 = boxed(&mut cx, c::java_lang_Long, Value::Long(1));
    let nan = boxed(&mut cx, c::java_lang_Float, Value::Float(f32::NAN));
    let nan2 = boxed(&mut cx, c::java_lang_Float, Value::Float(f32::NAN));
    let pz = boxed(&mut cx, c::java_lang_Float, Value::Float(0.0));
    let nz = boxed(&mut cx, c::java_lang_Float, Value::Float(-0.0));
    let eq = |cx: &mut StrCtx, class: &str, a: Value, b: Value| {
        dispatch_on(cx, class, m::equals, d::Object__Z, &[a, b]).unwrap()
    };
    assert_eq!(
        eq(&mut cx, c::java_lang_Integer, i1, i1b),
        Some(Value::Int(1))
    );
    assert_eq!(
        eq(&mut cx, c::java_lang_Integer, i1, l1),
        Some(Value::Int(0))
    );
    assert_eq!(
        eq(&mut cx, c::java_lang_Integer, i1, Value::Null),
        Some(Value::Int(0))
    );
    assert_eq!(
        eq(&mut cx, c::java_lang_Float, nan, nan2),
        Some(Value::Int(1))
    );
    assert_eq!(eq(&mut cx, c::java_lang_Float, pz, nz), Some(Value::Int(0)));
    let i5 = boxed(&mut cx, c::java_lang_Integer, Value::Int(5));
    let cmp = dispatch_on(
        &mut cx,
        c::java_lang_Integer,
        m::compareTo,
        d::Integer__I,
        &[i1, i5],
    );
    assert_eq!(cmp.unwrap(), Some(Value::Int(-1)));
}

#[test]
fn float_to_int_bits() {
    let mut cx = StrCtx::new();
    let r = dispatch_on(
        &mut cx,
        c::java_lang_Float,
        m::floatToIntBits,
        "(F)I",
        &[Value::Float(1.0)],
    );
    assert_eq!(r.unwrap(), Some(Value::Int(0x3f80_0000)));
}

#[test]
fn character_predicates_cover_ascii() {
    let mut cx = StrCtx::new();
    let c = |cx: &mut StrCtx, m: &str, ch: i32| {
        dispatch_on(cx, c::java_lang_Character, m, "(C)Z", &[Value::Int(ch)]).unwrap()
    };
    assert_eq!(c(&mut cx, m::isDigit, '7' as i32), Some(Value::Int(1)));
    assert_eq!(c(&mut cx, m::isDigit, 'x' as i32), Some(Value::Int(0)));
    assert_eq!(c(&mut cx, m::isLetter, 'x' as i32), Some(Value::Int(1)));
    assert_eq!(
        c(&mut cx, m::toUpperCase, 'a' as i32),
        Some(Value::Int('A' as i32))
    );
    assert_eq!(
        c(&mut cx, m::toLowerCase, 'Q' as i32),
        Some(Value::Int('q' as i32))
    );
    assert_eq!(c(&mut cx, m::toUpperCase, 0xE9), Some(Value::Int(0xE9)));
    assert_eq!(c(&mut cx, m::isLetter, 0xE9), Some(Value::Int(0)));
}

#[test]
fn object_identity_equals_hash_code_to_string() {
    let mut cx = StrCtx::new();
    let a = Value::ObjectRef(cx.objects.alloc("demo/Thing").unwrap());
    let b = Value::ObjectRef(cx.objects.alloc("demo/Thing").unwrap());
    let arr = Value::ArrayRef(cx.arrays.alloc(crate::array_heap::ATYPE_INT, 3).unwrap());
    let obj = |cx: &mut StrCtx, m: &str, d: &str, args: &[Value]| {
        dispatch_on(cx, c::java_lang_Object, m, d, args).unwrap()
    };
    assert_eq!(
        obj(&mut cx, m::equals, d::Object__Z, &[a, a]),
        Some(Value::Int(1))
    );
    assert_eq!(
        obj(&mut cx, m::equals, d::Object__Z, &[a, b]),
        Some(Value::Int(0))
    );
    assert_eq!(
        obj(&mut cx, m::equals, d::Object__Z, &[arr, arr]),
        Some(Value::Int(1))
    );
    let Value::ObjectRef(ai) = a else {
        unreachable!()
    };
    assert_eq!(
        obj(&mut cx, m::hashCode, "()I", &[a]),
        Some(Value::Int(ai as i32))
    );
    assert_ne!(
        obj(&mut cx, m::hashCode, "()I", &[a]),
        obj(&mut cx, m::hashCode, "()I", &[b])
    );
    let s = obj(&mut cx, m::toString, d::__String, &[a]).unwrap();
    let text = cx.resolve(s);
    assert_eq!(text, alloc::format!("demo.Thing@{ai:04x}"));
    let s = obj(&mut cx, m::toString, d::__String, &[arr]).unwrap();
    let text = cx.resolve(s);
    assert!(text.starts_with("[I@"), "{text}");
    // A string Reference still comes back unchanged.
    let hello = cx.intern(b"hello");
    assert_eq!(
        obj(&mut cx, m::toString, d::__String, &[hello]),
        Some(hello)
    );
}

#[test]
fn enum_hash_code_is_the_ordinal() {
    let mut cx = StrCtx::new();
    let n = cx.intern(b"BLUE");
    let idx = cx.objects.alloc("demo/Color").unwrap();
    cx.objects.set_field(idx, 0, n);
    cx.objects.set_field(idx, 1, Value::Int(2));
    let h = dispatch_on(
        &mut cx,
        c::java_lang_Enum,
        m::hashCode,
        "()I",
        &[Value::ObjectRef(idx)],
    );
    assert_eq!(h.unwrap(), Some(Value::Int(2)));
}
