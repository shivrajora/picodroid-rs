// SPDX-License-Identifier: GPL-3.0-only
use crate::types::{JvmError, Value};

use super::NativeContext;

/// Java `Math.min`/`Math.max` on floating point: NaN if either operand is
/// NaN, and `-0.0 < 0.0`. Rust's `f32::min`/`max` return the non-NaN
/// operand and treat the two zeros as equal.
macro_rules! java_minmax {
    ($name:ident, $t:ty, $pick:ident, $zero_wins:expr) => {
        fn $name(a: $t, b: $t) -> $t {
            if a.is_nan() || b.is_nan() {
                return <$t>::NAN;
            }
            if a == 0.0 && b == 0.0 {
                // Signed zero: min prefers -0.0, max prefers +0.0.
                return if a.is_sign_negative() == $zero_wins {
                    a
                } else {
                    b
                };
            }
            a.$pick(b)
        }
    };
}
java_minmax!(java_min_f32, f32, min, true);
java_minmax!(java_min_f64, f64, min, true);
java_minmax!(java_max_f32, f32, max, false);
java_minmax!(java_max_f64, f64, max, false);

/// Java `Math.round`: nearest integer, ties toward positive infinity —
/// i.e. `floor(x + 0.5)` without the precision loss of actually adding
/// 0.5 (`x - floor(x)` is exact). `as` saturates and maps NaN to 0, which
/// is the Java contract for the cast.
fn java_round_f32(f: f32) -> i32 {
    let fl = libm::floorf(f);
    let r = if f - fl >= 0.5 { fl + 1.0 } else { fl };
    r as i32
}

fn java_round_f64(d: f64) -> i64 {
    let fl = libm::floor(d);
    let r = if d - fl >= 0.5 { fl + 1.0 } else { fl };
    r as i64
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "abs" => match ctx.args.first() {
            // Java: abs(MIN_VALUE) == MIN_VALUE; plain `abs` overflows.
            Some(Value::Int(i)) => Some(Ok(Some(Value::Int(i.wrapping_abs())))),
            Some(Value::Long(l)) => Some(Ok(Some(Value::Long(l.wrapping_abs())))),
            Some(Value::Float(f)) => Some(Ok(Some(Value::Float(f.abs())))),
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(d.abs())))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "min" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.min(b))))),
            (Some(Value::Long(a)), Some(Value::Long(b))) => Some(Ok(Some(Value::Long(*a.min(b))))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                Some(Ok(Some(Value::Float(java_min_f32(*a, *b)))))
            }
            (Some(Value::Double(a)), Some(Value::Double(b))) => {
                Some(Ok(Some(Value::Double(java_min_f64(*a, *b)))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "max" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.max(b))))),
            (Some(Value::Long(a)), Some(Value::Long(b))) => Some(Ok(Some(Value::Long(*a.max(b))))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                Some(Ok(Some(Value::Float(java_max_f32(*a, *b)))))
            }
            (Some(Value::Double(a)), Some(Value::Double(b))) => {
                Some(Ok(Some(Value::Double(java_max_f64(*a, *b)))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "sqrt" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::sqrt(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "pow" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Double(a)), Some(Value::Double(b))) => {
                Some(Ok(Some(Value::Double(libm::pow(*a, *b)))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "floor" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::floor(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "ceil" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::ceil(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "round" => match ctx.args.first() {
            Some(Value::Float(f)) => Some(Ok(Some(Value::Int(java_round_f32(*f))))),
            Some(Value::Double(d)) => Some(Ok(Some(Value::Long(java_round_f64(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "sin" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::sin(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "cos" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::cos(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "tan" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::tan(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "atan2" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Double(y)), Some(Value::Double(x))) => {
                Some(Ok(Some(Value::Double(libm::atan2(*y, *x)))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "toRadians" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(
                *d * (core::f64::consts::PI / 180.0),
            )))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "toDegrees" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(
                *d * (180.0 / core::f64::consts::PI),
            )))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "log" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::log(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "log10" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::log10(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "exp" => match ctx.args.first() {
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(libm::exp(*d))))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        _ => None,
    }
}
