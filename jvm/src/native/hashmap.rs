// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::{iter_store::IterSource, iter_store::IteratorState, ObjectHeap},
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
        "keySet" => view(ctx, "java/util/HashMap$KeySet"),
        "values" => view(ctx, "java/util/HashMap$Values"),
        "entrySet" => view(ctx, "java/util/HashMap$EntrySet"),
        _ => None,
    }
}

/// A view object over the receiver's map buffer (field 0 = map_buf index).
fn view(
    ctx: &mut NativeContext<'_>,
    class: &'static str,
) -> Option<Result<Option<Value>, JvmError>> {
    let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
        Ok(i) => i,
        Err(e) => return Some(Err(e)),
    };
    let view = match ctx.objects.alloc(class) {
        Some(idx) => idx,
        None => return Some(Err(JvmError::StackOverflow)),
    };
    ctx.objects.set_field(view, 0, Value::Int(buf_idx as i32));
    Some(Ok(Some(Value::ObjectRef(view))))
}

/// Dispatch for the `HashMap$KeySet` / `$Values` / `$EntrySet` views:
/// `iterator()` (keys, values, or fresh `Map$Entry` objects, by the view's
/// class) and `size()`.
pub(crate) fn dispatch_view(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let buf_idx = match get_map_buf(ctx.objects, ctx.args) {
        Ok(i) => i,
        Err(e) => return Some(Err(e)),
    };
    match method_name {
        "iterator" => {
            let Some(Value::ObjectRef(recv)) = ctx.args.first().copied() else {
                return Some(Err(JvmError::InvalidReference));
            };
            let source = match ctx.objects.class_name(recv) {
                Some("java/util/HashMap$KeySet") => IterSource::MapKeys(buf_idx),
                Some("java/util/HashMap$Values") => IterSource::MapValues(buf_idx),
                _ => IterSource::MapEntries(buf_idx),
            };
            let iter_obj = match ctx.objects.alloc("java/util/Iterator") {
                Some(idx) => idx,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            ctx.objects.iter_register(
                iter_obj,
                IteratorState {
                    source,
                    position: 0,
                },
            );
            Some(Ok(Some(Value::ObjectRef(iter_obj))))
        }
        "size" => Some(Ok(Some(Value::Int(ctx.objects.map_len(buf_idx) as i32)))),
        _ => None,
    }
}

/// Dispatch for the `java/util/Map$Entry` objects an `entrySet()` iterator
/// yields: `getKey()` (field 0) and `getValue()` (field 1).
pub(crate) fn dispatch_entry(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let slot = match method_name {
        "getKey" => 0,
        "getValue" => 1,
        _ => return None,
    };
    let Some(Value::ObjectRef(idx)) = ctx.args.first().copied() else {
        return Some(Err(JvmError::InvalidReference));
    };
    Some(Ok(Some(
        ctx.objects.get_field(idx, slot).unwrap_or(Value::Null),
    )))
}
