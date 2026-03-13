use super::Executor;
use crate::framework::{frame::Frame, native::NativeMethodHandler, types::JvmError};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_stack(&mut self, opcode: u8, frame: &mut Frame) -> Result<(), JvmError> {
        match opcode {
            // pop
            0x57 => {
                frame.pop()?;
            }

            // pop2
            0x58 => {
                frame.pop()?;
                frame.pop()?;
            }

            // dup
            0x59 => {
                let v = frame.pop()?;
                frame.push(v)?;
                frame.push(v)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
