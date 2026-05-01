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

pub(crate) fn dispatch_integer(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(integer_to_string(ctx));
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
    boxed_dispatch!("java/lang/Boolean", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_long(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if method_name == "toString" {
        return Some(long_to_string(ctx));
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
    boxed_dispatch!("java/lang/Float", Value::Float(0.0), ctx, method_name)
}

pub(crate) fn dispatch_double(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
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
