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
