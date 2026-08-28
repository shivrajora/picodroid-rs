// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::iter_store::IterSource,
    types::{JvmError, Value},
};

use super::NativeContext;

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
        return Some(Err(JvmError::InvalidReference));
    };

    match method_name {
        "hasNext" => {
            let state = match ctx.objects.iter_get(obj_idx) {
                Some(s) => s,
                None => return Some(Err(JvmError::InvalidReference)),
            };
            let len = source_len(ctx, &state.source);
            Some(Ok(Some(Value::Int((state.position < len) as i32))))
        }
        "next" => {
            let (source, pos) = match ctx.objects.iter_get(obj_idx) {
                Some(s) => (s.source, s.position),
                None => return Some(Err(JvmError::InvalidReference)),
            };
            if pos >= source_len(ctx, &source) {
                return Some(Err(JvmError::ArrayIndexOutOfBounds));
            }
            let value = match source {
                IterSource::List(buf) => ctx.objects.list_get(buf, pos).unwrap_or(Value::Null),
                IterSource::MapKeys(buf) => map_entry(ctx, buf, pos).0,
                IterSource::MapValues(buf) => map_entry(ctx, buf, pos).1,
                IterSource::MapEntries(buf) => {
                    let (k, v) = map_entry(ctx, buf, pos);
                    let Some(entry) = ctx.objects.alloc_with_field_count("java/util/Map$Entry", 2)
                    else {
                        return Some(Err(JvmError::StackOverflow));
                    };
                    ctx.objects.set_field(entry, 0, k);
                    ctx.objects.set_field(entry, 1, v);
                    Value::ObjectRef(entry)
                }
            };
            // Advance position
            if let Some(state) = ctx.objects.iter_get_mut(obj_idx) {
                state.position += 1;
            }
            Some(Ok(Some(value)))
        }
        _ => None,
    }
}

/// Return the number of elements in the source collection.
fn source_len(ctx: &NativeContext<'_>, source: &IterSource) -> usize {
    match source {
        IterSource::List(buf_idx) => ctx.objects.list_len(*buf_idx),
        IterSource::MapKeys(buf_idx)
        | IterSource::MapValues(buf_idx)
        | IterSource::MapEntries(buf_idx) => ctx.objects.map_len(*buf_idx),
    }
}

/// The `(key, value)` pair at `pos` of a map buffer (`Null`s past the end).
fn map_entry(ctx: &NativeContext<'_>, buf: u16, pos: usize) -> (Value, Value) {
    ctx.objects
        .map_iter(buf)
        .nth(pos)
        .unwrap_or((Value::Null, Value::Null))
}
