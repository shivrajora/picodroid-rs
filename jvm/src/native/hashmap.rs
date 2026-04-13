use crate::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::NativeContext;

/// Extract the map buffer index stored in field 0 of a HashMap receiver.
fn get_map_buf(objects: &ObjectHeap, args: &[Value]) -> Result<u16, JvmError> {
    let Value::ObjectRef(obj_idx) = args.first().copied().unwrap_or(Value::Null) else {
        return Err(JvmError::InvalidReference);
    };
    match objects.get_field(obj_idx, 0) {
        Some(Value::Int(n)) => Ok(n as u16),
        _ => Err(JvmError::InvalidReference),
    }
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let buf_idx = match ctx.objects.map_alloc() {
                Some(i) => i,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            ctx.objects
                .set_field(obj_idx, 0, Value::Int(buf_idx as i32));
            Some(Ok(None))
        }
        "put" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let key = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let value = ctx.args.get(2).copied().unwrap_or(Value::Null);
            let old = ctx.objects.map_put(buf_idx, key, value, ctx.strings);
            Some(Ok(Some(old.unwrap_or(Value::Null))))
        }
        "get" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let key = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let value = ctx.objects.map_get(buf_idx, key, ctx.strings);
            Some(Ok(Some(value.unwrap_or(Value::Null))))
        }
        "remove" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let key = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let old = ctx.objects.map_remove(buf_idx, key, ctx.strings);
            Some(Ok(Some(old.unwrap_or(Value::Null))))
        }
        "containsKey" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let key = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let found = ctx.objects.map_contains_key(buf_idx, key, ctx.strings);
            Some(Ok(Some(Value::Int(found as i32))))
        }
        "containsValue" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let value = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let found = ctx.objects.map_contains_value(buf_idx, value, ctx.strings);
            Some(Ok(Some(Value::Int(found as i32))))
        }
        "size" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(ctx.objects.map_len(buf_idx) as i32))))
        }
        "isEmpty" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(
                (ctx.objects.map_len(buf_idx) == 0) as i32,
            ))))
        }
        "clear" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            ctx.objects.map_clear(buf_idx);
            Some(Ok(None))
        }
        "getOrDefault" => {
            let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let key = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let default = ctx.args.get(2).copied().unwrap_or(Value::Null);
            let value = ctx.objects.map_get(buf_idx, key, ctx.strings);
            Some(Ok(Some(value.unwrap_or(default))))
        }
        _ => None,
    }
}
