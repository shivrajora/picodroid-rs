// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::names::{c, d, m};
use crate::{array_heap::ArrayHeap, heap::StringTable, object_heap::ObjectHeap};

// ── String helper ─────────────────────────────────────────────────────────
//
// Holds the per-test state (strings, objects, arrays) so callers can intern
// strings before dispatching and resolve returned string references afterward.
struct StrCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
}

impl StrCtx {
    fn new() -> Self {
        Self {
            strings: StringTable::new(),
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
        }
    }

    /// Intern a static byte slice and return it as a Value::Reference.
    fn intern(&mut self, s: &'static [u8]) -> Value {
        Value::Reference(self.strings.intern(s).unwrap())
    }

    /// Dispatch a String method with the given args.
    fn dispatch(
        &mut self,
        method: &str,
        desc: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_lang_String, method, &mut ctx)
            .expect("String method not handled")
    }

    /// Resolve a Value::Reference to a &str (for asserting string output).
    fn resolve(&self, v: Value) -> &str {
        if let Value::Reference(idx) = v {
            self.strings.resolve(idx).unwrap_or("")
        } else {
            panic!("expected Reference, got {v:?}")
        }
    }
}

fn dispatch_math(
    method: &str,
    descriptor: &str,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor,
        args,
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_lang_Math, method, &mut ctx)
        .expect("Math method not handled")
}

// ── abs ──────────────────────────────────────────────────────────────────

#[test]
fn abs_min_value_is_min_value() {
    // Java: Math.abs(Integer.MIN_VALUE) == Integer.MIN_VALUE (no exception,
    // no panic). `i32::abs` overflows under debug overflow checks.
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(i32::MIN)]),
        Ok(Some(Value::Int(i32::MIN)))
    );
    assert_eq!(
        dispatch_math(m::abs, "(J)J", &[Value::Long(i64::MIN)]),
        Ok(Some(Value::Long(i64::MIN)))
    );
}

#[test]
fn round_negative_half_rounds_toward_positive_infinity() {
    // Java's Math.round is floor(x + 0.5): -2.5 -> -2, -0.5 -> 0, 2.5 -> 3.
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(-2.5)]),
        Ok(Some(Value::Int(-2)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(-2.5)]),
        Ok(Some(Value::Long(-2)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(-0.5)]),
        Ok(Some(Value::Long(0)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(2.5)]),
        Ok(Some(Value::Long(3)))
    );
    // NaN -> 0, saturation at the integer range.
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(f32::NAN)]),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(1e30)]),
        Ok(Some(Value::Long(i64::MAX)))
    );
}

#[test]
fn min_max_propagate_nan_and_order_signed_zero() {
    // Java: NaN if either argument is NaN; -0.0 < 0.0.
    let r = dispatch_math(
        m::min,
        "(DD)D",
        &[Value::Double(f64::NAN), Value::Double(1.0)],
    );
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d.is_nan()),
        "{r:?}"
    );
    let r = dispatch_math(
        m::max,
        "(FF)F",
        &[Value::Float(1.0), Value::Float(f32::NAN)],
    );
    assert!(
        matches!(r, Ok(Some(Value::Float(f))) if f.is_nan()),
        "{r:?}"
    );
    let r = dispatch_math(m::min, "(DD)D", &[Value::Double(0.0), Value::Double(-0.0)]);
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d == 0.0 && d.is_sign_negative()),
        "{r:?}"
    );
    let r = dispatch_math(m::max, "(DD)D", &[Value::Double(-0.0), Value::Double(0.0)]);
    assert!(
        matches!(r, Ok(Some(Value::Double(d))) if d == 0.0 && d.is_sign_positive()),
        "{r:?}"
    );
}

#[test]
fn abs_int_positive() {
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_int_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(I)I", &[Value::Int(-5)]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn abs_long_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(J)J", &[Value::Long(-10)]),
        Ok(Some(Value::Long(10)))
    );
}

#[test]
fn abs_float_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(F)F", &[Value::Float(-3.5)]),
        Ok(Some(Value::Float(3.5)))
    );
}

