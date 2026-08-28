// SPDX-License-Identifier: GPL-3.0-only
use super::Executor;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// A failed `checkcast`: allocate a `java/lang/ClassCastException` and
    /// return it as a thrown exception, so `catch (ClassCastException e)`
    /// matches (alloc-by-name; `builtin_super` supplies the RuntimeException
    /// chain). No message: Java's "`X cannot be cast to Y`" costs ~250 B of
    /// flash to format, and the stack trace already names the cast site.
    /// Allocation failure degrades to `StackOverflow` like every native throw.
    pub(super) fn class_cast_exception(&mut self) -> JvmError {
        match self.objects.alloc("java/lang/ClassCastException") {
            Some(exc) => JvmError::Exception(exc),
            None => JvmError::StackOverflow,
        }
    }

    /// athrow (0xbf): pop an object reference and throw it as an exception.
    /// Returns `Err(JvmError::Exception(obj_idx))` so the interpreter loop can
    /// search the current frame's exception table or propagate to the caller.
    pub(super) fn op_athrow(&mut self, frame: &mut Frame) -> Result<(), JvmError> {
        let val = frame.pop()?;
        match val {
            Value::ObjectRef(idx) => Err(JvmError::Exception(idx)),
            Value::Null => Err(JvmError::InvalidReference),
            _ => Err(JvmError::InvalidBytecode),
        }
    }
}
