// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::{iter_store::IterSource, iter_store::IteratorState, ObjectHeap},
    types::{JvmError, Value},
};

use super::NativeContext;

/// Extract the list buffer index stored in field 0 of an ArrayList receiver.
fn get_list_buf(objects: &ObjectHeap, args: &[Value]) -> Result<u16, JvmError> {
    let Value::ObjectRef(obj_idx) = args.first().copied().unwrap_or(Value::Null) else {
        return Err(JvmError::InvalidReference);
    };
    match objects.get_field(obj_idx, 0) {
        Some(Value::Int(n)) => Ok(n as u16),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Value equality for ArrayList.contains — uses value-based equality for
/// autoboxed wrapper objects so that `contains(42)` finds `Integer(42)` even
/// when the two `ObjectRef` indices differ (i.e., different heap slots).
fn values_eq(a: Value, b: Value, objects: &ObjectHeap) -> bool {
    match (a, b) {
        (Value::ObjectRef(ai), Value::ObjectRef(bi)) if ai != bi => {
            // Compare field 0 for wrapper equality (Integer, Long, Boolean, etc.)
            let fa = objects.get_field(ai, 0);
            fa.is_some() && fa == objects.get_field(bi, 0)
        }
        _ => a == b,
    }
}

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            // <init>() or <init>(int initialCapacity) — capacity hint ignored.
            let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let buf_idx = match ctx.objects.list_alloc() {
                Some(i) => i,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            ctx.objects
                .set_field(obj_idx, 0, Value::Int(buf_idx as i32));
            Some(Ok(None))
        }
        "add" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            if ctx.descriptor.starts_with("(I") {
                // add(int index, Object element) → void
                let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let v = ctx.args.get(2).copied().unwrap_or(Value::Null);
                ctx.objects.list_insert(buf_idx, i as usize, v);
                Some(Ok(None))
            } else {
                // add(Object element) → boolean (always true)
                let v = ctx.args.get(1).copied().unwrap_or(Value::Null);
                ctx.objects.list_add(buf_idx, v);
                Some(Ok(Some(Value::Int(1))))
            }
        }
        "get" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            match ctx.objects.list_get(buf_idx, i as usize) {
                Some(v) => Some(Ok(Some(v))),
                None => Some(Err(JvmError::ArrayIndexOutOfBounds)),
            }
        }
        "size" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(ctx.objects.list_len(buf_idx) as i32))))
        }
        "isEmpty" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(
                (ctx.objects.list_len(buf_idx) == 0) as i32,
            ))))
        }
        "set" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let v = ctx.args.get(2).copied().unwrap_or(Value::Null);
            let old = ctx
                .objects
                .list_set(buf_idx, i as usize, v)
                .unwrap_or(Value::Null);
            Some(Ok(Some(old)))
        }
        "remove" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let Value::Int(i) = ctx.args.get(1).copied().unwrap_or(Value::Null) else {
                return Some(Err(JvmError::InvalidReference));
            };
            match ctx.objects.list_remove(buf_idx, i as usize) {
                Some(v) => Some(Ok(Some(v))),
                None => Some(Err(JvmError::ArrayIndexOutOfBounds)),
            }
        }
        "clear" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            ctx.objects.list_clear(buf_idx);
            Some(Ok(None))
        }
        "iterator" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let iter_obj = match ctx.objects.alloc("java/util/Iterator") {
                Some(idx) => idx,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            ctx.objects.iter_register(
                iter_obj,
                IteratorState {
                    source: IterSource::List(buf_idx),
                    position: 0,
                },
            );
            Some(Ok(Some(Value::ObjectRef(iter_obj))))
        }
        "contains" => {
            let buf_idx = match get_list_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let needle = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let len = ctx.objects.list_len(buf_idx);
            let mut found = false;
            for i in 0..len {
                let elem = ctx.objects.list_get(buf_idx, i).unwrap_or(Value::Null);
                if values_eq(elem, needle, ctx.objects) {
                    found = true;
                    break;
                }
            }
            Some(Ok(Some(Value::Int(found as i32))))
        }
        _ => None,
    }
}