#[test]
fn abs_double_negative() {
    assert_eq!(
        dispatch_math(m::abs, "(D)D", &[Value::Double(-2.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

// ── min ──────────────────────────────────────────────────────────────────

#[test]
fn min_int() {
    assert_eq!(
        dispatch_math(m::min, "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn min_long() {
    assert_eq!(
        dispatch_math(m::min, "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(50)))
    );
}

#[test]
fn min_float() {
    assert_eq!(
        dispatch_math(m::min, "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(1.5)))
    );
}

#[test]
fn min_double() {
    assert_eq!(
        dispatch_math(m::min, "(DD)D", &[Value::Double(0.1), Value::Double(0.2)]),
        Ok(Some(Value::Double(0.1)))
    );
}

// ── max ──────────────────────────────────────────────────────────────────

#[test]
fn max_int() {
    assert_eq!(
        dispatch_math(m::max, "(II)I", &[Value::Int(3), Value::Int(7)]),
        Ok(Some(Value::Int(7)))
    );
}

#[test]
fn max_long() {
    assert_eq!(
        dispatch_math(m::max, "(JJ)J", &[Value::Long(100), Value::Long(50)]),
        Ok(Some(Value::Long(100)))
    );
}

#[test]
fn max_float() {
    assert_eq!(
        dispatch_math(m::max, "(FF)F", &[Value::Float(1.5), Value::Float(2.5)]),
        Ok(Some(Value::Float(2.5)))
    );
}

#[test]
fn max_double() {
    assert_eq!(
        dispatch_math(m::max, "(DD)D", &[Value::Double(9.0), Value::Double(3.0)]),
        Ok(Some(Value::Double(9.0)))
    );
}

// ── sqrt ─────────────────────────────────────────────────────────────────

#[test]
fn sqrt_four() {
    assert_eq!(
        dispatch_math(m::sqrt, "(D)D", &[Value::Double(4.0)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn sqrt_two() {
    let Value::Double(result) = dispatch_math(m::sqrt, "(D)D", &[Value::Double(2.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 1.4142135).abs() < 1e-6);
}

// ── pow ──────────────────────────────────────────────────────────────────

#[test]
fn pow_two_ten() {
    assert_eq!(
        dispatch_math(m::pow, "(DD)D", &[Value::Double(2.0), Value::Double(10.0)]),
        Ok(Some(Value::Double(1024.0)))
    );
}

// ── floor / ceil ─────────────────────────────────────────────────────────

#[test]
fn floor_positive() {
    assert_eq!(
        dispatch_math(m::floor, "(D)D", &[Value::Double(2.9)]),
        Ok(Some(Value::Double(2.0)))
    );
}

#[test]
fn floor_negative() {
    assert_eq!(
        dispatch_math(m::floor, "(D)D", &[Value::Double(-2.1)]),
        Ok(Some(Value::Double(-3.0)))
    );
}

#[test]
fn ceil_positive() {
    assert_eq!(
        dispatch_math(m::ceil, "(D)D", &[Value::Double(2.1)]),
        Ok(Some(Value::Double(3.0)))
    );
}

#[test]
fn ceil_negative() {
    assert_eq!(
        dispatch_math(m::ceil, "(D)D", &[Value::Double(-2.9)]),
        Ok(Some(Value::Double(-2.0)))
    );
}

// ── round ────────────────────────────────────────────────────────────────

#[test]
fn round_float_up() {
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(2.6)]),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn round_float_down() {
    assert_eq!(
        dispatch_math(m::round, "(F)I", &[Value::Float(2.4)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn round_double() {
    assert_eq!(
        dispatch_math(m::round, "(D)J", &[Value::Double(2.5)]),
        Ok(Some(Value::Long(3)))
    );
}

// ── sin / cos / tan ───────────────────────────────────────────────────────

#[test]
fn sin_zero() {
    assert_eq!(
        dispatch_math(m::sin, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

#[test]
fn cos_zero() {
    assert_eq!(
        dispatch_math(m::cos, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn sin_pi_over_2() {
    let Value::Double(result) = dispatch_math(
        m::sin,
        "(D)D",
        &[Value::Double(core::f64::consts::FRAC_PI_2)],
    )
    .unwrap()
    .unwrap() else {
        panic!("expected Double");
    };
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn tan_zero() {
    assert_eq!(
        dispatch_math(m::tan, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(0.0)))
    );
}

// ── atan2 ────────────────────────────────────────────────────────────────

#[test]
fn atan2_one_one() {
    let Value::Double(result) =
        dispatch_math(m::atan2, "(DD)D", &[Value::Double(1.0), Value::Double(1.0)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::FRAC_PI_4).abs() < 1e-10);
}

// ── toRadians / toDegrees ────────────────────────────────────────────────

#[test]
fn to_radians_180() {
    let Value::Double(result) = dispatch_math(m::toRadians, "(D)D", &[Value::Double(180.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::PI).abs() < 1e-10);
}

#[test]
fn to_degrees_pi() {
    let Value::Double(result) = dispatch_math(
        m::toDegrees,
        "(D)D",
        &[Value::Double(core::f64::consts::PI)],
    )
    .unwrap()
    .unwrap() else {
        panic!("expected Double");
    };
    assert!((result - 180.0).abs() < 1e-10);
}

// ── log / log10 / exp ────────────────────────────────────────────────────

#[test]
fn log_e() {
    let Value::Double(result) =
        dispatch_math(m::log, "(D)D", &[Value::Double(core::f64::consts::E)])
            .unwrap()
            .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 1.0).abs() < 1e-10);
}

#[test]
fn log10_100() {
    let Value::Double(result) = dispatch_math(m::log10, "(D)D", &[Value::Double(100.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - 2.0).abs() < 1e-10);
}

#[test]
fn exp_zero() {
    assert_eq!(
        dispatch_math(m::exp, "(D)D", &[Value::Double(0.0)]),
        Ok(Some(Value::Double(1.0)))
    );
}

#[test]
fn exp_one() {
    let Value::Double(result) = dispatch_math(m::exp, "(D)D", &[Value::Double(1.0)])
        .unwrap()
        .unwrap()
    else {
        panic!("expected Double");
    };
    assert!((result - core::f64::consts::E).abs() < 1e-10);
}

// ── String native method tests ────────────────────────────────────────────

static S_EMPTY: &[u8] = b"";
static S_HELLO: &[u8] = b"hello";
static S_ABC: &[u8] = b"abc";
static S_FOO: &[u8] = b"foo";
static S_BAR: &[u8] = b"bar";
static S_ELL: &[u8] = b"ell";
static S_HEL: &[u8] = b"hel";
static S_LLO: &[u8] = b"llo";
static S_PADDED: &[u8] = b"  hi  ";
static S_UPPER_HELLO: &[u8] = b"HELLO";

#[test]
fn string_length_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_EMPTY);
    assert_eq!(
        ctx.dispatch(m::length, "()I", &[s]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_length_nonempty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::length, "()I", &[s]),
        Ok(Some(Value::Int(5)))
    );
}

#[test]
fn string_char_at() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_ABC);
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(0)]),
        Ok(Some(Value::Int(b'a' as i32)))
    );
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(2)]),
        Ok(Some(Value::Int(b'c' as i32)))
    );
}

#[test]
fn string_index_of_string_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_ELL);
    assert_eq!(
        ctx.dispatch(m::indexOf, d::String__I, &[haystack, needle]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_index_of_string_not_found() {
    let mut ctx = StrCtx::new();
    let haystack = ctx.intern(S_HELLO);
    let needle = ctx.intern(S_BAR);
    assert_eq!(
        ctx.dispatch(m::indexOf, d::String__I, &[haystack, needle]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_index_of_char_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::indexOf, "(I)I", &[s, Value::Int(b'l' as i32)]),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn string_index_of_char_not_found() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    assert_eq!(
        ctx.dispatch(m::indexOf, "(I)I", &[s, Value::Int(b'z' as i32)]),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn string_substring() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch(
            m::substring,
            d::I_I__String,
            &[s, Value::Int(1), Value::Int(4)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "ell");
}

#[test]
fn string_equals() {
    let mut ctx = StrCtx::new();
    let foo1 = ctx.intern(S_FOO);
    let foo2 = ctx.intern(S_FOO);
    let bar = ctx.intern(S_BAR);
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, foo2]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, bar]),
        Ok(Some(Value::Int(0)))
    );
    // equals(null) must return false, not an error
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[foo1, Value::Null]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_starts_ends_with() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_HELLO);
    let hel = ctx.intern(S_HEL);
    let llo = ctx.intern(S_LLO);
    assert_eq!(
        ctx.dispatch(m::startsWith, d::String__Z, &[s, hel]),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        ctx.dispatch(m::endsWith, d::String__Z, &[s, llo]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_to_upper_lower() {
    let mut ctx = StrCtx::new();
    let lower = ctx.intern(S_HELLO);
    let result = ctx
        .dispatch(m::toUpperCase, d::__String, &[lower])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "HELLO");

    let upper = ctx.intern(S_UPPER_HELLO);
    let result = ctx
        .dispatch(m::toLowerCase, d::__String, &[upper])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_trim() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(S_PADDED);
    let result = ctx.dispatch(m::trim, d::__String, &[s]).unwrap().unwrap();
    assert_eq!(ctx.resolve(result), "hi");
}

// ── StringBuilder native method tests ─────────────────────────────────────
//
// Each StringBuilder owns a buffer in ObjectHeap addressed by the slot index
// its receiver holds in field 0, so the harness allocates a real instance and
// passes it as `this` on every call.

struct SbCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    this: u16,
}

impl SbCtx {
    fn new() -> Self {
        let mut objects = ObjectHeap::new();
        let this = objects.alloc(c::java_lang_StringBuilder).unwrap();
        Self {
            strings: StringTable::new(),
            objects,
            arrays: ArrayHeap::new(),
            this,
        }
    }

    fn call(
        &mut self,
        method: &str,
        desc: &str,
        extra: Option<Value>,
    ) -> Result<Option<Value>, JvmError> {
        let this = Value::ObjectRef(self.this);
        let args: alloc::vec::Vec<Value> = match extra {
            None => alloc::vec![this],
            Some(v) => alloc::vec![this, v],
        };
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_lang_StringBuilder, method, &mut ctx)
            .expect("StringBuilder method not handled")
    }

    fn to_string(&mut self) -> &str {
        let result = self.call(m::toString, d::__String, None).unwrap().unwrap();
        if let Value::Reference(idx) = result {
            // SAFETY: the string is interned into self.strings and lives as long as self
            let ptr = self.strings.resolve(idx).unwrap_or("") as *const str;
            unsafe { &*ptr }
        } else {
            panic!("toString returned non-Reference")
        }
    }
}

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

// ── Boxed type tests ──────────────────────────────────────────────────────
//
// Each test allocates a boxed object via valueOf, then reads it back via
// the unboxing accessor, sharing the same ObjectHeap across both calls.

fn dispatch_boxed(
    class: &str,
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, method, &mut ctx)
        .expect("boxed method not handled")
}

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

fn dispatch_boxed_to_string(
    class: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, m::toString, &mut ctx)
        .expect("toString not handled")
}

fn resolve_str<'a>(strings: &'a StringTable, v: Value) -> &'a str {
    if let Value::Reference(idx) = v {
        strings.resolve(idx).unwrap_or("")
    } else {
        panic!("expected Reference, got {v:?}");
    }
}

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

// ── ArrayList / Collections tests ─────────────────────────────────────────

fn dispatch_list(
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_ArrayList, method, &mut ctx)
        .expect("ArrayList method not handled")
}

#[test]
fn arraylist_init_and_size() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_list(m::isEmpty, "()Z", &[list], &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn arraylist_add_and_get() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(10)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(20)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(0)], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(1)], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn arraylist_set_returns_old() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(1)], &mut objects).unwrap();
    // set(0, 99) returns the old value Int(1)
    assert_eq!(
        dispatch_list(
            m::set,
            d::I_Object__Object,
            &[list, Value::Int(0), Value::Int(99)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(m::get, d::I__Object, &[list, Value::Int(0)], &mut objects),
        Ok(Some(Value::Int(99)))
    );
}

#[test]
fn arraylist_to_array_keeps_object_zero() {
    // The first object an executor allocates lives in slot 0; a round trip
    // through an Object[] must hand it back, not null.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let first = objects.alloc("Foo").unwrap();
    assert_eq!(first, 0);
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(
        m::add,
        d::Object__Z,
        &[list, Value::ObjectRef(first)],
        &mut objects,
    )
    .unwrap();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: d::__aObject,
        args: &[list],
        strings: &mut strings,
        objects: &mut objects,
        arrays: &mut arrays,
        upcall: None,
    };
    let arr = BuiltinHandler
        .dispatch(c::java_util_ArrayList, m::toArray, &mut ctx)
        .unwrap()
        .unwrap()
        .unwrap();
    let Value::ArrayRef(a) = arr else {
        panic!("expected ArrayRef");
    };
    let raw = arrays.load(a, 0).unwrap();
    assert_eq!(crate::array_heap::decode_ref(raw), Value::ObjectRef(0));
}

#[test]
fn arraylist_contains_matches_string_content() {
    // A literal and a runtime-built string with the same text are distinct
    // References; HashMap compared contents, ArrayList compared indices.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    let lit = Value::Reference(strings.intern(b"ab").unwrap());
    let dynamic = Value::Reference(strings.intern_dyn(b"ab").unwrap());
    assert_ne!(lit, dynamic);
    dispatch_list(m::add, d::Object__Z, &[list, lit], &mut objects).unwrap();
    let mut call = |m: &str, d: &str, args: &[Value], objects: &mut ObjectHeap| {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d,
            args,
            strings: &mut strings,
            objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m, &mut ctx)
            .unwrap()
    };
    let obj = d::Object__Z;
    assert_eq!(
        call(m::contains, obj, &[list, dynamic], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::remove, obj, &[list, dynamic], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn arraylist_index_bounds_throw_index_out_of_bounds() {
    // add(i, v) clamped (a negative index appended!), set(i, v) silently
    // returned null; get/remove were uncatchable hard errors.
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [1, 2] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let ioobe = |r: Result<Option<Value>, JvmError>, objects: &ObjectHeap, what: &str| {
        let Err(JvmError::Exception(idx)) = r else {
            panic!("{what}: {r:?}");
        };
        assert_eq!(
            objects.class_name(idx),
            Some(c::java_lang_IndexOutOfBoundsException),
            "{what}"
        );
    };
    let addi = d::I_Object__V;
    let r = dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(5), Value::Int(9)],
        &mut objects,
    );
    ioobe(r, &objects, "add(5)");
    let r = dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(-1), Value::Int(9)],
        &mut objects,
    );
    ioobe(r, &objects, "add(-1)");
    // add(size, v) is legal and appends.
    dispatch_list(
        m::add,
        addi,
        &[list, Value::Int(2), Value::Int(3)],
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(3)))
    );
    let seti = d::I_Object__Object;
    let r = dispatch_list(
        m::set,
        seti,
        &[list, Value::Int(9), Value::Int(0)],
        &mut objects,
    );
    ioobe(r, &objects, "set(9)");
    let r = dispatch_list(m::get, d::I__Object, &[list, Value::Int(9)], &mut objects);
    ioobe(r, &objects, "get(9)");
    let r = dispatch_list(m::get, d::I__Object, &[list, Value::Int(-1)], &mut objects);
    ioobe(r, &objects, "get(-1)");
    let r = dispatch_list(
        m::remove,
        d::I__Object,
        &[list, Value::Int(3)],
        &mut objects,
    );
    ioobe(r, &objects, "remove(3)");
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn arraylist_remove_object_overload() {
    // remove(Object) compiles to (Ljava/lang/Object;)Z; the arm demanded an
    // Int index and threw InvalidReference for it.
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let mut strings = StringTable::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    let a = Value::Reference(strings.intern(b"a").unwrap());
    let b = Value::Reference(strings.intern(b"b").unwrap());
    let five = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(five, 0, Value::Int(5));
    let five2 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(five2, 0, Value::Int(5));
    for v in [a, b, Value::ObjectRef(five)] {
        dispatch_list(m::add, d::Object__Z, &[list, v], &mut objects).unwrap();
    }
    let mut call = |m: &str, d: &str, args: &[Value], objects: &mut ObjectHeap| {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d,
            args,
            strings: &mut strings,
            objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m, &mut ctx)
            .unwrap()
    };
    let rm = d::Object__Z;
    assert_eq!(
        call(m::remove, rm, &[list, a], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
    assert_eq!(
        call(m::remove, rm, &[list, a], &mut objects),
        Ok(Some(Value::Int(0)))
    );
    // A different boxed Integer with the same value matches (equals semantics).
    assert_eq!(
        call(
            m::remove,
            rm,
            &[list, Value::ObjectRef(five2)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        call(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    // The index overload still works and returns the element.
    assert_eq!(
        call(
            m::remove,
            d::I__Object,
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(b))
    );
}

#[test]
fn arraylist_remove() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(5)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(
            m::remove,
            d::I__Object,
            &[list, Value::Int(0)],
            &mut objects
        ),
        Ok(Some(Value::Int(5)))
    );
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn arraylist_contains() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(7)], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(
            m::contains,
            d::Object__Z,
            &[list, Value::Int(7)],
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_list(
            m::contains,
            d::Object__Z,
            &[list, Value::Int(8)],
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

// ── HashMap native method tests ──────────────────────────────────────────

fn dispatch_map(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_HashMap, method, &mut ctx)
        .expect("HashMap method not handled")
}

fn dispatch_set(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_HashSet, method, &mut ctx)
        .expect("HashSet method not handled")
}

fn make_map(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let map = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[map], strings, objects).unwrap();
    map
}

fn make_set(strings: &mut StringTable, objects: &mut ObjectHeap) -> Value {
    let set = Value::ObjectRef(objects.alloc(c::java_util_HashSet).unwrap());
    dispatch_set("<init>", "()V", &[set], strings, objects).unwrap();
    set
}

#[test]
fn hashmap_init_and_size() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_map(m::isEmpty, "()Z", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_put_and_get() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10), put(2, 20), put(3, 30)
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(3), Value::Int(30)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(3)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(3)))
    );
}

