// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── String.format ─────────────────────────────────────────────────────────

#[test]
fn format_literal_no_specifiers() {
    let mut ctx = StrCtx::new();
    assert_eq!(ctx.fmt(b"hello world", &[]), "hello world");
}

#[test]
fn format_percent_literal() {
    let mut ctx = StrCtx::new();
    assert_eq!(ctx.fmt(b"100%% done", &[]), "100% done");
}

#[test]
fn format_newline() {
    let mut ctx = StrCtx::new();
    assert_eq!(ctx.fmt(b"a%nb", &[]), "a\nb");
}

#[test]
fn format_string_basic() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"world");
    assert_eq!(ctx.fmt(b"hello, %s!", &[s]), "hello, world!");
}

#[test]
fn format_string_null() {
    let mut ctx = StrCtx::new();
    assert_eq!(ctx.fmt(b"=%s=", &[Value::Null]), "=null=");
}

#[test]
fn format_string_upper() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    assert_eq!(ctx.fmt(b"%S", &[s]), "HELLO");
}

#[test]
fn format_string_width_and_justify() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hi");
    assert_eq!(ctx.fmt(b"[%5s]", &[s]), "[   hi]");
    let s = ctx.intern(b"hi");
    assert_eq!(ctx.fmt(b"[%-5s]", &[s]), "[hi   ]");
}

#[test]
fn format_string_precision_truncates() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abcdef");
    assert_eq!(ctx.fmt(b"%.3s", &[s]), "abc");
}

#[test]
fn format_decimal_positive() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(42));
    assert_eq!(ctx.fmt(b"=%d=", &[n]), "=42=");
}

#[test]
fn format_decimal_negative() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(-7));
    assert_eq!(ctx.fmt(b"%d", &[n]), "-7");
}

#[test]
fn format_decimal_zero_pad() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(42));
    assert_eq!(ctx.fmt(b"%05d", &[n]), "00042");
}

#[test]
fn format_decimal_zero_pad_negative() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(-42));
    assert_eq!(ctx.fmt(b"%06d", &[n]), "-00042");
}

#[test]
fn format_decimal_plus_flag() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(42));
    assert_eq!(ctx.fmt(b"%+d", &[n]), "+42");
}

#[test]
fn format_decimal_grouping() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(1_234_567));
    assert_eq!(ctx.fmt(b"%,d", &[n]), "1,234,567");
}

#[test]
fn format_decimal_long() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Long, Value::Long(9_876_543_210));
    assert_eq!(ctx.fmt(b"%d", &[n]), "9876543210");
}

#[test]
fn format_hex_lower() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(0xdead_beefu32 as i32));
    assert_eq!(ctx.fmt(b"%x", &[n]), "deadbeef");
}

#[test]
fn format_hex_upper_alt() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(255));
    assert_eq!(ctx.fmt(b"%#X", &[n]), "0XFF");
}

#[test]
fn format_hex_zero_pad() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(0xab));
    assert_eq!(ctx.fmt(b"%08x", &[n]), "000000ab");
}

#[test]
fn format_octal() {
    let mut ctx = StrCtx::new();
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(8));
    assert_eq!(ctx.fmt(b"%o", &[n]), "10");
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(8));
    assert_eq!(ctx.fmt(b"%#o", &[n]), "010");
}

#[test]
fn format_char() {
    let mut ctx = StrCtx::new();
    let c = ctx.box_primitive(c::java_lang_Character, Value::Int(b'A' as i32));
    assert_eq!(ctx.fmt(b"%c", &[c]), "A");
}

#[test]
fn format_boolean() {
    let mut ctx = StrCtx::new();
    let t = ctx.box_primitive(c::java_lang_Boolean, Value::Int(1));
    assert_eq!(ctx.fmt(b"%b", &[t]), "true");
    let f = ctx.box_primitive(c::java_lang_Boolean, Value::Int(0));
    assert_eq!(ctx.fmt(b"%b", &[f]), "false");
    assert_eq!(ctx.fmt(b"%b", &[Value::Null]), "false");
}

#[test]
fn format_float_special_values_use_java_spelling() {
    // Rust's formatter spells them "inf"/"NaN"; Java prints "Infinity" and
    // ignores the 0 flag for them.
    let mut ctx = StrCtx::new();
    let inf = ctx.box_primitive(c::java_lang_Double, Value::Double(f64::INFINITY));
    let ninf = ctx.box_primitive(c::java_lang_Double, Value::Double(f64::NEG_INFINITY));
    let nan = ctx.box_primitive(c::java_lang_Double, Value::Double(f64::NAN));
    assert_eq!(ctx.fmt(b"%f", &[inf]), "Infinity");
    assert_eq!(ctx.fmt(b"%.2e", &[ninf]), "-Infinity");
    assert_eq!(ctx.fmt(b"%f", &[nan]), "NaN");
    assert_eq!(ctx.fmt(b"%010f", &[inf]), "  Infinity");
    assert_eq!(ctx.fmt(b"%+f", &[inf]), "+Infinity");
}

