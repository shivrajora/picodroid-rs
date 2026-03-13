use super::Executor;
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
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