#[test]
fn hashmap_put_overwrite() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, 10) returns null (no previous)
    assert_eq!(
        dispatch_map(
            m::put,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
    // put(1, 99) returns old value 10
    assert_eq!(
        dispatch_map(
            m::put,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashmap_get_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_remove() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::remove,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_remove_missing() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    assert_eq!(
        dispatch_map(
            m::remove,
            d::Object__Object,
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

#[test]
fn hashmap_contains_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(5), Value::Int(50)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(6)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_contains_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(42)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::containsValue,
            d::Object__Z,
            &[map, Value::Int(42)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::containsValue,
            d::Object__Z,
            &[map, Value::Int(99)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_clear() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(m::clear, "()V", &[map], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_map(m::size, "()I", &[map], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashmap_get_or_default() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // Key present: returns value
    assert_eq!(
        dispatch_map(
            m::getOrDefault,
            d::Object_Object__Object,
            &[map, Value::Int(1), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(10)))
    );
    // Key absent: returns default
    assert_eq!(
        dispatch_map(
            m::getOrDefault,
            d::Object_Object__Object,
            &[map, Value::Int(99), Value::Int(-1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-1)))
    );
}

#[test]
fn hashmap_integer_keys() {
    // Test with boxed Integer objects as keys (wrapper equality via field 0)
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    // Create two Integer(42) objects at different heap slots
    let int1 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1, 0, Value::Int(42));
    let int2 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int2, 0, Value::Int(42));
    assert_ne!(int1, int2); // different heap slots

    // put with int1 as key
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::ObjectRef(int1), Value::Int(100)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // get with int2 as key — should find it via wrapper equality
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::ObjectRef(int2)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(100)))
    );
}

#[test]
fn hashmap_string_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    let key_a = Value::Reference(strings.intern(m::alpha.as_bytes()).unwrap());
    let key_b = Value::Reference(strings.intern(b"beta").unwrap());

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, key_a, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, key_b, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, key_a],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, key_b],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}

#[test]
fn hashmap_null_key() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Null, Value::Int(77)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Null],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(77)))
    );
}

#[test]
fn hashmap_null_value() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    // put(1, null)
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Null],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // containsKey should return true
    assert_eq!(
        dispatch_map(
            m::containsKey,
            d::Object__Z,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    // get returns Null (same as "not found"), but containsKey distinguishes
    assert_eq!(
        dispatch_map(
            m::get,
            d::Object__Object,
            &[map, Value::Int(1)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Null))
    );
}

// ── HashSet native method tests ──────────────────────────────────────────

#[test]
fn hashset_add_contains_remove() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    // add(10) returns true (was absent)
    assert_eq!(
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(
            m::contains,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );

    // remove(10) returns true (was present)
    assert_eq!(
        dispatch_set(
            m::remove,
            d::Object__Z,
            &[set, Value::Int(10)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn hashset_add_duplicate() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(5)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    // Second add returns false (was already present)
    assert_eq!(
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(5)],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn hashset_iterator_visits_every_element() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);
    for v in [7, 3, 7, 9] {
        dispatch_set(
            m::add,
            d::Object__Z,
            &[set, Value::Int(v)],
            &mut strings,
            &mut objects,
        )
        .unwrap();
    }
    let iter = dispatch_set(
        m::iterator,
        d::__Iterator,
        &[set],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let mut seen = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        seen.push(
            dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(seen.len(), 3);
    for v in [3, 7, 9] {
        assert!(seen.contains(&Value::Int(v)));
    }
}

#[test]
fn hashset_clear() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let set = make_set(&mut strings, &mut objects);

    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(1)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set(
        m::add,
        d::Object__Z,
        &[set, Value::Int(2)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_set(m::clear, "()V", &[set], &mut strings, &mut objects).unwrap();
    assert_eq!(
        dispatch_set(m::size, "()I", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
    assert_eq!(
        dispatch_set(m::isEmpty, "()Z", &[set], &mut strings, &mut objects),
        Ok(Some(Value::Int(1)))
    );
}

// ── Regression: integer-key map then string-key map ──────────────────────

#[test]
fn hashmap_int_then_string_keys_shared_heap() {
    // Reproduces the sim bug: creating a HashMap with Integer keys, using
    // StringBuilder, then creating a second HashMap with string keys fails
    // to find the string keys.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();

    // Map 1: Integer keys
    let m1 = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[m1], &mut strings, &mut objects).unwrap();

    // Integer.valueOf(1) — alloc Integer, set field 0
    let int1 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1, 0, Value::Int(1));
    let int10 = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int10, 0, Value::Int(10));

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[m1, Value::ObjectRef(int1), Value::ObjectRef(int10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // Verify m1.get works
    let int1b = objects.alloc(c::java_lang_Integer).unwrap();
    objects.set_field(int1b, 0, Value::Int(1));
    let result = dispatch_map(
        m::get,
        d::Object__Object,
        &[m1, Value::ObjectRef(int1b)],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, Value::ObjectRef(int10));

    // StringBuilder usage (simulating "v1=" + v1)
    let _sb = objects.alloc(c::java_lang_StringBuilder).unwrap();
    let sb_buf = objects.sb_alloc().unwrap();
    objects.sb_append_bytes(sb_buf, b"v1=");
    objects.sb_append_int(sb_buf, 10);
    let sb_bytes = objects.sb_contents_slice(sb_buf).to_vec();
    let _str_idx = strings.intern_dyn(&sb_bytes).unwrap();

    // Map 2: String keys
    let m2 = Value::ObjectRef(objects.alloc(c::java_util_HashMap).unwrap());
    dispatch_map("<init>", "()V", &[m2], &mut strings, &mut objects).unwrap();

    let hello = Value::Reference(strings.intern(b"hello").unwrap());
    let world = Value::Reference(strings.intern(b"world").unwrap());

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[m2, hello, world],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // This should find "hello" → "world"
    let result = dispatch_map(
        m::get,
        d::Object__Object,
        &[m2, hello],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, world);
}

// ── Iterator native method tests ─────────────────────────────────────────

fn dispatch_iter(
    method: &str,
    desc: &str,
    args: &[Value],
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_Iterator, method, &mut ctx)
        .expect("Iterator method not handled")
}

#[test]
fn iterator_arraylist_empty() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    // Create iterator via ArrayList.iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // hasNext should be false immediately
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_basic() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(10)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(20)], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(30)], &mut objects).unwrap();

    // Create iterator
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Iterate: hasNext/next cycle
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_arraylist_single() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    dispatch_list(m::add, d::Object__Z, &[list, Value::Int(42)], &mut objects).unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(42)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_next_past_end() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();

    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[list],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // next() on empty iterator should error
    assert!(dispatch_iter(m::next, d::__Object, &[iter], &mut objects).is_err());
}

#[test]
fn iterator_hashmap_keys() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // keySet()
    let keyset = dispatch_map(m::keySet, d::__Set, &[map], &mut strings, &mut objects)
        .unwrap()
        .unwrap();

    // keySet().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[keyset],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_HashMap_KeySet, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    // Collect keys
    let mut keys = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let k = dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
            .unwrap()
            .unwrap();
        keys.push(k);
    }
    assert_eq!(keys.len(), 2);
    // Keys should be Int(1) and Int(2) (order not guaranteed, but our impl preserves insertion order)
    assert!(keys.contains(&Value::Int(1)));
    assert!(keys.contains(&Value::Int(2)));
}

#[test]
fn hashmap_key_and_value_views_answer_contains() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    let keyset = dispatch_map(m::keySet, d::__Set, &[map], &mut strings, &mut objects)
        .unwrap()
        .unwrap();
    let values = dispatch_map(
        m::values,
        d::__Collection,
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let mut arrays = ArrayHeap::new();
    let mut probe = |class: &str, view: Value, needle: Value| -> Value {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::Object__Z,
            args: &[view, needle],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(class, m::contains, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        probe(c::java_util_HashMap_KeySet, keyset, Value::Int(1)),
        Value::Int(1)
    );
    assert_eq!(
        probe(c::java_util_HashMap_KeySet, keyset, Value::Int(10)),
        Value::Int(0)
    );
    assert_eq!(
        probe(c::java_util_HashMap_Values, values, Value::Int(10)),
        Value::Int(1)
    );
    assert_eq!(
        probe(c::java_util_HashMap_Values, values, Value::Int(1)),
        Value::Int(0)
    );
}

#[test]
fn iterator_hashmap_values() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let map = make_map(&mut strings, &mut objects);

    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(1), Value::Int(10)],
        &mut strings,
        &mut objects,
    )
    .unwrap();
    dispatch_map(
        m::put,
        d::Object_Object__Object,
        &[map, Value::Int(2), Value::Int(20)],
        &mut strings,
        &mut objects,
    )
    .unwrap();

    // values()
    let vals = dispatch_map(
        m::values,
        d::__Collection,
        &[map],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();

    // values().iterator()
    let mut arrays = ArrayHeap::new();
    let iter = {
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: d::__Iterator,
            args: &[vals],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_HashMap_Values, m::iterator, &mut ctx)
            .unwrap()
            .unwrap()
            .unwrap()
    };

    let mut values = alloc::vec::Vec::new();
    while dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects)
        .unwrap()
        .unwrap()
        == Value::Int(1)
    {
        let v = dispatch_iter(m::next, d::__Object, &[iter], &mut objects)
            .unwrap()
            .unwrap();
        values.push(v);
    }
    assert_eq!(values.len(), 2);
    assert!(values.contains(&Value::Int(10)));
    assert!(values.contains(&Value::Int(20)));
}

// ── Enum native method tests ─────────────────────────────────────────────

fn dispatch_enum(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_lang_Enum, method, &mut ctx)
        .expect("Enum method not handled")
}

fn make_enum_instance(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    name: &'static [u8],
    ordinal: i32,
) -> Value {
    let obj = Value::ObjectRef(objects.alloc("TestEnum").unwrap());
    let name_ref = Value::Reference(strings.intern(name).unwrap());
    dispatch_enum(
        "<init>",
        d::String_I__V,
        &[obj, name_ref, Value::Int(ordinal)],
        strings,
        objects,
    )
    .unwrap();
    obj
}

