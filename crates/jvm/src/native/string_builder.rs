// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::NativeContext;
use super::INIT;
use crate::names::{c, mh};

/// Extract the byte-buffer index stored in field 0 of a StringBuilder receiver.
fn get_sb_buf(objects: &ObjectHeap, args: &[Value]) -> Result<u16, JvmError> {
    let Value::ObjectRef(obj_idx) = args.first().copied().unwrap_or(Value::Null) else {
        return Err(JvmError::InvalidReference);
    };
    match objects.get_field(obj_idx, 0) {
        Some(Value::Int(n)) => Ok(n as u16),
        _ => Err(JvmError::InvalidReference),
    }
}

pub(crate) fn dispatch(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // `<init>` allocates the buffer; every other method reads the index the
    // receiver already holds. Each builder owns its own buffer, so two
    // concurrently-alive builders never interleave.
    if method == INIT {
        let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
            return Some(Err(JvmError::InvalidReference));
        };
        let buf_idx = match ctx.objects.sb_alloc() {
            Some(i) => i,
            None => return Some(Err(JvmError::StackOverflow)),
        };
        if ctx
            .objects
            .set_field(obj_idx, 0, Value::Int(buf_idx as i32))
            .is_none()
        {
            return Some(Err(JvmError::StackOverflow));
        }
        // <init>(String): if a String argument was supplied, seed the buffer.
        if let Some(Value::Reference(idx)) = ctx.args.get(1) {
            let s = ctx.strings.resolve(*idx).unwrap_or("");
            if ctx.objects.sb_append_bytes(buf_idx, s.as_bytes()).is_err() {
                return Some(Err(super::throw_named(ctx, c::java_lang_OutOfMemoryError)));
            }
        }
        return Some(Ok(None));
    }

    let buf = match get_sb_buf(ctx.objects, ctx.args) {
        Ok(i) => i,
        Err(e) => return Some(Err(e)),
    };

    match method {
        mh::append => {
            let grown = match ctx.args.get(1) {
                Some(Value::Reference(idx)) => {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    ctx.objects.sb_append_bytes(buf, s.as_bytes())
                }
                Some(Value::Int(n)) => {
                    let desc = ctx.descriptor;
                    if desc.starts_with("(C)") {
                        // append(char): the bytes the String model holds for
                        // it (`string::char_bytes`: UTF-8 from U+0080 up).
                        let mut cb = [0u8; 3];
                        ctx.objects
                            .sb_append_bytes(buf, super::string::char_bytes(*n, &mut cb))
                    } else if desc.starts_with("(Z)") {
                        // append(boolean)
                        ctx.objects
                            .sb_append_bytes(buf, if *n != 0 { b"true" } else { b"false" })
                    } else {
                        ctx.objects.sb_append_int(buf, *n)
                    }
                }
                Some(Value::Long(n)) => ctx.objects.sb_append_long(buf, *n),
                Some(Value::Float(f)) => ctx.objects.sb_append_float(buf, *f),
                Some(Value::Double(d)) => ctx.objects.sb_append_double(buf, *d),
                // append(Object) with null. A non-null object never reaches
                // here: the interpreter stringifies it first (see
                // `Executor::stringify_object_arg`).
                Some(Value::Null) => ctx.objects.sb_append_bytes(buf, b"null"),
                _ => Ok(()),
            };
            if grown.is_err() {
                return Some(Err(super::throw_named(ctx, c::java_lang_OutOfMemoryError)));
            }
            // append() returns `this` for chaining.
            Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
        }
        mh::length => {
            let len = ctx.objects.sb_len(buf) as i32;
            Some(Ok(Some(Value::Int(len))))
        }
        mh::charAt => {
            if let Some(Value::Int(i)) = ctx.args.get(1) {
                match usize::try_from(*i)
                    .ok()
                    .and_then(|i| ctx.objects.sb_char_at(buf, i))
                {
                    Some(ch) => Some(Ok(Some(Value::Int(ch as i32)))),
                    None => Some(Err(super::throw_named(
                        ctx,
                        c::java_lang_StringIndexOutOfBoundsException,
                    ))),
                }
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::toString => {
            // Non-destructive, as on Android: the builder keeps its contents
            // and can be appended to (or stringified again) afterwards.
            let str_ref = ctx
                .strings
                .intern_dyn(ctx.objects.sb_contents_slice(buf))
                .ok_or(JvmError::StackOverflow);
            Some(str_ref.map(|r| Some(Value::Reference(r))))
        }
        _ => None,
    }
}
