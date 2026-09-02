// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::{iter_store::IterSource, iter_store::IteratorState, ObjectHeap},
    types::{JvmError, Value},
};

use super::NativeContext;
use crate::names::{c, m};

/// Sentinel value stored in the map backing store for HashSet entries.
const SET_PRESENT: Value = Value::Int(1);

/// Extract the map buffer index stored in field 0 of a HashSet receiver.
fn get_set_buf(objects: &ObjectHeap, args: &[Value]) -> Result<u16, JvmError> {
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
        m::add => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let elem = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let was_absent = ctx
                .objects
                .map_put(buf_idx, elem, SET_PRESENT, ctx.strings)
                .is_none();
            Some(Ok(Some(Value::Int(was_absent as i32))))
        }
        m::remove => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let elem = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let was_present = ctx.objects.map_remove(buf_idx, elem, ctx.strings).is_some();
            Some(Ok(Some(Value::Int(was_present as i32))))
        }
        m::contains => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let elem = ctx.args.get(1).copied().unwrap_or(Value::Null);
            let found = ctx.objects.map_contains_key(buf_idx, elem, ctx.strings);
            Some(Ok(Some(Value::Int(found as i32))))
        }
        m::size => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(ctx.objects.map_len(buf_idx) as i32))))
        }
        m::isEmpty => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            Some(Ok(Some(Value::Int(
                (ctx.objects.map_len(buf_idx) == 0) as i32,
            ))))
        }
        // iterator(): the set's elements are the keys of its map buffer, so
        // the HashMap key-view iterator serves it unchanged (hash order).
        m::iterator => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            let Some(Value::ObjectRef(owner)) = ctx.args.first().copied() else {
                return Some(Err(JvmError::InvalidReference));
            };
            let iter_obj = match ctx.objects.alloc(c::java_util_Iterator) {
                Some(idx) => idx,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            ctx.objects.iter_register(
                iter_obj,
                IteratorState {
                    source: IterSource::MapKeys(buf_idx),
                    position: 0,
                    owner,
                    expected_len: ctx.objects.map_len(buf_idx),
                    last_returned: None,
                },
            );
            Some(Ok(Some(Value::ObjectRef(iter_obj))))
        }
        m::clear => {
            let buf_idx = match get_set_buf(ctx.objects, ctx.args) {
                Ok(i) => i,
                Err(e) => return Some(Err(e)),
            };
            ctx.objects.map_clear(buf_idx);
            Some(Ok(None))
        }
        _ => None,
    }
}
