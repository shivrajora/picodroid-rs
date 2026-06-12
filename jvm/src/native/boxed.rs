// SPDX-License-Identifier: GPL-3.0-only
use crate::object_heap::{float_to_str_buf, int_to_decimal_buf, long_to_decimal_buf};
use crate::types::{JvmError, Value};

use super::NativeContext;

/// Dispatch `<init>`, `valueOf`, and the unboxing accessor for a boxed
/// primitive type.  All five wrappers (Integer, Boolean, Long, Float, Double)
/// share the same three-method pattern — only the class name and the default
/// value differ.
macro_rules! boxed_dispatch {
    ($class:literal, $default:expr, $ctx:expr, $method:expr) => {
        match $method {
            "<init>" => {
                let Value::ObjectRef(obj) = $ctx.args.first().copied().unwrap_or(Value::Null)
                else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let val = $ctx.args.get(1).copied().unwrap_or(Value::Null);
                $ctx.objects.set_field(obj, 0, val);
                Some(Ok(None))
            }
            "valueOf" => {
                let val = $ctx.args.first().copied().unwrap_or(Value::Null);
                let obj_idx = $ctx.objects.alloc($class).ok_or(JvmError::StackOverflow);
                match obj_idx {
                    Err(e) => Some(Err(e)),
                    Ok(idx) => {
                        $ctx.objects.set_field(idx, 0, val);
                        Some(Ok(Some(Value::ObjectRef(idx))))
                    }
                }
            }
            // Unboxing accessor: intValue, booleanValue, longValue, etc.
            _ if $method.ends_with("Value") => {
                let Value::ObjectRef(obj) = $ctx.args.first().copied().unwrap_or(Value::Null)
                else {
                    return Some(Err(JvmError::InvalidReference));
                };
                Some(Ok(Some($ctx.objects.get_field(obj, 0).unwrap_or($default))))
            }
            _ => None,
        }
    };
}

/// `valueOf(String)` shares a method name with the boxing `valueOf(primitive)`;
/// the descriptor's first parameter tells them apart.
fn is_string_arg(ctx: &NativeContext<'_>) -> bool {
    ctx.descriptor.starts_with("(Ljava/lang/String;")
}

