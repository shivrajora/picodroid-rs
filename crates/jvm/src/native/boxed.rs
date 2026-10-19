// SPDX-License-Identifier: GPL-3.0-only
use crate::object_heap::{
    double_to_str_buf, float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf,
};
use crate::types::{JvmError, Value};

use super::NativeContext;
use super::INIT;
use crate::names::{c, mh};

/// The unboxing accessors every wrapper answers (`intValue`, `booleanValue`,
/// …), as loaded. One table and one function shared by every
/// `boxed_dispatch` caller rather than eight match arms per wrapper.
const UNBOXING_ACCESSORS: [u32; 8] = [
    mh::intValue,
    mh::longValue,
    mh::floatValue,
    mh::doubleValue,
    mh::shortValue,
    mh::byteValue,
    mh::booleanValue,
    mh::charValue,
];

fn is_unboxing_accessor(method: u32) -> bool {
    UNBOXING_ACCESSORS.contains(&method)
}

/// `<init>`, `valueOf(primitive)`, the unboxing accessors and
/// [`dispatch_common`] for wrapper `class`, whose unset value reads as
/// `default`. One out-of-line copy for all eight wrappers: it was a macro
/// expanded into each `dispatch_*`, ~620 B apiece.
#[inline(never)]
fn boxed_dispatch(
    class: &'static str,
    default: Value,
    ctx: &mut NativeContext<'_>,
    method: u32,
) -> Option<Result<Option<Value>, JvmError>> {
    match method {
        INIT => {
            let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            // `new Integer("42")`: the String constructor parses as the
            // class's `parseX` does; it used to store the String
            // reference itself as the value.
            let val = if is_string_arg(ctx) {
                match ctor_value_from_string(class, ctx) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                }
            } else {
                ctx.args.get(1).copied().unwrap_or(Value::Null)
            };
            // A builtin object gets its slots on first write; a write the
            // heap cannot honour is an allocation failure, surfaced as
            // OutOfMemoryError rather than a box with no value.
            if ctx.objects.set_field(obj, 0, val).is_none() {
                return Some(Err(JvmError::StackOverflow));
            }
            Some(Ok(None))
        }
        mh::valueOf => {
            let val = ctx.args.first().copied().unwrap_or(Value::Null);
            Some(box_value(class, val, ctx))
        }
        // Unboxing accessor: intValue, booleanValue, longValue, etc.,
        // converting as Java does (`Float.valueOf(2.5f).intValue()` is 2).
        _ if is_unboxing_accessor(method) => {
            let Value::ObjectRef(obj) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let raw = ctx.objects.get_field(obj, 0).unwrap_or(default);
            Some(Ok(Some(convert_accessor(method, raw))))
        }
        _ => dispatch_common(class, method, ctx),
    }
}

/// Java's `d2i` (see `crate::fconv`).
fn f2i(d: f64) -> i32 {
    crate::fconv::d2i(d)
}

/// Java's `d2l` (see `crate::fconv`).
fn f2l(d: f64) -> i64 {
    crate::fconv::d2l(d)
}

/// The value a `xxxValue()` accessor returns for the box's stored value,
/// with Java's widening and narrowing conversions (JLS §5.1.2/§5.1.3):
/// `Float.valueOf(2.5f).intValue()` is 2, `Integer.valueOf(300).byteValue()`
/// is 44, `Long.valueOf(1L << 33).intValue()` is 0. A box's own accessor
/// hands its value back unchanged.
fn convert_accessor(method: u32, v: Value) -> Value {
    let as_i64 = |v: Value| match v {
        Value::Int(i) => i as i64,
        Value::Long(l) => l,
        Value::Float(f) => f2l(f as f64),
        Value::Double(d) => f2l(d),
        _ => 0,
    };
    let as_i32 = |v: Value| match v {
        Value::Int(i) => i,
        Value::Long(l) => l as i32,
        Value::Float(f) => f2i(f as f64),
        Value::Double(d) => f2i(d),
        _ => 0,
    };
    let as_f64 = |v: Value| match v {
        Value::Int(i) => i as f64,
        Value::Long(l) => l as f64,
        Value::Float(f) => f as f64,
        Value::Double(d) => d,
        _ => 0.0,
    };
    match method {
        mh::intValue => Value::Int(as_i32(v)),
        mh::longValue => Value::Long(as_i64(v)),
        mh::floatValue => Value::Float(match v {
            Value::Float(f) => f,
            Value::Long(l) => l as f32,
            other => as_f64(other) as f32,
        }),
        mh::doubleValue => Value::Double(as_f64(v)),
        mh::shortValue => Value::Int(as_i32(v) as i16 as i32),
        mh::byteValue => Value::Int(as_i32(v) as i8 as i32),
        mh::charValue => Value::Int(as_i32(v) as u16 as i32),
        _ => v, // booleanValue and any box's own accessor
    }
}

