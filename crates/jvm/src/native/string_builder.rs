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
        mh::insert => {
            let Some(Value::Int(at)) = ctx.args.get(1) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let Ok(at) = usize::try_from(*at) else {
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_StringIndexOutOfBoundsException,
                )));
            };
            // The value's bytes, as `append` would write them.
            let (mut cb, mut ib, mut lb, mut fb) = ([0u8; 3], [0u8; 12], [0u8; 21], [0u8; 32]);
            let bytes: &[u8] = match ctx.args.get(2) {
                Some(Value::Reference(idx)) => ctx.strings.resolve(*idx).unwrap_or("").as_bytes(),
                Some(Value::Int(n)) => {
                    let desc = ctx.descriptor;
                    if desc.starts_with("(IC)") {
                        super::string::char_bytes(*n, &mut cb)
                    } else if desc.starts_with("(IZ)") {
                        if *n != 0 {
                            b"true"
                        } else {
                            b"false"
                        }
                    } else {
                        crate::object_heap::int_to_decimal_buf(*n, &mut ib)
                    }
                }
                Some(Value::Long(n)) => crate::object_heap::long_to_decimal_buf(*n, &mut lb),
                Some(Value::Float(f)) => crate::object_heap::float_to_str_buf(*f, &mut fb),
                Some(Value::Double(d)) => crate::object_heap::double_to_str_buf(*d, &mut fb),
                Some(Value::Null) => b"null",
                _ => b"",
            };
            match ctx.objects.sb_insert_bytes(buf, at, bytes) {
                Ok(true) => Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None))),
                Ok(false) => Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_StringIndexOutOfBoundsException,
                ))),
                Err(_) => Some(Err(super::throw_named(ctx, c::java_lang_OutOfMemoryError))),
            }
        }
        mh::setLength => {
            let Some(Value::Int(len)) = ctx.args.get(1) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let Ok(len) = usize::try_from(*len) else {
                return Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_StringIndexOutOfBoundsException,
                )));
            };
            if ctx.objects.sb_set_len(buf, len).is_err() {
                return Some(Err(super::throw_named(ctx, c::java_lang_OutOfMemoryError)));
            }
            Some(Ok(None))
        }
        mh::deleteCharAt | mh::delete => {
            let (start, end) = match (ctx.args.get(1), ctx.args.get(2)) {
                (Some(Value::Int(i)), None) => (*i, i.wrapping_add(1)),
                (Some(Value::Int(s)), Some(Value::Int(e))) => (*s, *e),
                _ => return Some(Err(JvmError::InvalidReference)),
            };
            // deleteCharAt(len) is an error where delete(len, len) is not:
            // only the two-argument form clamps `end` to the length.
            let in_range = start >= 0
                && end >= start
                && (method == mh::delete || (start as usize) < ctx.objects.sb_len(buf))
                && ctx.objects.sb_delete(buf, start as usize, end as usize);
            if in_range {
                Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
            } else {
                Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_StringIndexOutOfBoundsException,
                )))
            }
        }
        mh::setCharAt => {
            let (Some(Value::Int(i)), Some(Value::Int(ch))) = (ctx.args.get(1), ctx.args.get(2))
            else {
                return Some(Err(JvmError::InvalidReference));
            };
            // One byte per char in the byte string model; a code point above
            // 0x7F is stored as its low byte, as `charAt` reads bytes.
            let ok = usize::try_from(*i).is_ok_and(|i| ctx.objects.sb_set_byte(buf, i, *ch as u8));
            if ok {
                Some(Ok(None))
            } else {
                Some(Err(super::throw_named(
                    ctx,
                    c::java_lang_StringIndexOutOfBoundsException,
                )))
            }
        }
        mh::reverse => {
            ctx.objects.sb_reverse(buf);
            Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
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
