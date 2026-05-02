// SPDX-License-Identifier: GPL-3.0-only
//! Native methods for `java.lang.Class`.
//!
//! Class objects are allocated by `resolve_ldc` when bytecode does
//! `ldc CONSTANT_Class` (i.e. `MyClass.class`). Each instance stores the
//! JVM-internal class name as a `String name` field at slot 0; `getName`
//! returns it directly.

use crate::{
    native::NativeContext,
    types::{JvmError, Value},
};

pub fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        "<init>" => Some(Ok(None)),
        "getName" => {
            let Some(&Value::ObjectRef(this)) = ctx.args.first() else {
                return Some(Err(JvmError::InvalidReference));
            };
            match ctx.objects.get_field(this, 0) {
                Some(Value::Reference(idx)) => Some(Ok(Some(Value::Reference(idx)))),
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        _ => None,
    }
}