/// `args[i]` as a primitive: unboxed from a wrapper object, or as passed.
fn unboxed(ctx: &NativeContext<'_>, i: usize) -> Value {
    match ctx.args.get(i).copied() {
        Some(Value::ObjectRef(obj)) => ctx.objects.get_field(obj, 0).unwrap_or(Value::Null),
        Some(v) => v,
        None => Value::Null,
    }
}

/// A primitive as (value bits, integer key, float value, is_float). Bits
/// are what `floatToIntBits` yields — NaNs collapsed to the canonical one —
/// so they give Java's `equals`/`hashCode` (`Float.equals(NaN)` is true,
/// `0.0` and `-0.0` differ, `Long.hashCode` folds the halves); the key or
/// float value gives `compare`.
fn prim(v: Value) -> Option<(u64, i64, f64, bool)> {
    Some(match v {
        Value::Int(n) => (n as u32 as u64, n as i64, 0.0, false),
        Value::Long(n) => (n as u64, n, 0.0, false),
        Value::Float(f) => {
            let f = if f.is_nan() { f32::NAN } else { f };
            (f.to_bits() as u64, 0, f as f64, true)
        }
        Value::Double(d) => {
            let d = if d.is_nan() { f64::NAN } else { d };
            (d.to_bits(), 0, d, true)
        }
        _ => return None,
    })
}

/// Arms every wrapper shares — the Java 8 value/identity surface that data
/// classes, `Intrinsics.areEqual`, `HashMap` keys and `compareBy` lean on.
/// Each accepts both the instance form (`args[0]` is the box) and the static
/// form (`args[0]`/`args[1]` are primitives): `hashCode()` and
/// `hashCode(I)`, `compareTo(Integer)` and `compare(II)`. `floatToIntBits`
/// is the one bit conversion (data-class `hashCode` inlines it).
fn dispatch_common(
    class: &'static str,
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let a = unboxed(ctx, 0);
    let r: i32 = match method {
        // Same wrapper class and same value bits (Integer(1) != Long(1)).
        mh::equals => {
            let same_class = match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::ObjectRef(x)), Some(Value::ObjectRef(y))) => {
                    ctx.objects.class_name(*x) == ctx.objects.class_name(*y)
                }
                _ => false,
            };
            (same_class
                && match (prim(a), prim(unboxed(ctx, 1))) {
                    (Some((x, ..)), Some((y, ..))) => x == y,
                    _ => false,
                }) as i32
        }
        mh::hashCode => {
            let (bits, ..) = prim(a)?;
            if class == c::java_lang_Boolean {
                if bits != 0 {
                    1231
                } else {
                    1237
                }
            } else {
                (bits ^ (bits >> 32)) as i32
            }
        }
        mh::compareTo | mh::compare => {
            let (Some((_, xi, xf, is_float)), Some((_, yi, yf, _))) =
                (prim(a), prim(unboxed(ctx, 1)))
            else {
                return Some(Err(JvmError::InvalidReference));
            };
            // Java's float total order: `-0.0 < 0.0`, NaN (canonical by
            // now) above everything and equal to itself — the bit pattern
            // as a signed integer orders exactly that way once `<`/`>` have
            // settled the rest.
            if is_float {
                if xf < yf {
                    -1
                } else if xf > yf {
                    1
                } else {
                    let (xb, yb) = (xf.to_bits() as i64, yf.to_bits() as i64);
                    (xb > yb) as i32 - (xb < yb) as i32
                }
            } else {
                (xi > yi) as i32 - (xi < yi) as i32
            }
        }
        mh::floatToIntBits => prim(a)?.0 as i32,
        _ => return None,
    };
    Some(Ok(Some(Value::Int(r))))
}