#[test]
fn enum_init_name_ordinal() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    let name = dispatch_enum(m::name, d::__String, &[red], &mut strings, &mut objects)
        .unwrap()
        .unwrap();
    let Value::Reference(idx) = name else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("RED"));

    assert_eq!(
        dispatch_enum(m::ordinal, "()I", &[red], &mut strings, &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_to_string() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    let result = dispatch_enum(
        m::toString,
        d::__String,
        &[green],
        &mut strings,
        &mut objects,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = result else {
        panic!("expected Reference");
    };
    assert_eq!(strings.resolve(idx), Some("GREEN"));
}

#[test]
fn enum_equals_same() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);

    assert_eq!(
        dispatch_enum(
            m::equals,
            d::Object__Z,
            &[red, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn enum_equals_different() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let green = make_enum_instance(&mut objects, &mut strings, b"GREEN", 1);

    assert_eq!(
        dispatch_enum(
            m::equals,
            d::Object__Z,
            &[red, green],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn enum_compare_to() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let red = make_enum_instance(&mut objects, &mut strings, b"RED", 0);
    let blue = make_enum_instance(&mut objects, &mut strings, b"BLUE", 2);

    // RED(0).compareTo(BLUE(2)) = -2
    assert_eq!(
        dispatch_enum(
            m::compareTo,
            d::Enum__I,
            &[red, blue],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(-2)))
    );
    // BLUE(2).compareTo(RED(0)) = 2
    assert_eq!(
        dispatch_enum(
            m::compareTo,
            d::Enum__I,
            &[blue, red],
            &mut strings,
            &mut objects
        ),
        Ok(Some(Value::Int(2)))
    );
}

// ── String enhancement tests ────────────────────────────────────────────

#[test]
fn string_concat() {
    let mut ctx = StrCtx::new();
    let a = ctx.intern(b"hello");
    let b = ctx.intern(b" world");
    let result = ctx
        .dispatch(m::concat, d::String__String, &[a, b])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello world");
}

#[test]
fn string_concat_empty() {
    let mut ctx = StrCtx::new();
    let a = ctx.intern(b"hello");
    let empty = ctx.intern(b"");
    let result = ctx
        .dispatch(m::concat, d::String__String, &[a, empty])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_hash_code_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    assert_eq!(
        ctx.dispatch(m::hashCode, "()I", &[s]),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn string_hash_code_known() {
    // Java's "abc".hashCode() = 96354
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    assert_eq!(
        ctx.dispatch(m::hashCode, "()I", &[s]),
        Ok(Some(Value::Int(96354)))
    );
}

#[test]
fn string_replace_char() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let result = ctx
        .dispatch(
            m::replace,
            d::C_C__String,
            &[s, Value::Int(b'l' as i32), Value::Int(b'r' as i32)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "herro");
}

#[test]
fn string_replace_char_no_match() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let result = ctx
        .dispatch(
            m::replace,
            d::C_C__String,
            &[s, Value::Int(b'z' as i32), Value::Int(b'y' as i32)],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hello");
}

#[test]
fn string_replace_string() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"aXbXc");
    let target = ctx.intern(b"X");
    let repl = ctx.intern(b"YY");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "aYYbYYc");
}

#[test]
fn string_replace_string_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let target = ctx.intern(b"b");
    let repl = ctx.intern(b"");
    let result = ctx
        .dispatch(
            m::replace,
            d::CharSequence_CharSequence__String,
            &[s, target, repl],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "ac");
}

#[test]
fn string_to_char_array() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let result = ctx.dispatch(m::toCharArray, "()[C", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    assert_eq!(ctx.arrays.load(arr, 0), Some(b'a' as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(b'b' as i32));
    assert_eq!(ctx.arrays.load(arr, 2), Some(b'c' as i32));
}

#[test]
fn string_to_char_array_empty() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"");
    let result = ctx.dispatch(m::toCharArray, "()[C", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(0));
}

#[test]
fn string_get_bytes() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    let result = ctx.dispatch(m::getBytes, "()[B", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.atype(arr), Some(crate::array_heap::ATYPE_BYTE));
    assert_eq!(ctx.arrays.length(arr), Some(3));
    assert_eq!(ctx.arrays.load(arr, 0), Some(b'a' as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(b'b' as i32));
    assert_eq!(ctx.arrays.load(arr, 2), Some(b'c' as i32));
}

#[test]
fn string_get_bytes_sign_extends() {
    let mut ctx = StrCtx::new();
    // 0xC2 0xB0 = UTF-8 "°" — high bytes must come back as negative i32s,
    // matching baload's sign-extension of byte[] slots.
    let s = ctx.intern(b"\xC2\xB0");
    let result = ctx.dispatch(m::getBytes, "()[B", &[s]).unwrap().unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.load(arr, 0), Some(0xC2u8 as i8 as i32));
    assert_eq!(ctx.arrays.load(arr, 1), Some(0xB0u8 as i8 as i32));
}

/// `new String(byte[])` native arm: returns the interned Reference (the
/// interpreter's `finalize_invoke` swaps it for the placeholder receiver).
#[test]
fn string_init_from_bytes() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 3).unwrap();
    for (i, b) in [b'h', b'e', b'y'].iter().enumerate() {
        ctx.arrays.store(arr, i, *b as i32);
    }
    let result = ctx
        .dispatch("<init>", "([B)V", &[Value::Null, Value::ArrayRef(arr)])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "hey");
}

#[test]
fn string_init_from_bytes_range_and_bounds() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 5).unwrap();
    for (i, b) in b"abcde".iter().enumerate() {
        ctx.arrays.store(arr, i, *b as i32);
    }
    let result = ctx
        .dispatch(
            "<init>",
            "([BII)V",
            &[
                Value::Null,
                Value::ArrayRef(arr),
                Value::Int(1),
                Value::Int(3),
            ],
        )
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "bcd");

    // off+len past the end / negative len → bounds error, not a panic.
    for (off, len) in [(3, 3), (0, 6), (-1, 2), (0, -1)] {
        let r = ctx.dispatch(
            "<init>",
            "([BII)V",
            &[
                Value::Null,
                Value::ArrayRef(arr),
                Value::Int(off),
                Value::Int(len),
            ],
        );
        assert_eq!(
            r,
            Err(JvmError::ArrayIndexOutOfBounds),
            "off={off} len={len}"
        );
    }
}

#[test]
fn string_init_from_bytes_sanitizes_non_ascii() {
    let mut ctx = StrCtx::new();
    let arr = ctx.arrays.alloc(crate::array_heap::ATYPE_BYTE, 3).unwrap();
    ctx.arrays.store(arr, 0, b'a' as i32);
    ctx.arrays.store(arr, 1, 0xC2u8 as i8 as i32); // high byte → '?'
    ctx.arrays.store(arr, 2, b'z' as i32);
    let result = ctx
        .dispatch("<init>", "([B)V", &[Value::Null, Value::ArrayRef(arr)])
        .unwrap()
        .unwrap();
    assert_eq!(ctx.resolve(result), "a?z");
}

#[test]
fn string_split_basic() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,b,c");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r2 = ((ctx.arrays.load(arr, 2).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("a"));
    assert_eq!(ctx.strings.resolve(r1), Some("b"));
    assert_eq!(ctx.strings.resolve(r2), Some("c"));
}

#[test]
fn string_split_no_match() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"hello");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(1));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("hello"));
}

#[test]
fn string_split_multi_char() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a::b::c");
    let delim = ctx.intern(b"::");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r0 = ((ctx.arrays.load(arr, 0).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    let r2 = ((ctx.arrays.load(arr, 2).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r0), Some("a"));
    assert_eq!(ctx.strings.resolve(r1), Some("b"));
    assert_eq!(ctx.strings.resolve(r2), Some("c"));
}

#[test]
fn string_equals_non_string_is_false() {
    // "x".equals(someObject) / equals(array) is specified to be false —
    // it was a hard InvalidReference error (uncatchable).
    let mut ctx = StrCtx::new();
    let s = ctx.intern(m::x.as_bytes());
    let obj = Value::ObjectRef(ctx.objects.alloc("Foo").unwrap());
    let arr = Value::ArrayRef(ctx.arrays.alloc(crate::array_heap::ATYPE_INT, 1).unwrap());
    for other in [obj, arr, Value::Null] {
        assert_eq!(
            ctx.dispatch(m::equals, d::Object__Z, &[s, other]),
            Ok(Some(Value::Int(0))),
            "equals({other:?})"
        );
    }
    let same = ctx.intern(m::x.as_bytes());
    assert_eq!(
        ctx.dispatch(m::equals, d::Object__Z, &[s, same]),
        Ok(Some(Value::Int(1)))
    );
}

#[test]
fn string_char_at_out_of_range_throws() {
    // Java: StringIndexOutOfBoundsException, not '\0'.
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abc");
    for i in [3, -1, 100] {
        let r = ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(i)]);
        let Err(JvmError::Exception(idx)) = r else {
            panic!("charAt({i}) = {r:?}");
        };
        assert_eq!(
            ctx.objects.class_name(idx),
            Some(c::java_lang_StringIndexOutOfBoundsException)
        );
    }
    assert_eq!(
        ctx.dispatch(m::charAt, "(I)C", &[s, Value::Int(2)]),
        Ok(Some(Value::Int(b'c' as i32)))
    );
}

#[test]
fn string_index_of_honours_from_index() {
    // The 2-arg overloads used to drop fromIndex entirely, so the classic
    // `while ((i = s.indexOf(x, i + 1)) >= 0)` loop never terminated.
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"abcabc");
    let a = ctx.intern(b"a");
    let abc = ctx.intern(b"abc");
    let d = |ctx: &mut StrCtx, m: &str, desc: &str, args: &[Value]| -> i32 {
        match ctx.dispatch(m, desc, args) {
            Ok(Some(Value::Int(i))) => i,
            other => panic!("{m}{desc} -> {other:?}"),
        }
    };
    let so = d::String_I__I;
    let co = "(II)I";
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(1)]), 3);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(4)]), -1);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(-5)]), 0);
    assert_eq!(d(&mut ctx, m::indexOf, so, &[s, a, Value::Int(99)]), -1);
    assert_eq!(
        d(
            &mut ctx,
            m::indexOf,
            co,
            &[s, Value::Int(b'c' as i32), Value::Int(3)]
        ),
        5
    );
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, abc, Value::Int(3)]), 3);
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, abc, Value::Int(2)]), 0);
    assert_eq!(d(&mut ctx, m::lastIndexOf, so, &[s, a, Value::Int(-1)]), -1);
    assert_eq!(
        d(
            &mut ctx,
            m::lastIndexOf,
            co,
            &[s, Value::Int(b'a' as i32), Value::Int(2)]
        ),
        0
    );
    assert_eq!(
        d(
            &mut ctx,
            m::lastIndexOf,
            co,
            &[s, Value::Int(b'a' as i32), Value::Int(99)]
        ),
        3
    );
    // startsWith(prefix, toffset)
    let bc = ctx.intern(b"bc");
    let sw = d::String_I__Z;
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(1)]), 1);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(0)]), 0);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(-1)]), 0);
    assert_eq!(d(&mut ctx, m::startsWith, sw, &[s, bc, Value::Int(6)]), 0);
}

