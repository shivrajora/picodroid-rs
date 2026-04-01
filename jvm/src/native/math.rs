use crate::types::{JvmError, Value};

use super::NativeContext;

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "abs" => match ctx.args.first() {
            Some(Value::Int(i)) => Some(Ok(Some(Value::Int(i.abs())))),
            Some(Value::Long(l)) => Some(Ok(Some(Value::Long(l.abs())))),
            Some(Value::Float(f)) => Some(Ok(Some(Value::Float(f.abs())))),
            Some(Value::Double(d)) => Some(Ok(Some(Value::Double(d.abs())))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "min" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.min(b))))),
            (Some(Value::Long(a)), Some(Value::Long(b))) => Some(Ok(Some(Value::Long(*a.min(b))))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                Some(Ok(Some(Value::Float(a.min(*b)))))
            }
            (Some(Value::Double(a)), Some(Value::Double(b))) => {
                Some(Ok(Some(Value::Double(a.min(*b)))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "max" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Int(a)), Some(Value::Int(b))) => Some(Ok(Some(Value::Int(*a.max(b))))),
            (Some(Value::Long(a)), Some(Value::Long(b))) => Some(Ok(Some(Value::Long(*a.max(b))))),
            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                Some(Ok(Some(Value::Float(a.max(*b)))))
            }
            (Some(Value::Double(a)), Some(Value::Double(b))) => {
                Some(Ok(Some(Value::Double(a.max(*b)))))
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
            Some(Value::Float(f)) => Some(Ok(Some(Value::Int(libm::roundf(*f) as i32)))),
            Some(Value::Double(d)) => Some(Ok(Some(Value::Long(libm::round(*d) as i64)))),
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