/// `java/lang/Character` static predicates and case mapping over the ASCII
/// range the byte-backed string table can hold; code points above 0x7F are
/// neither letters nor digits here and map to themselves.
fn char_static(method: u32, c: i32) -> Option<Value> {
    let b = u8::try_from(c).ok().filter(|b| b.is_ascii());
    Some(Value::Int(match method {
        mh::isDigit => b.is_some_and(|b| b.is_ascii_digit()) as i32,
        mh::isLetter => b.is_some_and(|b| b.is_ascii_alphabetic()) as i32,
        mh::toUpperCase => b.map_or(c, |b| b.to_ascii_uppercase() as i32),
        mh::toLowerCase => b.map_or(c, |b| b.to_ascii_lowercase() as i32),
        _ => return None,
    }))
}

/// `isNaN` / `isInfinite` / `isFinite` for `Float` and `Double`, in the
/// static form (`args[0]` is the primitive) and the instance form
/// (`args[0]` is the box); a `Float` box stores a `Value::Float`.
fn float_predicate(
    method: u32,
    ctx: &NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let test: fn(f64) -> bool = match method {
        mh::isNaN => f64::is_nan,
        mh::isInfinite => f64::is_infinite,
        mh::isFinite => f64::is_finite,
        _ => return None,
    };
    let v = match unboxed(ctx, 0) {
        Value::Float(f) => f as f64,
        Value::Double(d) => d,
        _ => return Some(Err(JvmError::InvalidReference)),
    };
    Some(Ok(Some(Value::Int(test(v) as i32))))
}

/// `valueOf(String)` shares a method name with the boxing `valueOf(primitive)`;
/// the descriptor's first parameter tells them apart.
fn is_string_arg(ctx: &NativeContext<'_>) -> bool {
    ctx.descriptor.starts_with(crate::names::d::p_String)
}

pub(crate) fn dispatch_integer(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(integer_to_string(ctx));
    }
    if method == mh::parseInt {
        return Some(parse_int(ctx).map(|v| Some(Value::Int(v))));
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_int(ctx).and_then(|v| box_value(c::java_lang_Integer, Value::Int(v), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Integer, Value::Int(0), ctx, method)
}

pub(crate) fn dispatch_boolean(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(boolean_to_string(ctx));
    }
    if method == mh::parseBoolean || (method == mh::valueOf && is_string_arg(ctx)) {
        // Java's contract: true iff the string equalsIgnoreCase("true"); never throws.
        let b = matches!(resolve_str(ctx, 0), Ok(s) if s.eq_ignore_ascii_case("true"));
        if method == mh::parseBoolean {
            return Some(Ok(Some(Value::Int(b as i32))));
        }
        return Some(box_value(c::java_lang_Boolean, Value::Int(b as i32), ctx));
    }
    boxed_dispatch(c::java_lang_Boolean, Value::Int(0), ctx, method)
}

