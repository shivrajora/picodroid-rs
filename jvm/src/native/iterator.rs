// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::iter_store::IterSource,
    types::{JvmError, Value},
};

use super::NativeContext;
use crate::names::{c, m};

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
        return Some(Err(JvmError::InvalidReference));
    };

    match method_name {
        m::hasNext => {
            let state = match ctx.objects.iter_get(obj_idx) {
                Some(s) => s,
                None => return Some(Err(JvmError::InvalidReference)),
            };
            let len = source_len(ctx, &state.source);
            Some(Ok(Some(Value::Int((state.position < len) as i32))))
        }
        m::next => {
            let (source, pos, expected) = match ctx.objects.iter_get(obj_idx) {
                Some(s) => (s.source, s.position, s.expected_len),
                None => return Some(Err(JvmError::InvalidReference)),
            };
            let len = source_len(ctx, &source);
            if len != expected {
                // Fail fast, like java.util: the source changed behind the
                // iterator (bugbash S6).
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_util_ConcurrentModificationException,
                )));
            }
            if pos >= len {
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_util_NoSuchElementException,
                )));
            }
            let value = match source {
                IterSource::List(buf) => ctx.objects.list_get(buf, pos).unwrap_or(Value::Null),
                IterSource::MapKeys(buf) => map_entry(ctx, buf, pos).0,
                IterSource::MapValues(buf) => map_entry(ctx, buf, pos).1,
                IterSource::MapEntries(buf) => {
                    let (k, v) = map_entry(ctx, buf, pos);
                    let Some(entry) = ctx
                        .objects
                        .alloc_with_field_count(c::java_util_Map_Entry, 2)
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
                state.last_returned = Some(state.position);
                state.position += 1;
            }
            Some(Ok(Some(value)))
        }
        m::remove => {
            let (source, last, expected) = match ctx.objects.iter_get(obj_idx) {
                Some(s) => (s.source, s.last_returned, s.expected_len),
                None => return Some(Err(JvmError::InvalidReference)),
            };
            // remove() before next(), or twice in a row.
            let Some(at) = last else {
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_IllegalStateException,
                )));
            };
            if source_len(ctx, &source) != expected {
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_util_ConcurrentModificationException,
                )));
            }
            let removed = match source {
                IterSource::List(buf) => ctx.objects.list_remove(buf, at).is_some(),
                IterSource::MapKeys(buf)
                | IterSource::MapValues(buf)
                | IterSource::MapEntries(buf) => ctx.objects.map_remove_at(buf, at),
            };
            if !removed {
                return Some(Err(JvmError::InvalidReference));
            }
            if let Some(state) = ctx.objects.iter_get_mut(obj_idx) {
                state.position = at;
                state.expected_len -= 1;
                state.last_returned = None;
            }
            Some(Ok(None))
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