#[test]
fn string_value_of_char_newline_passes_through() {
    // Same defect StringBuilder.append(char) had (5d5f0a6): `.max(0x20)`
    // turned '\n'/'\t' into spaces, so String.valueOf('\n') joined lines
    // with a space.
    let mut ctx = StrCtx::new();
    for (c, want) in [
        (b'\n', "\n"),
        (b'\t', "\t"),
        (b'\r', "\r"),
        (b'a', "a"),
        (0x07u8, " "),
    ] {
        let r = ctx
            .dispatch(m::valueOf, d::C__String, &[Value::Int(c as i32)])
            .unwrap()
            .unwrap();
        assert_eq!(ctx.resolve(r), want, "valueOf({c:#x})");
    }
}

#[test]
fn string_split_empty_parts() {
    let mut ctx = StrCtx::new();
    let s = ctx.intern(b"a,,b");
    let delim = ctx.intern(b",");
    let result = ctx
        .dispatch(m::split, d::String__aString, &[s, delim])
        .unwrap()
        .unwrap();
    let Value::ArrayRef(arr) = result else {
        panic!("expected ArrayRef");
    };
    assert_eq!(ctx.arrays.length(arr), Some(3));
    let r1 = ((ctx.arrays.load(arr, 1).unwrap() as u32) & !crate::array_heap::REF_TAG) as u16;
    assert_eq!(ctx.strings.resolve(r1), Some(""));
}

#[test]
fn string_split_drops_trailing_empty_strings() {
    // Java's split(regex) has limit 0: trailing empty strings are removed,
    // interior ones kept, and a no-match input yields [input].
    fn split_len(ctx: &mut StrCtx, s: &'static [u8], d: &'static [u8]) -> u16 {
        let s = ctx.intern(s);
        let d = ctx.intern(d);
        let r = ctx
            .dispatch(m::split, d::String__aString, &[s, d])
            .unwrap()
            .unwrap();
        let Value::ArrayRef(arr) = r else {
            panic!("expected ArrayRef");
        };
        ctx.arrays.length(arr).unwrap()
    }
    let mut ctx = StrCtx::new();
    assert_eq!(split_len(&mut ctx, b"a,b,,", b","), 2);
    assert_eq!(split_len(&mut ctx, b",,", b","), 0);
    assert_eq!(split_len(&mut ctx, b"a,,b", b","), 3);
    assert_eq!(split_len(&mut ctx, b",a", b","), 2);
    assert_eq!(split_len(&mut ctx, b"", b","), 1);
    assert_eq!(split_len(&mut ctx, b"abc", b","), 1);
}

// ── Stress: split many times with GC pressure ─────────────────────────────

#[test]
fn string_split_stress() {
    // Split a 200-char string with 50 delimiters (51 parts). Repeat many times
    // and verify each iteration produces the expected parts.
    let mut ctx = StrCtx::new();
    static BIG: &[u8] = b"0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50";
    let s = ctx.intern(BIG);
    let delim = ctx.intern(b",");
    for _ in 0..20 {
        let result = ctx
            .dispatch(m::split, d::String__aString, &[s, delim])
            .unwrap()
            .unwrap();
        let Value::ArrayRef(arr) = result else {
            panic!("expected ArrayRef");
        };
        assert_eq!(ctx.arrays.length(arr), Some(51));
    }
}

// ── String.format ─────────────────────────────────────────────────────────

impl StrCtx {
    /// Build an Object[] from a slice of Values, using the REF_TAG encoding
    /// that anewarray/aastore produces in real bytecode.
    fn make_args(&mut self, vals: &[Value]) -> Value {
        let arr = self
            .arrays
            .alloc(crate::array_heap::ATYPE_REF, vals.len() as u16)
            .unwrap();
        for (i, v) in vals.iter().enumerate() {
            let raw = crate::array_heap::encode_ref(*v)
                .expect("make_args only accepts Null / Reference / ObjectRef");
            self.arrays.store(arr, i, raw);
        }
        Value::ArrayRef(arr)
    }

    /// Box a primitive Value into the named wrapper class and return the ObjectRef.
    fn box_primitive(&mut self, class: &'static str, v: Value) -> Value {
        let idx = self.objects.alloc(class).unwrap();
        self.objects.set_field(idx, 0, v);
        Value::ObjectRef(idx)
    }

    /// Convenience: call format("...", new Object[]{...}) and return the &str.
    fn fmt(&mut self, fmt: &'static [u8], args: &[Value]) -> alloc::string::String {
        let fmt_ref = self.intern(fmt);
        let arr = self.make_args(args);
        let result = self
            .dispatch(m::format, d::String_aObject__String, &[fmt_ref, arr])
            .unwrap()
            .unwrap();
        let Value::Reference(idx) = result else {
            panic!("expected Reference, got {result:?}");
        };
        self.strings.resolve(idx).unwrap_or("").into()
    }
}

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

// ── Random native method tests ───────────────────────────────────────────

struct RngCtx {
    strings: StringTable,
    objects: ObjectHeap,
    arrays: ArrayHeap,
    this_idx: u16,
}

impl RngCtx {
    fn new(seed: i64) -> Self {
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let this_idx = objects.alloc(c::java_util_Random).unwrap();
        // Seed via the native <init>(J) so behavior matches a real instance.
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: "(J)V",
            args: &[Value::ObjectRef(this_idx), Value::Long(seed)],
            strings: &mut strings,
            objects: &mut objects,
            arrays: &mut arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_Random, "<init>", &mut ctx)
            .expect("Random.<init>(J) not handled")
            .expect("Random.<init>(J) returned error");
        Self {
            strings,
            objects,
            arrays,
            this_idx,
        }
    }

    fn call(&mut self, method: &str, desc: &str, extra: &[Value]) -> Option<Value> {
        let mut args: alloc::vec::Vec<Value> = alloc::vec![Value::ObjectRef(self.this_idx)];
        args.extend_from_slice(extra);
        let mut ctx = NativeContext {
            classes: &[],
            descriptor: desc,
            args: &args,
            strings: &mut self.strings,
            objects: &mut self.objects,
            arrays: &mut self.arrays,
            upcall: None,
        };
        BuiltinHandler
            .dispatch(c::java_util_Random, method, &mut ctx)
            .expect("Random method not handled")
            .expect("Random method returned error")
    }
}

#[test]
fn random_seed_determinism_int() {
    let mut a = RngCtx::new(42);
    let mut b = RngCtx::new(42);
    for _ in 0..16 {
        assert_eq!(
            a.call(m::nextInt, "()I", &[]),
            b.call(m::nextInt, "()I", &[])
        );
    }
}

#[test]
fn random_seed_determinism_long() {
    let mut a = RngCtx::new(0xCAFE_BABEi64);
    let mut b = RngCtx::new(0xCAFE_BABEi64);
    for _ in 0..16 {
        assert_eq!(
            a.call(m::nextLong, "()J", &[]),
            b.call(m::nextLong, "()J", &[])
        );
    }
}

#[test]
fn random_setseed_resets_sequence() {
    let mut r = RngCtx::new(7);
    let first = r.call(m::nextInt, "()I", &[]);
    // Re-seed with the same value; next draw must match the first draw.
    r.call(m::setSeed, "(J)V", &[Value::Long(7)]);
    assert_eq!(r.call(m::nextInt, "()I", &[]), first);
}

#[test]
fn random_next_int_bound_in_range() {
    let mut r = RngCtx::new(123);
    for _ in 0..256 {
        let v = r.call(m::nextInt, "(I)I", &[Value::Int(10)]);
        match v {
            Some(Value::Int(n)) => assert!((0..10).contains(&n), "out of range: {n}"),
            other => panic!("expected Int, got {other:?}"),
        }
    }
}

#[test]
fn random_next_int_bound_power_of_two() {
    // Exercises the JDK's bound-is-power-of-2 fast path.
    let mut r = RngCtx::new(99);
    for _ in 0..256 {
        let v = r.call(m::nextInt, "(I)I", &[Value::Int(64)]);
        match v {
            Some(Value::Int(n)) => assert!((0..64).contains(&n), "out of range: {n}"),
            other => panic!("expected Int, got {other:?}"),
        }
    }
}

#[test]
fn random_next_float_in_unit_interval() {
    let mut r = RngCtx::new(1);
    for _ in 0..64 {
        match r.call(m::nextFloat, "()F", &[]) {
            Some(Value::Float(f)) => assert!((0.0..1.0).contains(&f), "out of [0,1): {f}"),
            other => panic!("expected Float, got {other:?}"),
        }
    }
}

#[test]
fn random_next_double_in_unit_interval() {
    let mut r = RngCtx::new(2);
    for _ in 0..64 {
        match r.call(m::nextDouble, "()D", &[]) {
            Some(Value::Double(d)) => assert!((0.0..1.0).contains(&d), "out of [0,1): {d}"),
            other => panic!("expected Double, got {other:?}"),
        }
    }
}

#[test]
fn random_next_boolean_yields_both_values() {
    let mut r = RngCtx::new(3);
    let mut saw_true = false;
    let mut saw_false = false;
    for _ in 0..64 {
        match r.call(m::nextBoolean, "()Z", &[]) {
            Some(Value::Int(0)) => saw_false = true,
            Some(Value::Int(1)) => saw_true = true,
            other => panic!("expected boolean Int, got {other:?}"),
        }
    }
    assert!(
        saw_true && saw_false,
        "boolean RNG biased: t={saw_true} f={saw_false}"
    );
}

#[test]
fn random_next_gaussian_distribution_sanity() {
    // Marsaglia polar with 256 samples — mean within ±0.3, stddev within ±0.3 of 1.
    let mut r = RngCtx::new(0xDEAD_BEEFi64);
    let n = 256usize;
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    for _ in 0..n {
        match r.call(m::nextGaussian, "()D", &[]) {
            Some(Value::Double(d)) => {
                sum += d;
                sum_sq += d * d;
            }
            other => panic!("expected Double, got {other:?}"),
        }
    }
    let mean = sum / n as f64;
    let variance = sum_sq / n as f64 - mean * mean;
    let stddev = libm::sqrt(variance);
    assert!(libm::fabs(mean) < 0.3, "mean too far from 0: {mean}");
    assert!(
        libm::fabs(stddev - 1.0) < 0.3,
        "stddev too far from 1: {stddev}"
    );
}