pub(crate) fn dispatch_long(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(long_to_string(ctx));
    }
    if method == mh::parseLong {
        return Some(parse_long(ctx).map(|v| Some(Value::Long(v))));
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_long(ctx).and_then(|v| box_value(c::java_lang_Long, Value::Long(v), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Long, Value::Long(0), ctx, method)
}

pub(crate) fn dispatch_float(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(float_to_string(ctx));
    }
    if method == mh::parseFloat {
        return Some(parse_f64(ctx).map(|v| Some(Value::Float(v as f32))));
    }
    if let Some(r) = float_predicate(method, ctx) {
        return Some(r);
    }
    if method == mh::intBitsToFloat {
        return Some(match ctx.args.first() {
            Some(Value::Int(bits)) => Ok(Some(Value::Float(f32::from_bits(*bits as u32)))),
            _ => Err(JvmError::InvalidReference),
        });
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_f64(ctx).and_then(|v| box_value(c::java_lang_Float, Value::Float(v as f32), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Float, Value::Float(0.0), ctx, method)
}

pub(crate) fn dispatch_double(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(double_to_string(ctx));
    }
    if method == mh::parseDouble {
        return Some(parse_f64(ctx).map(|v| Some(Value::Double(v))));
    }
    if let Some(r) = float_predicate(method, ctx) {
        return Some(r);
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_f64(ctx).and_then(|v| box_value(c::java_lang_Double, Value::Double(v), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Double, Value::Double(0.0), ctx, method)
}

pub(crate) fn dispatch_character(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(character_to_string(ctx));
    }
    if let Some(Value::Int(c)) = ctx.args.first().copied() {
        if let Some(v) = char_static(method, c) {
            return Some(Ok(Some(v)));
        }
    }
    boxed_dispatch(c::java_lang_Character, Value::Int(0), ctx, method)
}

pub(crate) fn dispatch_byte(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(integer_to_string(ctx));
    }
    if method == mh::parseByte {
        return Some(parse_ranged(ctx, i8::MIN as i32, i8::MAX as i32));
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_ranged(ctx, i8::MIN as i32, i8::MAX as i32)
                .and_then(|v| box_value(c::java_lang_Byte, v.unwrap_or(Value::Int(0)), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Byte, Value::Int(0), ctx, method)
}

pub(crate) fn dispatch_short(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method == mh::toString {
        return Some(integer_to_string(ctx));
    }
    if method == mh::parseShort {
        return Some(parse_ranged(ctx, i16::MIN as i32, i16::MAX as i32));
    }
    if method == mh::valueOf && is_string_arg(ctx) {
        return Some(
            parse_ranged(ctx, i16::MIN as i32, i16::MAX as i32)
                .and_then(|v| box_value(c::java_lang_Short, v.unwrap_or(Value::Int(0)), ctx)),
        );
    }
    boxed_dispatch(c::java_lang_Short, Value::Int(0), ctx, method)
}

/// The value a wrapper's `String` constructor stores (`new Integer("42")`,
/// `new Boolean("true")`, …; the String is `args[1]`, after `this`): parsed
/// as the class's `parseX` parses it, with the same exceptions —
/// `NumberFormatException` for malformed or out-of-range text (and `null`
/// to the integral types), `NullPointerException` for `null` to `Float` /
/// `Double`; `Boolean` never throws and is `true` only for `"true"`
/// ignoring case. `Character` has no String constructor.
fn ctor_value_from_string(
    class: &'static str,
    ctx: &mut NativeContext<'_>,
) -> Result<Value, JvmError> {
    Ok(match class {
        c::java_lang_Boolean => Value::Int(
            matches!(resolve_str(ctx, 1), Ok(s) if s.eq_ignore_ascii_case("true")) as i32,
        ),
        c::java_lang_Long => Value::Long(parse_long_at(ctx, 1)?),
        c::java_lang_Float => Value::Float(parse_f64_at(ctx, 1)? as f32),
        c::java_lang_Double => Value::Double(parse_f64_at(ctx, 1)?),
        c::java_lang_Byte => ranged(parse_int_at(ctx, 1)?, i8::MIN as i32, i8::MAX as i32, ctx)?,
        c::java_lang_Short => ranged(parse_int_at(ctx, 1)?, i16::MIN as i32, i16::MAX as i32, ctx)?,
        _ => Value::Int(parse_int_at(ctx, 1)?),
    })
}

/// `v` as an `Int`, or the NumberFormatException an out-of-range
/// `parseByte` / `parseShort` throws.
fn ranged(v: i32, min: i32, max: i32, ctx: &mut NativeContext<'_>) -> Result<Value, JvmError> {
    if v < min || v > max {
        return Err(number_format_exception(ctx));
    }
    Ok(Value::Int(v))
}

/// `Byte.parseByte` / `Short.parseShort`: parse as i32, then range-check —
/// out-of-range is a NumberFormatException, exactly as in Java.
fn parse_ranged(
    ctx: &mut NativeContext<'_>,
    min: i32,
    max: i32,
) -> Result<Option<Value>, JvmError> {
    let v = parse_int(ctx)?;
    ranged(v, min, max, ctx).map(Some)
}

// ── toString helpers ───────────────────────────────────────────────────────
//
// Each one accepts both calling conventions:
//   - static toString(primitive) — args[0] is the primitive Value variant
//   - instance toString()        — args[0] is the boxed ObjectRef; field 0
//                                  holds the value
//
// The result is interned via `ctx.strings.intern_dyn` and returned as a
// `Value::Reference` so callers see a regular `String` reference.

fn integer_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let n = read_int_arg(ctx, Value::Int(0));
    let mut buf = [0u8; 12];
    intern_string(ctx, int_to_decimal_buf(n, &mut buf))
}

fn long_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let n = match ctx.args.first().copied() {
        Some(Value::Long(v)) => v,
        Some(Value::ObjectRef(obj)) => match ctx.objects.get_field(obj, 0) {
            Some(Value::Long(v)) => v,
            _ => 0,
        },
        _ => 0,
    };
    let mut buf = [0u8; 21];
    intern_string(ctx, long_to_decimal_buf(n, &mut buf))
}

fn boolean_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let n = read_int_arg(ctx, Value::Int(0));
    intern_string(ctx, if n != 0 { b"true" } else { b"false" })
}

