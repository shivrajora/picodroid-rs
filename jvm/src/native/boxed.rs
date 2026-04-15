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
    boxed_dispatch!("java/lang/Integer", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_boolean(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    boxed_dispatch!("java/lang/Boolean", Value::Int(0), ctx, method_name)
}

pub(crate) fn dispatch_long(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    boxed_dispatch!("java/lang/Long", Value::Long(0), ctx, method_name)
}

pub(crate) fn dispatch_float(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
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
    boxed_dispatch!("java/lang/Character", Value::Int(0), ctx, method_name)
}
