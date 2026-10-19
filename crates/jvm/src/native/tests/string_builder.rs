// SPDX-License-Identifier: GPL-3.0-only
use super::*;

// ── StringBuilder native method tests ─────────────────────────────────────
//
// Each StringBuilder owns a buffer in ObjectHeap addressed by the slot index
// its receiver holds in field 0, so the harness allocates a real instance and
// passes it as `this` on every call.

#[test]
fn sb_init_empty_to_string() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    assert_eq!(ctx.to_string(), "");
}

#[test]
fn sb_append_string() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.strings.intern(b"hello").unwrap();
    ctx.call(
        m::append,
        d::String__StringBuilder,
        Some(Value::Reference(s)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "hello");
}

#[test]
fn sb_append_int() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(m::append, d::I__StringBuilder, Some(Value::Int(42)))
        .unwrap();
    assert_eq!(ctx.to_string(), "42");
}

#[test]
fn sb_append_char() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        m::append,
        d::C__StringBuilder,
        Some(Value::Int(b'A' as i32)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "A");
}

#[test]
fn sb_char_at_out_of_range_throws() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        m::append,
        d::C__StringBuilder,
        Some(Value::Int(b'x' as i32)),
    )
    .unwrap();
    assert_eq!(
        ctx.call(m::charAt, "(I)C", Some(Value::Int(0))),
        Ok(Some(Value::Int(b'x' as i32)))
    );
    for i in [1, -1] {
        let r = ctx.call(m::charAt, "(I)C", Some(Value::Int(i)));
        let Err(JvmError::Exception(idx)) = r else {
            panic!("charAt({i}) = {r:?}");
        };
        assert_eq!(
            ctx.objects.class_name(idx),
            Some(c::java_lang_StringIndexOutOfBoundsException)
        );
    }
}

#[test]
fn sb_append_char_newline_passes_through() {
    // Java's append('\n') must yield a real newline (line-joining,
    // AlertDialog item lists) — not a space (regression for the old
    // `.max(0x20)` that turned every sub-0x20 control into a space).
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(
        m::append,
        d::C__StringBuilder,
        Some(Value::Int(b'a' as i32)),
    )
    .unwrap();
    ctx.call(
        m::append,
        d::C__StringBuilder,
        Some(Value::Int(b'\n' as i32)),
    )
    .unwrap();
    ctx.call(
        m::append,
        d::C__StringBuilder,
        Some(Value::Int(b'b' as i32)),
    )
    .unwrap();
    // A bell (0x07) is still scrubbed to a space.
    ctx.call(m::append, d::C__StringBuilder, Some(Value::Int(0x07)))
        .unwrap();
    assert_eq!(ctx.to_string(), "a\nb ");
}

#[test]
fn sb_append_bool_true() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(m::append, d::Z__StringBuilder, Some(Value::Int(1)))
        .unwrap();
    assert_eq!(ctx.to_string(), "true");
}

#[test]
fn sb_append_bool_false() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    ctx.call(m::append, d::Z__StringBuilder, Some(Value::Int(0)))
        .unwrap();
    assert_eq!(ctx.to_string(), "false");
}

#[test]
fn sb_length_and_char_at() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.strings.intern(b"abc").unwrap();
    ctx.call(
        m::append,
        d::String__StringBuilder,
        Some(Value::Reference(s)),
    )
    .unwrap();
    assert_eq!(ctx.call(m::length, "()I", None), Ok(Some(Value::Int(3))));
    assert_eq!(
        ctx.call(m::charAt, "(I)C", Some(Value::Int(1))),
        Ok(Some(Value::Int(b'b' as i32)))
    );
}

/// `"23" + '°' + "C"`: a char from U+0080 up is appended as its UTF-8 bytes;
/// the lone low byte it used to be made the whole string read as empty.
#[test]
fn sb_append_char_above_0x7f_is_utf8() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.strings.intern(b"23").unwrap();
    ctx.call(
        m::append,
        d::String__StringBuilder,
        Some(Value::Reference(s)),
    )
    .unwrap();
    ctx.call(
        m::append,
        "(C)Ljava/lang/StringBuilder;",
        Some(Value::Int(0xB0)),
    )
    .unwrap();
    ctx.call(
        m::append,
        "(C)Ljava/lang/StringBuilder;",
        Some(Value::Int(b'C' as i32)),
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "23°C");
}