#[test]
fn random_next_bytes_fills_array() {
    use crate::array_heap::ATYPE_BYTE;
    let mut r = RngCtx::new(11);
    let arr_idx = r.arrays.alloc(ATYPE_BYTE, 16).unwrap();
    r.call(m::nextBytes, "([B)V", &[Value::ArrayRef(arr_idx)]);
    // At least one slot should be non-zero (probability of all-zeros is 2^-128).
    let mut any_nonzero = false;
    for i in 0..16 {
        if r.arrays.load(arr_idx, i).unwrap() != 0 {
            any_nonzero = true;
            break;
        }
    }
    assert!(any_nonzero, "nextBytes left the array all zeros");
}

#[test]
fn random_next_bytes_partial_tail() {
    use crate::array_heap::ATYPE_BYTE;
    // Length not a multiple of 4 — exercises the inner-loop tail.
    let mut r = RngCtx::new(13);
    let arr_idx = r.arrays.alloc(ATYPE_BYTE, 7).unwrap();
    r.call(m::nextBytes, "([B)V", &[Value::ArrayRef(arr_idx)]);
    // Length must be unchanged (no overrun).
    assert_eq!(r.arrays.length(arr_idx), Some(7));
}

// ── Arrays native method tests ───────────────────────────────────────────

fn arrays_dispatch(
    method: &str,
    desc: &str,
    args: &[Value],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings,
        objects,
        arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_Arrays, method, &mut ctx)
        .expect("Arrays method not handled")
}

fn make_int_array(arrays: &mut ArrayHeap, vs: &[i32]) -> u16 {
    use crate::array_heap::ATYPE_INT;
    let idx = arrays.alloc(ATYPE_INT, vs.len() as u16).unwrap();
    for (i, v) in vs.iter().enumerate() {
        arrays.store(idx, i, *v).unwrap();
    }
    idx
}

fn read_int_array(arrays: &ArrayHeap, idx: u16) -> alloc::vec::Vec<i32> {
    let len = arrays.length(idx).unwrap() as usize;
    (0..len).map(|i| arrays.load(idx, i).unwrap()).collect()
}

#[test]
fn arrays_sort_int_random() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[5, 3, 8, 1, 9, 2, 7, 4, 6]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(
        read_int_array(&arrays, idx),
        alloc::vec![1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
}

#[test]
fn arrays_sort_int_already_sorted() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3, 4, 5]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, idx), alloc::vec![1, 2, 3, 4, 5]);
}

#[test]
fn arrays_sort_int_large_uses_quicksort_path() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    // Length > INSERTION_THRESHOLD to exercise the sort_unstable code path.
    let mut vs: alloc::vec::Vec<i32> = (0..32).rev().collect();
    let idx = make_int_array(&mut arrays, &vs);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    vs.sort();
    assert_eq!(read_int_array(&arrays, idx), vs);
}

#[test]
fn arrays_sort_int_empty_and_single_no_op() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let empty = make_int_array(&mut arrays, &[]);
    let single = make_int_array(&mut arrays, &[42]);
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(empty)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    arrays_dispatch(
        m::sort,
        "([I)V",
        &[Value::ArrayRef(single)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(arrays.length(empty), Some(0));
    assert_eq!(read_int_array(&arrays, single), alloc::vec![42]);
}

#[test]
fn arrays_sort_long() {
    use crate::array_heap::ATYPE_LONG;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, 5).unwrap();
    for (i, v) in [3i64, -10, 0, 7, -1].iter().enumerate() {
        arrays.store64(idx, i, *v).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([J)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i64> = (0..5).map(|i| arrays.load64(idx, i).unwrap()).collect();
    assert_eq!(got, alloc::vec![-10, -1, 0, 3, 7]);
}

#[test]
fn arrays_sort_double_with_nan_total_cmp() {
    use crate::array_heap::ATYPE_DOUBLE;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_DOUBLE, 4).unwrap();
    let input = [f64::NAN, 1.0, -2.5, 3.5];
    for (i, v) in input.iter().enumerate() {
        arrays.store64(idx, i, v.to_bits() as i64).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([D)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<f64> = (0..4)
        .map(|i| f64::from_bits(arrays.load64(idx, i).unwrap() as u64))
        .collect();
    // total_cmp sorts NaN last.
    assert_eq!(got[0], -2.5);
    assert_eq!(got[1], 1.0);
    assert_eq!(got[2], 3.5);
    assert!(got[3].is_nan());
}

#[test]
fn arrays_sort_byte_sign_extends() {
    use crate::array_heap::ATYPE_BYTE;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_BYTE, 4).unwrap();
    // Stored as i32, but logically byte: -1 (0xFF) must sort BELOW 1.
    arrays.store(idx, 0, 1).unwrap();
    arrays.store(idx, 1, -1).unwrap();
    arrays.store(idx, 2, 0).unwrap();
    arrays.store(idx, 3, -128).unwrap();
    arrays_dispatch(
        m::sort,
        "([B)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i32> = (0..4).map(|i| arrays.load(idx, i).unwrap()).collect();
    assert_eq!(got, alloc::vec![-128, -1, 0, 1]);
}

fn arrays_to_string_str(
    desc: &str,
    arr: u16,
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
) -> alloc::string::String {
    let v = arrays_dispatch(
        m::toString,
        desc,
        &[Value::ArrayRef(arr)],
        strings,
        objects,
        arrays,
    )
    .unwrap()
    .unwrap();
    let Value::Reference(idx) = v else {
        panic!("expected Reference, got {v:?}");
    };
    strings.resolve(idx).unwrap().into()
}

#[test]
fn arrays_fill_and_to_string_boolean_and_char() {
    // boolean[] had no arm at all (hard InvalidReference); char[] printed
    // code points ("[97, 98]") instead of the characters.
    use crate::array_heap::{ATYPE_BOOLEAN, ATYPE_CHAR};
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let b = arrays.alloc(ATYPE_BOOLEAN, 3).unwrap();
    arrays_dispatch(
        m::fill,
        "([ZZ)V",
        &[Value::ArrayRef(b), Value::Int(1)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, b), alloc::vec![1, 1, 1]);
    arrays.store(b, 1, 0);
    assert_eq!(
        arrays_to_string_str(d::aZ__String, b, &mut strings, &mut objects, &mut arrays),
        "[true, false, true]"
    );
    let c = arrays.alloc(ATYPE_CHAR, 2).unwrap();
    arrays.store(c, 0, b'a' as i32);
    arrays.store(c, 1, b'b' as i32);
    assert_eq!(
        arrays_to_string_str(d::aC__String, c, &mut strings, &mut objects, &mut arrays),
        "[a, b]"
    );
}

#[test]
fn arrays_fill_and_sort_range_overloads() {
    // fill(a, from, to, v) used to read `from` as the value and fill the
    // whole array; sort(a, from, to) sorted everything.
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let a = make_int_array(&mut arrays, &[0, 0, 0, 0, 0]);
    arrays_dispatch(
        m::fill,
        "([IIII)V",
        &[
            Value::ArrayRef(a),
            Value::Int(1),
            Value::Int(3),
            Value::Int(9),
        ],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, a), alloc::vec![0, 9, 9, 0, 0]);

    let s = make_int_array(&mut arrays, &[5, 4, 3, 2, 1]);
    arrays_dispatch(
        m::sort,
        "([III)V",
        &[Value::ArrayRef(s), Value::Int(1), Value::Int(4)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, s), alloc::vec![5, 2, 3, 4, 1]);

    // Bad ranges throw like Java: to > length -> AIOOBE, from > to -> IAE.
    let r = arrays_dispatch(
        m::fill,
        "([IIII)V",
        &[
            Value::ArrayRef(a),
            Value::Int(1),
            Value::Int(9),
            Value::Int(0),
        ],
        &mut strings,
        &mut objects,
        &mut arrays,
    );
    let Err(JvmError::Exception(idx)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(idx),
        Some(c::java_lang_ArrayIndexOutOfBoundsException)
    );
    let r = arrays_dispatch(
        m::sort,
        "([III)V",
        &[Value::ArrayRef(a), Value::Int(3), Value::Int(1)],
        &mut strings,
        &mut objects,
        &mut arrays,
    );
    let Err(JvmError::Exception(idx)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(idx),
        Some(c::java_lang_IllegalArgumentException)
    );
}

#[test]
fn arrays_fill_object_array() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let arr = arrays.alloc(crate::array_heap::ATYPE_REF, 2).unwrap();
    let s = Value::Reference(strings.intern(m::x.as_bytes()).unwrap());
    arrays_dispatch(
        m::fill,
        d::aObject_Object__V,
        &[Value::ArrayRef(arr), s],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(
        crate::array_heap::decode_ref(arrays.load(arr, 1).unwrap()),
        s
    );
}

#[test]
fn arrays_fill_int() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[0, 0, 0, 0, 0]);
    arrays_dispatch(
        m::fill,
        "([II)V",
        &[Value::ArrayRef(idx), Value::Int(7)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    assert_eq!(read_int_array(&arrays, idx), alloc::vec![7; 5]);
}

#[test]
fn arrays_fill_long() {
    use crate::array_heap::ATYPE_LONG;
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, 4).unwrap();
    arrays_dispatch(
        m::fill,
        "([JJ)V",
        &[Value::ArrayRef(idx), Value::Long(0xCAFE_BABE)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    for i in 0..4 {
        assert_eq!(arrays.load64(idx, i), Some(0xCAFE_BABE));
    }
}

#[test]
fn arrays_copy_of_grow_zero_pads() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3]);
    let result = arrays_dispatch(
        m::copyOf,
        "([II)[I",
        &[Value::ArrayRef(idx), Value::Int(5)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let new_idx = match result {
        Value::ArrayRef(i) => i,
        _ => panic!("expected ArrayRef"),
    };
    assert_eq!(read_int_array(&arrays, new_idx), alloc::vec![1, 2, 3, 0, 0]);
}

#[test]
fn arrays_copy_of_shrink_truncates() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3, 4, 5]);
    let result = arrays_dispatch(
        m::copyOf,
        "([II)[I",
        &[Value::ArrayRef(idx), Value::Int(2)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let new_idx = match result {
        Value::ArrayRef(i) => i,
        _ => panic!("expected ArrayRef"),
    };
    assert_eq!(read_int_array(&arrays, new_idx), alloc::vec![1, 2]);
}

#[test]
fn arrays_to_string_int() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[1, 2, 3]);
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "[1, 2, 3]");
}

#[test]
fn arrays_to_string_empty() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = make_int_array(&mut arrays, &[]);
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "[]");
}

