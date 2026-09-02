// SPDX-License-Identifier: GPL-3.0-only
use super::Executor;
use crate::names::c;
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
        self.runtime_fault(c::java_lang_ClassCastException)
    }

    /// Allocate `class` by name and return it as a thrown exception —
    /// division by zero, null dereference, bad array index and friends are
    /// specified as catchable Java exceptions, not VM faults (bugbash J4).
    /// Same alloc-by-name/no-message tradeoff as `class_cast_exception`.
    pub(super) fn runtime_fault(&mut self, class: &'static str) -> JvmError {
        match self.objects.alloc(class) {
            Some(exc) => JvmError::Exception(exc),
            None => JvmError::StackOverflow,
        }
    }

    pub(super) fn arithmetic_exception(&mut self) -> JvmError {
        self.runtime_fault(c::java_lang_ArithmeticException)
    }

    pub(super) fn null_pointer_exception(&mut self) -> JvmError {
        self.runtime_fault(c::java_lang_NullPointerException)
    }

    /// athrow (0xbf): pop an object reference and throw it as an exception.
    /// Returns `Err(JvmError::Exception(obj_idx))` so the interpreter loop can
    /// search the current frame's exception table or propagate to the caller.
    pub(super) fn op_athrow(&mut self, frame: &mut Frame) -> Result<(), JvmError> {
        let val = frame.pop()?;
        match val {
            Value::ObjectRef(idx) => Err(JvmError::Exception(idx)),
            // JVMS: athrow of null throws NullPointerException.
            Value::Null => Err(self.null_pointer_exception()),
            _ => Err(JvmError::InvalidBytecode),
        }
    }
}
