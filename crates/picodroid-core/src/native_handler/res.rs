// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.content.res.Resources` and `LayoutInflater.nativeWord`: reads
//! of the app's compiled resource table (`crate::resources`).
use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

fn arg_int(ctx: &NativeContext<'_>, i: usize) -> Result<i32, JvmError> {
    match ctx.args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

/// A message buffer that lives on the stack; text past the end is dropped.
struct StackText {
    buf: [u8; 48],
    len: usize,
}

impl StackText {
    const fn new() -> Self {
        Self {
            buf: [0; 48],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl core::fmt::Write for StackText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

/// `Resources.NotFoundException`, with the id the way `R.java` prints it.
pub(crate) fn not_found(ctx: &mut NativeContext<'_>, what: &str, id: i32) -> JvmError {
    // Formatted on the stack: this path must work on a full heap, where an
    // allocating `format!` is a board reset rather than an exception.
    let mut msg = StackText::new();
    let _ = core::fmt::write(&mut msg, format_args!("{what} resource ID #0x{id:08x}"));
    super::throw_exception(
        ctx,
        c::picodroid_content_res_Resources_NotFoundException,
        msg.as_str(),
    )
}

fn lookup<T>(
    ctx: &mut NativeContext<'_>,
    what: &str,
    get: fn(i32) -> Option<T>,
    wrap: fn(T) -> Value,
) -> Result<Option<Value>, JvmError> {
    let id = arg_int(ctx, 1)?;
    match get(id) {
        Some(v) => Ok(Some(wrap(v))),
        None => Err(not_found(ctx, what, id)),
    }
}

fn get_string(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let id = arg_int(ctx, 1)?;
    let Some(bytes) = crate::resources::string(id) else {
        return Err(not_found(ctx, "String", id));
    };
    match ctx.strings.intern_dyn(bytes) {
        Some(idx) => Ok(Some(Value::Reference(idx))),
        None => Err(JvmError::StackOverflow),
    }
}

/// Static `(layout, index)`. A miss on word 0 is a bad layout id; past that
/// it would be a truncated stream, which the compiler never writes.
fn layout_word(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let (id, index) = (arg_int(ctx, 0)?, arg_int(ctx, 1)?);
    match crate::resources::layout_word(id, index) {
        Some(w) => Ok(Some(Value::Int(w))),
        None => Err(not_found(ctx, "Layout", id)),
    }
}

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    use crate::resources as res;
    Some(match (class_name, method_name) {
        (c::picodroid_content_res_Resources, m::getString) => get_string(ctx),
        (c::picodroid_content_res_Resources, m::getColor) => {
            lookup(ctx, "Color", res::color, Value::Int)
        }
        (c::picodroid_content_res_Resources, m::getDimension) => {
            lookup(ctx, "Dimension", res::dimension, Value::Float)
        }
        (c::picodroid_content_res_Resources, m::getInteger) => {
            lookup(ctx, "Integer", res::integer, Value::Int)
        }
        (c::picodroid_content_res_Resources, m::getBoolean) => {
            lookup(ctx, "Boolean", res::boolean, |b| Value::Int(b as i32))
        }
        (c::picodroid_view_LayoutInflater, m::nativeWord) => layout_word(ctx),
        _ => return None,
    })
}