#[test]
fn arrays_to_string_null() {
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let result = arrays_dispatch(
        m::toString,
        d::aI__String,
        &[Value::Null],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap()
    .unwrap();
    let s = match result {
        Value::Reference(i) => strings.resolve(i).unwrap(),
        _ => panic!("expected Reference"),
    };
    assert_eq!(s, "null");
}

// ── Single-source-of-truth invariant ──────────────────────────────────────

/// Every class with a per-class entry in `BUILTIN_DISPATCH` must also appear in
/// `BUILTIN_CLASS_NAMES`. Without this, a class would dispatch correctly once
/// but fail virtual dispatch on subclasses because the interpreter could not
/// canonicalise its name to a stable `&'static str`.
#[test]
fn builtin_dispatch_classes_subset_of_names() {
    for &(dispatch_name, _hash, _fn) in BUILTIN_DISPATCH {
        assert!(
            BUILTIN_CLASS_NAMES.iter().any(|&n| n == dispatch_name),
            "class {dispatch_name:?} appears in BUILTIN_DISPATCH but is missing from BUILTIN_CLASS_NAMES"
        );
    }
}

/// `BUILTIN_METHODS` is keyed by exactly the classes `BUILTIN_DISPATCH`
/// serves — a dispatcher without rows would make the generated contract
/// reject every use of that class, and rows for a class nothing dispatches
/// would admit calls that die at run time.
#[test]
fn builtin_methods_cover_every_dispatch_class() {
    for &(name, _hash, _fn) in BUILTIN_DISPATCH {
        assert!(
            BUILTIN_METHODS.iter().any(|(c, _)| *c == name),
            "class {name:?} is in BUILTIN_DISPATCH but has no BUILTIN_METHODS entry"
        );
    }
    for &(name, rows) in BUILTIN_METHODS {
        assert!(
            BUILTIN_DISPATCH.iter().any(|(c, _, _)| *c == name),
            "class {name:?} has BUILTIN_METHODS rows but no BUILTIN_DISPATCH entry"
        );
        assert!(
            BUILTIN_CLASS_NAMES.contains(&name),
            "class {name:?} in BUILTIN_METHODS is missing from BUILTIN_CLASS_NAMES"
        );
        assert!(
            !rows.is_empty(),
            "class {name:?} has an empty BUILTIN_METHODS list"
        );
        for (i, (method, descs)) in rows.iter().enumerate() {
            assert!(
                !rows[..i].iter().any(|(m, _)| m == method),
                "{name}.{method} is listed twice in BUILTIN_METHODS"
            );
            for d in descs.iter() {
                assert!(
                    d.starts_with('(') && d.contains(')') && !d.ends_with(')'),
                    "{name}.{method}: {d:?} is not a JVM method descriptor"
                );
                if *method == "<init>" {
                    assert!(d.ends_with(")V"), "{name}.<init>: {d:?} must return void");
                }
            }
        }
    }
}

/// `BUILTIN_INTERFACE_METHODS` names interfaces the JVM canonicalises and
/// does not dispatch itself (their members resolve on the implementor).
#[test]
fn builtin_interface_methods_name_known_interfaces() {
    for &(iface, rows) in BUILTIN_INTERFACE_METHODS {
        assert!(
            BUILTIN_CLASS_NAMES.contains(&iface),
            "interface {iface:?} in BUILTIN_INTERFACE_METHODS is missing from BUILTIN_CLASS_NAMES"
        );
        assert!(
            !BUILTIN_DISPATCH.iter().any(|(c, _, _)| *c == iface),
            "{iface:?} has a dispatcher; list its methods in BUILTIN_METHODS instead"
        );
        assert!(!rows.is_empty(), "interface {iface:?} has no rows");
        for (method, descs) in rows.iter() {
            assert!(
                !descs.is_empty(),
                "{iface}.{method}: interface members are descriptor-exact"
            );
            for d in descs.iter() {
                assert!(
                    d.starts_with('(') && d.contains(')') && !d.ends_with(')'),
                    "{iface}.{method}: {d:?} is not a JVM method descriptor"
                );
            }
        }
    }
}

/// The dispatcher source a class's rows are matched against.
fn dispatcher_source(class: &str) -> &'static str {
    match class {
        c::java_lang_String => include_str!("string.rs"),
        c::java_lang_StringBuilder => include_str!("string_builder.rs"),
        c::java_util_ArrayList => include_str!("collections.rs"),
        c::java_util_HashMap
        | c::java_util_LinkedHashMap
        | c::java_util_HashMap_KeySet
        | c::java_util_HashMap_Values
        | c::java_util_HashMap_EntrySet
        | c::java_util_Map_Entry => include_str!("hashmap.rs"),
        c::java_util_HashSet | c::java_util_LinkedHashSet => include_str!("hashset.rs"),
        c::java_util_Iterator => include_str!("iterator.rs"),
        c::java_util_Random => include_str!("random.rs"),
        c::java_lang_Enum => include_str!("enumeration.rs"),
        c::java_lang_Class => include_str!("class_obj.rs"),
        c::java_lang_Math => include_str!("math.rs"),
        c::java_util_Arrays | c::java_lang_System => include_str!("arrays.rs"),
        c::java_lang_Integer
        | c::java_lang_Boolean
        | c::java_lang_Long
        | c::java_lang_Float
        | c::java_lang_Double
        | c::java_lang_Character
        | c::java_lang_Byte
        | c::java_lang_Short => include_str!("boxed.rs"),
        // Object and the Throwable family are dispatched from this module.
        _ => include_str!("mod.rs"),
    }
}

/// Direction B of the builtin method table: every row names an arm that
/// exists. Text-level — the name must appear as a string literal in the
/// dispatcher's source — which is enough to catch a misspelt or stale row
/// without building a receiver per class. The reverse direction (an arm
/// with no row) is not checked here; it surfaces as a contract failure.
#[test]
fn builtin_method_rows_name_real_arms() {
    let interpreter = include_str!("../interpreter/ops_invoke.rs");
    let mut missing = alloc::vec::Vec::new();
    for &(class, rows) in BUILTIN_METHODS {
        let source = dispatcher_source(class);
        for (method, _) in rows {
            // Arms match through `m::<name>` (never a literal); `<init>` is the
            // one name with no const.
            let original = crate::names::unshrink_member(method);
            let literal = if original.starts_with('<') {
                alloc::format!("\"{original}\"")
            } else {
                alloc::format!("m::{original}")
            };
            let served = match (class, *method) {
                // Resolved by the interpreter before dispatch.
                (c::java_lang_Object, m::getClass) | (c::java_util_ArrayList, m::sort) => {
                    interpreter.contains(&literal)
                }
                _ => source.contains(&literal),
            };
            if !served {
                missing.push(alloc::format!("{class}.{method}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "BUILTIN_METHODS rows with no matching string literal in their dispatcher \
         (stale row or typo): {missing:?}"
    );
}

/// Every primitive sort funnels through one `u64`-key sort (see
/// `native::arrays`), so the float key transforms have to reproduce
/// `total_cmp` exactly — including the cases that make a naive bitwise
/// comparison wrong: `-0.0` below `+0.0`, negative values running backwards,
/// and signed NaNs at the two ends.
#[test]
fn arrays_sort_float_matches_total_cmp() {
    use crate::array_heap::ATYPE_FLOAT;
    let edges: [f32; 11] = [
        f32::NAN,
        -f32::NAN,
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1.5,
        -1.5,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        42.0,
    ];
    // Run both the insertion-sort path (< INSERTION_THRESHOLD) and the
    // quicksort path by padding the same edge values out past the threshold.
    for reps in [1usize, 3] {
        let input: alloc::vec::Vec<f32> = (0..reps).flat_map(|_| edges.iter().copied()).collect();
        let len = input.len();
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let idx = arrays.alloc(ATYPE_FLOAT, len as u16).unwrap();
        for (i, v) in input.iter().enumerate() {
            arrays.store(idx, i, v.to_bits() as i32).unwrap();
        }
        arrays_dispatch(
            m::sort,
            "([F)V",
            &[Value::ArrayRef(idx)],
            &mut strings,
            &mut objects,
            &mut arrays,
        )
        .unwrap();
        let got: alloc::vec::Vec<u32> = (0..len)
            .map(|i| arrays.load(idx, i).unwrap() as u32)
            .collect();
        let mut want = input.clone();
        want.sort_by(f32::total_cmp);
        let want: alloc::vec::Vec<u32> = want.iter().map(|v| v.to_bits()).collect();
        // Compare bit patterns, not values: NaN != NaN, and -0.0 == 0.0.
        assert_eq!(got, want, "f32 sort diverged from total_cmp (reps={reps})");
    }
}

#[test]
fn arrays_sort_double_matches_total_cmp() {
    use crate::array_heap::ATYPE_DOUBLE;
    let edges: [f64; 11] = [
        f64::NAN,
        -f64::NAN,
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1.5,
        -1.5,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        42.0,
    ];
    for reps in [1usize, 3] {
        let input: alloc::vec::Vec<f64> = (0..reps).flat_map(|_| edges.iter().copied()).collect();
        let len = input.len();
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let idx = arrays.alloc(ATYPE_DOUBLE, len as u16).unwrap();
        for (i, v) in input.iter().enumerate() {
            arrays.store64(idx, i, v.to_bits() as i64).unwrap();
        }
        arrays_dispatch(
            m::sort,
            "([D)V",
            &[Value::ArrayRef(idx)],
            &mut strings,
            &mut objects,
            &mut arrays,
        )
        .unwrap();
        let got: alloc::vec::Vec<u64> = (0..len)
            .map(|i| arrays.load64(idx, i).unwrap() as u64)
            .collect();
        let mut want = input.clone();
        want.sort_by(f64::total_cmp);
        let want: alloc::vec::Vec<u64> = want.iter().map(|v| v.to_bits()).collect();
        assert_eq!(got, want, "f64 sort diverged from total_cmp (reps={reps})");
    }
}

/// The i64 key transform has to keep negatives below positives across the
/// sign boundary, including the extremes where a sign-bit flip is easy to
/// get wrong.
#[test]
fn arrays_sort_long_spans_sign_boundary() {
    use crate::array_heap::ATYPE_LONG;
    let input: [i64; 8] = [i64::MAX, -1, 0, i64::MIN, 1, -2, i64::MIN + 1, i64::MAX - 1];
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = ArrayHeap::new();
    let idx = arrays.alloc(ATYPE_LONG, input.len() as u16).unwrap();
    for (i, v) in input.iter().enumerate() {
        arrays.store64(idx, i, *v).unwrap();
    }
    arrays_dispatch(
        m::sort,
        "([J)V",
        &[Value::ArrayRef(idx)],
        &mut strings,
        &mut objects,
        &mut arrays,
    )
    .unwrap();
    let got: alloc::vec::Vec<i64> = (0..input.len())
        .map(|i| arrays.load64(idx, i).unwrap())
        .collect();
    let mut want = input.to_vec();
    want.sort_unstable();
    assert_eq!(got, want);
}

// ── Boxed value/identity surface, Object identity, Enum.valueOf ────────────
//
// The Java 8 wrapper API that Kotlin data classes (`Float.hashCode(F)`,
// `Float.compare(FF)`), `Intrinsics.areEqual` and `compareBy` lean on.

fn dispatch_on(
    cx: &mut StrCtx,
    class: &str,
    method: &str,
    desc: &str,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: desc,
        args,
        strings: &mut cx.strings,
        objects: &mut cx.objects,
        arrays: &mut cx.arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(class, method, &mut ctx)
        .unwrap_or_else(|| panic!("{class}.{method} not handled"))
}

fn boxed(cx: &mut StrCtx, class: &'static str, v: Value) -> Value {
    let idx = cx.objects.alloc(class).unwrap();
    cx.objects.set_field(idx, 0, v);
    Value::ObjectRef(idx)
}

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

// ── entrySet / views / LinkedHash* aliases / toArray / append(null) ────────

const OBJ_DESC: &str = d::__Object;

fn new_map(cx: &mut StrCtx, class: &'static str) -> Value {
    let map = Value::ObjectRef(cx.objects.alloc(class).unwrap());
    dispatch_on(cx, class, "<init>", "()V", &[map]).unwrap();
    map
}

#[test]
fn entry_set_iterates_key_value_pairs() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_HashMap);
    let k1 = cx.intern(b"one");
    let k2 = cx.intern(b"two");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k1, Value::Int(1)],
    )
    .unwrap();
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k2, Value::Int(2)],
    )
    .unwrap();

    let view = dispatch_on(&mut cx, c::java_util_HashMap, m::entrySet, d::__Set, &[map])
        .unwrap()
        .unwrap();
    let Value::ObjectRef(vi) = view else {
        panic!("entrySet returned {view:?}");
    };
    assert_eq!(
        cx.objects.class_name(vi),
        Some(c::java_util_HashMap_EntrySet)
    );
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_HashMap_EntrySet,
            m::size,
            "()I",
            &[view]
        )
        .unwrap(),
        Some(Value::Int(2))
    );
    let it = dispatch_on(
        &mut cx,
        c::java_util_HashMap_EntrySet,
        m::iterator,
        d::__Iterator,
        &[view],
    )
    .unwrap()
    .unwrap();

    let mut pairs: alloc::vec::Vec<(Value, Value)> = alloc::vec::Vec::new();
    while dispatch_on(&mut cx, c::java_util_Iterator, m::hasNext, "()Z", &[it]).unwrap()
        == Some(Value::Int(1))
    {
        let e = dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it])
            .unwrap()
            .unwrap();
        let Value::ObjectRef(ei) = e else {
            panic!("next returned {e:?}");
        };
        assert_eq!(cx.objects.class_name(ei), Some(c::java_util_Map_Entry));
        let k = dispatch_on(&mut cx, c::java_util_Map_Entry, m::getKey, OBJ_DESC, &[e])
            .unwrap()
            .unwrap();
        let v = dispatch_on(&mut cx, c::java_util_Map_Entry, m::getValue, OBJ_DESC, &[e])
            .unwrap()
            .unwrap();
        pairs.push((k, v));
    }
    assert_eq!(pairs.len(), 2);
    assert!(pairs.contains(&(k1, Value::Int(1))));
    assert!(pairs.contains(&(k2, Value::Int(2))));
    // Past the end.
    assert!(dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it]).is_err());
}

