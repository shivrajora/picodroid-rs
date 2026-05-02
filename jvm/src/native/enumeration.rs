// SPDX-License-Identifier: GPL-3.0-only
use crate::types::{JvmError, Value};

use super::NativeContext;

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            // Enum.<init>(String name, int ordinal)
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let name = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let ordinal = ctx.args.get(2).copied().unwrap_or(Value::Int(0));
            ctx.objects.set_field(obj_idx, 0, name);
            ctx.objects.set_field(obj_idx, 1, ordinal);
            Some(Ok(None))
        }
        "name" | "toString" => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let name = ctx.objects.get_field(obj_idx, 0).unwrap_or(Value::Null);
            Some(Ok(Some(name)))
        }
        "ordinal" => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let ordinal = ctx.objects.get_field(obj_idx, 1).unwrap_or(Value::Int(0));
            Some(Ok(Some(ordinal)))
        }
        "equals" => {
            // Reference equality
            let a = ctx.args.first().copied().unwrap_or(Value::Null);
            let b = ctx.args.get(1).copied().unwrap_or(Value::Null);
            Some(Ok(Some(Value::Int((a == b) as i32))))
        }
        "compareTo" => {
            let Value::ObjectRef(a_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let Value::ObjectRef(b_idx) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let a_ord = match ctx.objects.get_field(a_idx, 1) {
                Some(Value::Int(n)) => n,
                _ => 0,
            };
            let b_ord = match ctx.objects.get_field(b_idx, 1) {
                Some(Value::Int(n)) => n,
                _ => 0,
            };
            Some(Ok(Some(Value::Int(a_ord - b_ord))))
        }
        _ => None,
    }
}
