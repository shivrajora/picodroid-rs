use super::Executor;
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_convert(&mut self, opcode: u8, frame: &mut Frame) -> Result<(), JvmError> {
        match opcode {
            // i2b — int to byte (sign-extend lower 8 bits)
            0x91 => {
                let v = frame.pop()?;
                if let Value::Int(n) = v {
                    frame.push(Value::Int(n as i8 as i32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // i2c — int to char (zero-extend lower 16 bits)
            0x92 => {
                let v = frame.pop()?;
                if let Value::Int(n) = v {
                    frame.push(Value::Int(n as u16 as i32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // i2s — int to short (sign-extend lower 16 bits)
            0x93 => {
                let v = frame.pop()?;
                if let Value::Int(n) = v {
                    frame.push(Value::Int(n as i16 as i32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