pub(crate) fn dispatch_integer(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(integer_to_string(ctx));
    }
    if method_name == "parseInt" {
        return Some(parse_int(ctx).map(|v| Some(Value::Int(v))));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_int(ctx).and_then(|v| box_value("java/lang/Integer", Value::Int(v), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Integer", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_boolean(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(boolean_to_string(ctx));
    }
    if method_name == "parseBoolean" || (method_name == "valueOf" && is_string_arg(ctx)) {
        // Java's contract: true iff the string equalsIgnoreCase("true"); never throws.
        let b = matches!(resolve_str(ctx, 0), Ok(s) if s.eq_ignore_ascii_case("true"));
        if method_name == "parseBoolean" {
            return Some(Ok(Some(Value::Int(b as i32))));
        }
        return Some(box_value("java/lang/Boolean", Value::Int(b as i32), ctx));
    }
    boxed_dispatch!("java/lang/Boolean", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_long(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(long_to_string(ctx));
    }
    if method_name == "parseLong" {
        return Some(parse_long(ctx).map(|v| Some(Value::Long(v))));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_long(ctx).and_then(|v| box_value("java/lang/Long", Value::Long(v), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Long", Value::Long(0), ctx, method_name)
}

pub(crate) fn dispatch_float(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(float_to_string(ctx));
    }
    if method_name == "parseFloat" {
        return Some(parse_f64(ctx).map(|v| Some(Value::Float(v as f32))));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_f64(ctx).and_then(|v| box_value("java/lang/Float", Value::Float(v as f32), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Float", Value::Float(0.0), ctx, method_name)
}

pub(crate) fn dispatch_double(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "parseDouble" {
        return Some(parse_f64(ctx).map(|v| Some(Value::Double(v))));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_f64(ctx).and_then(|v| box_value("java/lang/Double", Value::Double(v), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Double", Value::Double(0.0), ctx, method_name)
}

pub(crate) fn dispatch_character(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(character_to_string(ctx));
    }
    boxed_dispatch!("java/lang/Character", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_byte(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(integer_to_string(ctx));
    }
    if method_name == "parseByte" {
        return Some(parse_ranged(ctx, i8::MIN as i32, i8::MAX as i32));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_ranged(ctx, i8::MIN as i32, i8::MAX as i32)
                .and_then(|v| box_value("java/lang/Byte", v.unwrap_or(Value::Int(0)), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Byte", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_short(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(integer_to_string(ctx));
    }
    if method_name == "parseShort" {
        return Some(parse_ranged(ctx, i16::MIN as i32, i16::MAX as i32));
    }
    if method_name == "valueOf" && is_string_arg(ctx) {
        return Some(
            parse_ranged(ctx, i16::MIN as i32, i16::MAX as i32)
                .and_then(|v| box_value("java/lang/Short", v.unwrap_or(Value::Int(0)), ctx)),
        );
    }
    boxed_dispatch!("java/lang/Short", Value::Int(0), ctx, method_name)
}

/// `Byte.parseByte` / `Short.parseShort`: parse as i32, then range-check —
/// out-of-range is a NumberFormatException, exactly as in Java.
fn parse_ranged(
    ctx: &mut NativeContext<'_>,
    min: i32,
    max: i32,
) -> Result<Option<Value>, JvmError> {
    let v = parse_int(ctx)?;
    if v < min || v > max {
        return Err(number_format_exception(ctx));
    }
    Ok(Some(Value::Int(v)))
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

fn character_to_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let n = read_int_arg(ctx, Value::Int(0));
    // Encode the BMP code point as UTF-8. char values outside the basic plane
    // aren't reachable through `char` in javac, but we cap defensively.
    let cp = (n as u32) & 0xFFFF;
    let mut buf = [0u8; 4];
    let len = if cp < 0x80 {
        buf[0] = cp as u8;
        1
    } else if cp < 0x800 {
        buf[0] = 0xC0 | ((cp >> 6) as u8);
        buf[1] = 0x80 | ((cp & 0x3F) as u8);
        2
    } else {
        buf[0] = 0xE0 | ((cp >> 12) as u8);
        buf[1] = 0x80 | (((cp >> 6) & 0x3F) as u8);
        buf[2] = 0x80 | ((cp & 0x3F) as u8);
        3
    };
    intern_string(ctx, &buf[..len])
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
    match ctx.objects.alloc("java/lang/NumberFormatException") {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}

fn box_value(
    class: &'static str,
    val: Value,
    ctx: &mut NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let idx = ctx.objects.alloc(class).ok_or(JvmError::StackOverflow)?;
    ctx.objects.set_field(idx, 0, val);
    Ok(Some(Value::ObjectRef(idx)))
}

fn parse_int(ctx: &mut NativeContext<'_>) -> Result<i32, JvmError> {
    let s = resolve_str(ctx, 0)?;
    s.parse::<i32>().map_err(|_| number_format_exception(ctx))
}

fn parse_long(ctx: &mut NativeContext<'_>) -> Result<i64, JvmError> {
    let s = resolve_str(ctx, 0)?;
    s.parse::<i64>().map_err(|_| number_format_exception(ctx))
}

fn parse_f64(ctx: &mut NativeContext<'_>) -> Result<f64, JvmError> {
    let s = resolve_str(ctx, 0)?.trim();
    // Java's FP grammar allows a trailing type suffix ("1.5f", "2d").
    let s = match s.as_bytes().last() {
        Some(b'f' | b'F' | b'd' | b'D') if s.len() > 1 => &s[..s.len() - 1],
        _ => s,
    };
    s.parse::<f64>().map_err(|_| number_format_exception(ctx))
}
