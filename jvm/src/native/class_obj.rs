// SPDX-License-Identifier: GPL-3.0-only
//! Native methods for `java.lang.Class`.
//!
//! Class objects are allocated by `resolve_ldc` when bytecode does
//! `ldc CONSTANT_Class` (i.e. `MyClass.class`). Each instance stores the
//! JVM-internal slash-form class name as a `String name` field at slot 0;
//! `getName` returns the Java-spec dot-form, lazily converted and cached in
//! slot 1 (the cache rides an object field, so the GC's normal field scan
//! keeps the converted string alive with its Class object).

use alloc::vec::Vec;

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
            // Cached dot-form from a previous call.
            if let Some(Value::Reference(cached)) = ctx.objects.get_field(this, 1) {
                return Some(Ok(Some(Value::Reference(cached))));
            }
            let Some(Value::Reference(raw)) = ctx.objects.get_field(this, 0) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let Some(name) = ctx.strings.resolve(raw) else {
                return Some(Err(JvmError::InvalidReference));
            };
            // Names without a package separator (primitives, single-segment
            // classes) are already in dot-form — cache the raw index.
            if !name.contains('/') {
                let _ = ctx.objects.set_field(this, 1, Value::Reference(raw));
                return Some(Ok(Some(Value::Reference(raw))));
            }
            let dotted: Vec<u8> = name
                .bytes()
                .map(|b| if b == b'/' { b'.' } else { b })
                .collect();
            let Some(idx) = ctx.strings.intern_dyn(&dotted) else {
                return Some(Err(JvmError::StackOverflow));
            };
            let _ = ctx.objects.set_field(this, 1, Value::Reference(idx));
            Some(Ok(Some(Value::Reference(idx))))
        }
        _ => None,
    }
}