fn float_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let f = match ctx.args.first().copied() {
        Some(Value::Float(v)) => v,
        Some(Value::ObjectRef(obj)) => match ctx.objects.get_field(obj, 0) {
            Some(Value::Float(v)) => v,
            _ => 0.0,
        },
        _ => 0.0,
    };
    let mut buf = [0u8; 32];
    intern_string(ctx, float_to_str_buf(f, &mut buf))
}

fn double_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let d = match ctx.args.first().copied() {
        Some(Value::Double(v)) => v,
        Some(Value::ObjectRef(obj)) => match ctx.objects.get_field(obj, 0) {
            Some(Value::Double(v)) => v,
            _ => 0.0,
        },
        _ => 0.0,
    };
    let mut buf = [0u8; 32];
    intern_string(ctx, double_to_str_buf(d, &mut buf))
}

fn character_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let n = read_int_arg(ctx, Value::Int(0));
    // The bytes the String model holds for the char (`string::char_bytes`:
    // UTF-8 from U+0080 up), shared with every other char sink.
    let mut buf = [0u8; 3];
    let bytes = super::string::char_bytes(n, &mut buf);
    intern_string(ctx, bytes)
}

fn read_int_arg(ctx: &NativeContext<'_>, default: Value) -> i32 {
    let value = match ctx.args.first().copied() {
        Some(Value::Int(v)) => Value::Int(v),
        Some(Value::ObjectRef(obj)) => ctx.objects.get_field(obj, 0).unwrap_or(default),
        _ => default,
    };
    if let Value::Int(v) = value {
        v
    } else {
        0
    }
}

fn intern_string(ctx: &mut NativeContext<'_>, bytes: &[u8]) -> Result<Option<Value>, JvmError> {
    let idx = ctx
        .strings
        .intern_dyn(bytes)
        .ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(idx)))
}

// ── String → primitive parsing (Integer.parseInt family) ───────────────────
//
// Grammar follows Java: parseInt/parseLong take an optional sign and decimal
// digits only (core's FromStr matches exactly — optional +/-, no whitespace,
// no underscores); parseFloat/parseDouble additionally trim whitespace and
// accept a trailing f/F/d/D suffix per the Java grammar. Malformed input
// throws java.lang.NumberFormatException.

fn resolve_str<'a>(ctx: &'a NativeContext<'_>, i: usize) -> Result<&'a str, JvmError> {
    match ctx.args.get(i).copied() {
        Some(Value::Reference(idx)) => ctx.strings.resolve(idx).ok_or(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidReference),
    }
}