// ── editing: insert / setLength / delete / deleteCharAt / setCharAt / reverse ──

#[test]
fn sb_insert_string_char_int_and_null() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let hello = ctx.str_arg("hello");
    ctx.call(m::append, d::String__StringBuilder, Some(hello))
        .unwrap();
    let big = ctx.str_arg("big ");
    ctx.call_args(m::insert, d::I_String__StringBuilder, &[Value::Int(0), big])
        .unwrap();
    assert_eq!(ctx.to_string(), "big hello");
    ctx.call_args(
        m::insert,
        d::I_C__StringBuilder,
        &[Value::Int(3), Value::Int(',' as i32)],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "big, hello");
    ctx.call_args(
        m::insert,
        d::I_I__StringBuilder,
        &[Value::Int(10), Value::Int(42)],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "big, hello42");
    ctx.call_args(
        m::insert,
        d::I_Object__StringBuilder,
        &[Value::Int(0), Value::Null],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "nullbig, hello42");
    ctx.call_args(
        m::insert,
        d::I_Z__StringBuilder,
        &[Value::Int(4), Value::Int(1)],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "nulltruebig, hello42");
}

#[test]
fn sb_insert_past_end_throws() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let x = ctx.str_arg("x");
    let r = ctx.call_args(m::insert, d::I_String__StringBuilder, &[Value::Int(1), x]);
    assert!(r.is_err(), "insert past the end must throw");
    assert_eq!(ctx.to_string(), "");
}

#[test]
fn sb_set_length_truncates_and_pads() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.str_arg("hello");
    ctx.call(m::append, d::String__StringBuilder, Some(s))
        .unwrap();
    ctx.call(m::setLength, "(I)V", Some(Value::Int(2))).unwrap();
    assert_eq!(ctx.to_string(), "he");
    ctx.call(m::setLength, "(I)V", Some(Value::Int(4))).unwrap();
    assert_eq!(ctx.to_string(), "he\0\0");
    ctx.call(m::setLength, "(I)V", Some(Value::Int(0))).unwrap();
    assert_eq!(ctx.to_string(), "");
    assert!(ctx
        .call(m::setLength, "(I)V", Some(Value::Int(-1)))
        .is_err());
}

#[test]
fn sb_delete_and_delete_char_at() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.str_arg("abcdef");
    ctx.call(m::append, d::String__StringBuilder, Some(s))
        .unwrap();
    ctx.call(m::deleteCharAt, d::I__StringBuilder, Some(Value::Int(1)))
        .unwrap();
    assert_eq!(ctx.to_string(), "acdef");
    // delete clamps `end` to the length, as Java does.
    ctx.call_args(
        m::delete,
        d::I_I__StringBuilder,
        &[Value::Int(3), Value::Int(99)],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "acd");
    // delete(len, len) is a no-op; deleteCharAt(len) is an error.
    ctx.call_args(
        m::delete,
        d::I_I__StringBuilder,
        &[Value::Int(3), Value::Int(3)],
    )
    .unwrap();
    assert!(ctx
        .call(m::deleteCharAt, d::I__StringBuilder, Some(Value::Int(3)))
        .is_err());
    assert!(ctx
        .call_args(
            m::delete,
            d::I_I__StringBuilder,
            &[Value::Int(2), Value::Int(1)]
        )
        .is_err());
    assert_eq!(ctx.to_string(), "acd");
}

#[test]
fn sb_set_char_at_and_reverse() {
    let mut ctx = SbCtx::new();
    ctx.call("<init>", "()V", None).unwrap();
    let s = ctx.str_arg("abc");
    ctx.call(m::append, d::String__StringBuilder, Some(s))
        .unwrap();
    ctx.call_args(
        m::setCharAt,
        "(IC)V",
        &[Value::Int(1), Value::Int('X' as i32)],
    )
    .unwrap();
    assert_eq!(ctx.to_string(), "aXc");
    assert!(ctx
        .call_args(
            m::setCharAt,
            "(IC)V",
            &[Value::Int(3), Value::Int('X' as i32)]
        )
        .is_err());
    let back = ctx.call(m::reverse, d::__StringBuilder, None).unwrap();
    assert_eq!(
        back,
        Some(Value::ObjectRef(ctx.this)),
        "reverse returns this"
    );
    assert_eq!(ctx.to_string(), "cXa");
}
