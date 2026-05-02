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
            // Two-step: read element, then advance position.
            // Read first to avoid borrow conflict.
            let (source_clone, pos) = match ctx.objects.iter_get(obj_idx) {
                Some(s) => {
                    let src = match &s.source {
                        IterSource::List(idx) => IterSource::List(*idx),
                        IterSource::MapKeys(idx) => IterSource::MapKeys(*idx),
                        IterSource::MapValues(idx) => IterSource::MapValues(*idx),
                    };
                    (src, s.position)
                }
                None => return Some(Err(JvmError::InvalidReference)),
            };
            let len = source_len(ctx, &source_clone);
            if pos >= len {
                return Some(Err(JvmError::ArrayIndexOutOfBounds));
            }
            let value = source_get(ctx, &source_clone, pos);
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
        IterSource::MapKeys(buf_idx) | IterSource::MapValues(buf_idx) => {
            ctx.objects.map_len(*buf_idx)
        }
    }
}

/// Return the element at `position` from the source collection.
fn source_get(ctx: &NativeContext<'_>, source: &IterSource, pos: usize) -> Value {
    match source {
        IterSource::List(buf_idx) => ctx.objects.list_get(*buf_idx, pos).unwrap_or(Value::Null),
        IterSource::MapKeys(buf_idx) => ctx
            .objects
            .map_iter(*buf_idx)
            .nth(pos)
            .map(|(k, _)| k)
            .unwrap_or(Value::Null),
        IterSource::MapValues(buf_idx) => ctx
            .objects
            .map_iter(*buf_idx)
            .nth(pos)
            .map(|(_, v)| v)
            .unwrap_or(Value::Null),
    }
}