fn number_format_exception(ctx: &mut NativeContext<'_>) -> JvmError {
    match ctx.objects.alloc(c::java_lang_NumberFormatException) {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

/// `valueOf`: the JLS-cached range (§5.1.7) hands back one shared box per
/// value, so `Integer a = 127, b = 127; a == b` holds as on Android;
/// everything else is a fresh box. `<init>` never comes here — `new
/// Integer(5)` is always a distinct object.
#[inline(never)]
fn box_value(
    class: &'static str,
    val: Value,
    ctx: &mut NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    if let Some(idx) = ctx.objects.cached_box(class, val) {
        return Ok(Some(Value::ObjectRef(idx)));
    }
    let idx = ctx.objects.alloc(class).ok_or(JvmError::StackOverflow)?;
    ctx.objects
        .set_field(idx, 0, val)
        .ok_or(JvmError::StackOverflow)?;
    ctx.objects.cache_box(class, val, idx);
    Ok(Some(Value::ObjectRef(idx)))
}

/// `Integer.parseInt(null)` / `Long.parseLong(null)` throw
/// NumberFormatException, `Float/Double.parseX(null)` NullPointerException,
/// as in Java (QA 2026-09-13: all four were the uncatchable
/// InvalidReference).
fn null_string_arg(ctx: &NativeContext<'_>, at: usize) -> bool {
    matches!(ctx.args.get(at), Some(Value::Null) | None)
}

fn parse_int(ctx: &mut NativeContext<'_>) -> Result<i32, JvmError> {
    parse_int_at(ctx, 0)
}

/// `parseInt` of the String at `args[at]`: 0 for the static form, 1 for a
/// constructor's.
fn parse_int_at(ctx: &mut NativeContext<'_>, at: usize) -> Result<i32, JvmError> {
    if null_string_arg(ctx, at) {
        return Err(number_format_exception(ctx));
    }
    let s = resolve_str(ctx, at)?;
    s.parse::<i32>().map_err(|_| number_format_exception(ctx))
}

fn parse_long(ctx: &mut NativeContext<'_>) -> Result<i64, JvmError> {
    parse_long_at(ctx, 0)
}

fn parse_long_at(ctx: &mut NativeContext<'_>, at: usize) -> Result<i64, JvmError> {
    if null_string_arg(ctx, at) {
        return Err(number_format_exception(ctx));
    }
    let s = resolve_str(ctx, at)?;
    s.parse::<i64>().map_err(|_| number_format_exception(ctx))
}

fn parse_f64(ctx: &mut NativeContext<'_>) -> Result<f64, JvmError> {
    parse_f64_at(ctx, 0)
}

fn parse_f64_at(ctx: &mut NativeContext<'_>, at: usize) -> Result<f64, JvmError> {
    if null_string_arg(ctx, at) {
        return Err(super::throw_named(ctx, c::java_lang_NullPointerException));
    }
    let s = resolve_str(ctx, at)?.trim();
    // Java's FP grammar allows a trailing type suffix ("1.5f", "2d") on a
    // numeric literal only: "Infinityd" is malformed.
    let (sign, unsigned) = match s.as_bytes().first() {
        Some(b'+') => ("+", &s[1..]),
        Some(b'-') => ("-", &s[1..]),
        _ => ("", s),
    };
    // Rust's parser also takes "inf", "infinity" and "nan" in any case;
    // Java accepts exactly "Infinity" and "NaN" (with an optional sign).
    match unsigned {
        "Infinity" => {
            return Ok(if sign == "-" {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            })
        }
        "NaN" => return Ok(f64::NAN),
        _ => {}
    }
    if unsigned
        .bytes()
        .any(|b| !(b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')))
    {
        // Strip one trailing type suffix, then every byte must be numeric.
        let body = match unsigned.as_bytes().last() {
            Some(b'f' | b'F' | b'd' | b'D') if unsigned.len() > 1 => {
                &unsigned[..unsigned.len() - 1]
            }
            _ => unsigned,
        };
        if body.starts_with(['+', '-'])
            || body
                .bytes()
                .any(|b| !(b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')))
        {
            return Err(number_format_exception(ctx));
        }
        // Parse the unsigned body and apply the sign: no buffer is built, so
        // nothing here can fail on a full heap (native_handler/alloc_scan).
        let v = body
            .parse::<f64>()
            .map_err(|_| number_format_exception(ctx))?;
        return Ok(if sign == "-" { -v } else { v });
    }
    s.parse::<f64>().map_err(|_| number_format_exception(ctx))
}
