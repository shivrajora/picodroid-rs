use crate::types::{JvmError, Value};

use super::NativeContext;

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => {
            ctx.objects.sb_push();
            // <init>(String): if a String argument was supplied, seed the buffer.
            if let Some(Value::Reference(idx)) = ctx.args.get(1) {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                ctx.objects.sb_append_bytes(s.as_bytes());
            }
            Some(Ok(None))
        }
        "append" => {
            match ctx.args.get(1) {
                Some(Value::Reference(idx)) => {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    ctx.objects.sb_append_bytes(s.as_bytes());
                }
                Some(Value::Int(n)) => {
                    let desc = ctx.descriptor;
                    if desc.starts_with("(C)") {
                        // append(char): emit the character as a single byte.
                        // Multi-byte Unicode chars are not supported on this platform.
                        let ch = (*n as u8).max(0x20); // replace non-printable with space
                        ctx.objects.sb_append_bytes(&[ch]);
                    } else if desc.starts_with("(Z)") {
                        // append(boolean)
                        ctx.objects
                            .sb_append_bytes(if *n != 0 { b"true" } else { b"false" });
                    } else {
                        ctx.objects.sb_append_int(*n);
                    }
                }
                Some(Value::Long(n)) => {
                    ctx.objects.sb_append_long(*n);
                }
                Some(Value::Float(f)) => {
                    ctx.objects.sb_append_float(*f);
                }
                Some(Value::Double(d)) => {
                    ctx.objects.sb_append_float(*d as f32);
                }
                _ => {}
            }
            // append() returns `this` for chaining.
            Some(Ok(ctx.args.first().copied().map(Some).unwrap_or(None)))
        }
        "length" => {
            let len = ctx.objects.sb_len() as i32;
            Some(Ok(Some(Value::Int(len))))
        }
        "charAt" => {
            if let Some(Value::Int(i)) = ctx.args.get(1) {
                let ch = ctx.objects.sb_char_at(*i as usize).unwrap_or(0);
                Some(Ok(Some(Value::Int(ch as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "toString" => {
            let bytes = ctx.objects.sb_pop();
            let str_ref = ctx
                .strings
                .intern_dyn(&bytes)
                .ok_or(JvmError::StackOverflow);
            Some(str_ref.map(|r| Some(Value::Reference(r))))
        }
        _ => None,
    }
}