#[test]
fn format_s_of_double_keeps_double_precision() {
    let mut ctx = StrCtx::new();
    let d = ctx.box_primitive(c::java_lang_Double, Value::Double(1.0 / 3.0));
    assert_eq!(ctx.fmt(b"%s", &[d]), "0.3333333333333333");
    let big = ctx.box_primitive(c::java_lang_Double, Value::Double(1e10));
    assert_eq!(ctx.fmt(b"%s", &[big]), "1.0E10");
}

#[test]
fn double_stringification_keeps_double_precision() {
    // String.valueOf(double), StringBuilder.append(double) and
    // Arrays.toString(double[]) all narrowed to f32 first.
    let third = 1.0f64 / 3.0;
    let mut ctx = StrCtx::new();
    let r = ctx
        .dispatch(m::valueOf, d::D__String, &[Value::Double(third)])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(r), "0.3333333333333333");

    let mut sb = SbCtx::new();
    sb.call("<init>", "()V", None).unwrap();
    sb.call(m::append, d::D__StringBuilder, Some(Value::Double(third)))
        .unwrap();
    assert_eq!(sb.to_string(), "0.3333333333333333");

    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let arr = arrays.alloc(crate::array_heap::ATYPE_DOUBLE, 1).unwrap();
    arrays.store64(arr, 0, third.to_bits() as i64);
    assert_eq!(
        arrays_to_string_str(d::aD__String, arr, &mut strings, &mut objects, &mut arrays),
        "[0.3333333333333333]"
    );
}

#[test]
fn format_float_basic() {
    let mut ctx = StrCtx::new();
    let f = ctx.box_primitive(c::java_lang_Double, Value::Double(3.14));
    assert_eq!(ctx.fmt(b"%.2f", &[f]), "3.14");
}

#[test]
fn format_float_width_and_precision() {
    let mut ctx = StrCtx::new();
    let f = ctx.box_primitive(c::java_lang_Double, Value::Double(3.14159));
    assert_eq!(ctx.fmt(b"%10.4f", &[f]), "    3.1416");
}

#[test]
fn format_float_negative_zero_pad() {
    let mut ctx = StrCtx::new();
    let f = ctx.box_primitive(c::java_lang_Double, Value::Double(-1.5));
    assert_eq!(ctx.fmt(b"%08.2f", &[f]), "-0001.50");
}

#[test]
fn format_scientific() {
    let mut ctx = StrCtx::new();
    let f = ctx.box_primitive(c::java_lang_Double, Value::Double(12345.678));
    // Java prints 1.234568e+04 (6-digit default precision, rounded)
    assert_eq!(ctx.fmt(b"%e", &[f]), "1.234568e+04");
}

#[test]
fn format_mixed_specifiers() {
    let mut ctx = StrCtx::new();
    let name = ctx.intern(b"pico");
    let n = ctx.box_primitive(c::java_lang_Integer, Value::Int(42));
    let hx = ctx.box_primitive(c::java_lang_Integer, Value::Int(0xff));
    assert_eq!(
        ctx.fmt(b"%s=%d hex=%#x", &[name, n, hx]),
        "pico=42 hex=0xff"
    );
}

#[test]
fn format_too_few_args_throws() {
    let mut ctx = StrCtx::new();
    let fmt_ref = ctx.intern(b"%d %d");
    let one = ctx.box_primitive(c::java_lang_Integer, Value::Int(1));
    let arr = ctx.make_args(&[one]);
    let err = ctx.dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr]);
    assert!(matches!(err, Err(JvmError::Exception(_))));
}

#[test]
fn format_unknown_conversion_throws() {
    let mut ctx = StrCtx::new();
    let fmt_ref = ctx.intern(b"%q");
    let arr = ctx.make_args(&[]);
    let err = ctx.dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr]);
    assert!(matches!(err, Err(JvmError::Exception(_))));
}

#[test]
fn format_wrong_type_for_decimal_throws() {
    let mut ctx = StrCtx::new();
    let fmt_ref = ctx.intern(b"%d");
    let s = ctx.intern(b"not an int");
    let arr = ctx.make_args(&[s]);
    let err = ctx.dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr]);
    assert!(matches!(err, Err(JvmError::Exception(_))));
}

// ── bugbash S4: %s of an object uses Object.toString's identity shape ─────

#[test]
fn format_s_object_without_interpreter_uses_identity_shape() {
    // With no upcall env the fallback must match identity_to_string:
    // dotted name @ 4-hex index (it used to print pkg/Cls@<decimal>).
    let mut ctx = StrCtx::new();
    let obj = Value::ObjectRef(ctx.objects.alloc("com/example/Thing").unwrap());
    let out = ctx.fmt(b"%s", &[obj]);
    assert!(
        out.starts_with("com.example.Thing@") && out.len() == "com.example.Thing@".len() + 4,
        "{out}"
    );
}

/// The hash column of `BUILTIN_DISPATCH` is `name_hash` of its class column
/// — the lookup in `BuiltinHandler::dispatch` compares hashes first.
#[test]
fn builtin_dispatch_hash_column_matches_names() {
    for &(name, hash, _) in BUILTIN_DISPATCH {
        assert_eq!(
            hash,
            crate::class_file::name_hash(name.as_bytes()),
            "stale hash for {name}"
        );
    }
}
