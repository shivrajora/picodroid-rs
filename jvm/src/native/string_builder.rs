// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

use super::NativeContext;

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
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // `<init>` allocates the buffer; every other method reads the index the
    // receiver already holds. Each builder owns its own buffer, so two
    // concurrently-alive builders never interleave.
    if method_name == "<init>" {
        let Value::ObjectRef(obj_idx) = ctx.args.first().copied().unwrap_or(Value::Null) else {
            return Some(Err(JvmError::InvalidReference));
        };
        let buf_idx = match ctx.objects.sb_alloc() {
            Some(i) => i,
            None => return Some(Err(JvmError::StackOverflow)),
        };
        ctx.objects
            .set_field(obj_idx, 0, Value::Int(buf_idx as i32));
        // <init>(String): if a String argument was supplied, seed the buffer.
        if let Some(Value::Reference(idx)) = ctx.args.get(1) {
            let s = ctx.strings.resolve(*idx).unwrap_or("");
            ctx.objects.sb_append_bytes(buf_idx, s.as_bytes());
        }
        return Some(Ok(None));
    }

    let buf = match get_sb_buf(ctx.objects, ctx.args) {
        Ok(i) => i,
        Err(e) => return Some(Err(e)),
    };

    match method_name {
        "append" => {
            match ctx.args.get(1) {
                Some(Value::Reference(idx)) => {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    ctx.objects.sb_append_bytes(buf, s.as_bytes());
                }
                Some(Value::Int(n)) => {
                    let desc = ctx.descriptor;
                    if desc.starts_with("(C)") {
                        // append(char): emit the character as a single byte.
                        // Multi-byte Unicode chars are not supported on this platform.
                        // Whitespace controls (`\t \n \r`) pass through verbatim —
                        // Java's append('\n') must yield a newline (StringBuilder
                        // line-joining, AlertDialog item lists); only other
                        // sub-0x20 controls become a space to avoid garbage glyphs.
                        let b = *n as u8;
                        let ch = if b >= 0x20 || b == b'\n' || b == b'\t' || b == b'\r' {
                            b
                        } else {
                            b' '
                        };
                        ctx.objects.sb_append_bytes(buf, &[ch]);
                    } else if desc.starts_with("(Z)") {
                        // append(boolean)
                        ctx.objects
                            .sb_append_bytes(buf, if *n != 0 { b"true" } else { b"false" });
                    } else {
                        ctx.objects.sb_append_int(buf, *n);
                    }
                }
                Some(Value::Long(n)) => {
                    ctx.objects.sb_append_long(buf, *n);
                }
                Some(Value::Float(f)) => {
                    ctx.objects.sb_append_float(buf, *f);
                }
                Some(Value::Double(d)) => {
                    ctx.objects.sb_append_float(buf, *d as f32);
                }
                _ => {}
            }
            // append() returns `this` for chaining.
            Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
        }
        "length" => {
            let len = ctx.objects.sb_len(buf) as i32;
            Some(Ok(Some(Value::Int(len))))
        }
        "charAt" => {
            if let Some(Value::Int(i)) = ctx.args.get(1) {
                let ch = ctx.objects.sb_char_at(buf, *i as usize).unwrap_or(0);
                Some(Ok(Some(Value::Int(ch as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "toString" => {
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