#[test]
fn key_set_and_values_views_pick_their_source_by_class() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_HashMap);
    let k = cx.intern(b"k");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_HashMap,
        m::put,
        put,
        &[map, k, Value::Int(9)],
    )
    .unwrap();
    for (method, class, expect) in [
        (m::keySet, c::java_util_HashMap_KeySet, k),
        (m::values, c::java_util_HashMap_Values, Value::Int(9)),
    ] {
        let view = dispatch_on(&mut cx, c::java_util_HashMap, method, d::__Set, &[map])
            .unwrap()
            .unwrap();
        let it = dispatch_on(&mut cx, class, m::iterator, d::__Iterator, &[view])
            .unwrap()
            .unwrap();
        assert_eq!(
            dispatch_on(&mut cx, c::java_util_Iterator, m::next, OBJ_DESC, &[it]).unwrap(),
            Some(expect)
        );
        assert_eq!(
            dispatch_on(&mut cx, class, m::size, "()I", &[view]).unwrap(),
            Some(Value::Int(1))
        );
    }
}

#[test]
fn linked_hash_map_and_set_alias_the_hash_dispatchers() {
    let mut cx = StrCtx::new();
    let map = new_map(&mut cx, c::java_util_LinkedHashMap);
    let k = cx.intern(b"k");
    let put = d::Object_Object__Object;
    dispatch_on(
        &mut cx,
        c::java_util_LinkedHashMap,
        m::put,
        put,
        &[map, k, Value::Int(5)],
    )
    .unwrap();
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_LinkedHashMap,
            m::get,
            d::Object__Object,
            &[map, k]
        )
        .unwrap(),
        Some(Value::Int(5))
    );
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashMap, m::size, "()I", &[map]).unwrap(),
        Some(Value::Int(1))
    );

    let set = new_map(&mut cx, c::java_util_LinkedHashSet);
    let add = d::Object__Z;
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashSet, m::add, add, &[set, k]).unwrap(),
        Some(Value::Int(1))
    );
    assert_eq!(
        dispatch_on(&mut cx, c::java_util_LinkedHashSet, m::add, add, &[set, k]).unwrap(),
        Some(Value::Int(0))
    );
    assert_eq!(
        dispatch_on(
            &mut cx,
            c::java_util_LinkedHashSet,
            m::contains,
            add,
            &[set, k]
        )
        .unwrap(),
        Some(Value::Int(1))
    );
}

#[test]
fn to_array_copies_every_reference_kind() {
    let mut cx = StrCtx::new();
    let list = Value::ObjectRef(cx.objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_on(&mut cx, c::java_util_ArrayList, "<init>", "()V", &[list]).unwrap();
    let s = cx.intern(b"s");
    let o = Value::ObjectRef(cx.objects.alloc("O").unwrap());
    let arr = Value::ArrayRef(cx.arrays.alloc(crate::array_heap::ATYPE_INT, 1).unwrap());
    for v in [s, o, Value::Null, arr] {
        dispatch_on(
            &mut cx,
            c::java_util_ArrayList,
            m::add,
            d::Object__Z,
            &[list, v],
        )
        .unwrap();
    }
    let out = dispatch_on(
        &mut cx,
        c::java_util_ArrayList,
        m::toArray,
        d::aObject__aObject,
        &[list, Value::Null],
    )
    .unwrap()
    .unwrap();
    let Value::ArrayRef(ai) = out else {
        panic!("toArray returned {out:?}");
    };
    assert_eq!(cx.arrays.atype(ai), Some(crate::array_heap::ATYPE_REF));
    assert_eq!(cx.arrays.length(ai), Some(4));
    for (i, v) in [s, o, Value::Null, arr].into_iter().enumerate() {
        assert_eq!(
            crate::array_heap::decode_ref(cx.arrays.load(ai, i).unwrap()),
            v
        );
    }
}

#[test]
fn append_null_and_value_of_object_on_string_or_null() {
    let mut cx = StrCtx::new();
    let sb = Value::ObjectRef(cx.objects.alloc(c::java_lang_StringBuilder).unwrap());
    dispatch_on(&mut cx, c::java_lang_StringBuilder, "<init>", "()V", &[sb]).unwrap();
    dispatch_on(
        &mut cx,
        c::java_lang_StringBuilder,
        m::append,
        d::Object__StringBuilder,
        &[sb, Value::Null],
    )
    .unwrap();
    let s = dispatch_on(
        &mut cx,
        c::java_lang_StringBuilder,
        m::toString,
        d::__String,
        &[sb],
    )
    .unwrap()
    .unwrap();
    assert_eq!(cx.resolve(s), "null");

    let ab = cx.intern(b"ab");
    let desc = d::Object__String;
    assert_eq!(
        dispatch_on(&mut cx, c::java_lang_String, m::valueOf, desc, &[ab]).unwrap(),
        Some(ab)
    );
    let n = dispatch_on(
        &mut cx,
        c::java_lang_String,
        m::valueOf,
        desc,
        &[Value::Null],
    )
    .unwrap()
    .unwrap();
    assert_eq!(cx.resolve(n), "null");
}

// ── bugbash S6: Iterator.remove and fail-fast iteration ───────────────────

#[test]
fn iterator_remove_removes_the_last_returned_element() {
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [10, 20, 30] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let iter = make_list_iterator(&mut objects, list);
    // remove() before next() is IllegalStateException.
    let r = dispatch_iter(m::remove, "()V", &[iter], &mut objects);
    let Err(JvmError::Exception(e)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(e),
        Some(c::java_lang_IllegalStateException)
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(10)))
    );
    dispatch_iter(m::remove, "()V", &[iter], &mut objects).unwrap();
    assert_eq!(
        dispatch_list(m::size, "()I", &[list], &mut objects),
        Ok(Some(Value::Int(2)))
    );
    // Iteration continues over the survivors.
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(20)))
    );
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(30)))
    );
    assert_eq!(
        dispatch_iter(m::hasNext, "()Z", &[iter], &mut objects),
        Ok(Some(Value::Int(0)))
    );
}

#[test]
fn iterator_detects_concurrent_modification() {
    // Removing through the collection mid-iteration used to silently skip
    // every other element; java.util fails fast in next().
    let mut objects = ObjectHeap::new();
    let list = Value::ObjectRef(objects.alloc(c::java_util_ArrayList).unwrap());
    dispatch_list("<init>", "()V", &[list], &mut objects).unwrap();
    for v in [1, 2, 3, 4] {
        dispatch_list(m::add, d::Object__Z, &[list, Value::Int(v)], &mut objects).unwrap();
    }
    let iter = make_list_iterator(&mut objects, list);
    assert_eq!(
        dispatch_iter(m::next, d::__Object, &[iter], &mut objects),
        Ok(Some(Value::Int(1)))
    );
    dispatch_list(
        m::remove,
        d::I__Object,
        &[list, Value::Int(0)],
        &mut objects,
    )
    .unwrap();
    let r = dispatch_iter(m::next, d::__Object, &[iter], &mut objects);
    let Err(JvmError::Exception(e)) = r else {
        panic!("{r:?}");
    };
    assert_eq!(
        objects.class_name(e),
        Some(c::java_util_ConcurrentModificationException)
    );
}

fn make_list_iterator(objects: &mut ObjectHeap, list: Value) -> Value {
    let mut strings = StringTable::new();
    let mut arrays = ArrayHeap::new();
    let mut ctx = NativeContext {
        classes: &[],
        descriptor: d::__Iterator,
        args: &[list],
        strings: &mut strings,
        objects,
        arrays: &mut arrays,
        upcall: None,
    };
    BuiltinHandler
        .dispatch(c::java_util_ArrayList, m::iterator, &mut ctx)
        .unwrap()
        .unwrap()
        .unwrap()
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
