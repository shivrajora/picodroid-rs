// SPDX-License-Identifier: GPL-3.0-only
use crate::types::{JvmError, Value};

use super::NativeContext;
use super::INIT;
use crate::names::mh;

pub(crate) fn dispatch(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method {
        // Identity hash in Java; the ordinal is just as stable per constant
        // and survives a GC's slot reuse, which the heap index would not.
        mh::hashCode => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let ordinal = ctx.objects.get_field(obj_idx, 1).unwrap_or(Value::Int(0));
            Some(Ok(Some(ordinal)))
        }
        INIT => {
            // Enum.<init>(String name, int ordinal)
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let name = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let ordinal = ctx.args.get(2).copied().unwrap_or(Value::Int(0));
            if ctx.objects.set_field(obj_idx, 0, name).is_none()
                || ctx.objects.set_field(obj_idx, 1, ordinal).is_none()
            {
                return Some(Err(JvmError::StackOverflow));
            }
            Some(Ok(None))
        }
        mh::name | mh::toString => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let name = ctx.objects.get_field(obj_idx, 0).unwrap_or(Value::Null);
            Some(Ok(Some(name)))
        }
        mh::ordinal => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let ordinal = ctx.objects.get_field(obj_idx, 1).unwrap_or(Value::Int(0));
            Some(Ok(Some(ordinal)))
        }
        mh::equals => {
            // Reference equality
            let a = ctx.args.first().copied().unwrap_or(Value::Null);
            let b = ctx.args.get(1).copied().unwrap_or(Value::Null);
            Some(Ok(Some(Value::Int((a == b) as i32))))
        }
        mh::compareTo => {
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
