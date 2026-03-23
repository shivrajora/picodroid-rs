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

            // i2l — int to long
            0x85 => {
                let v = frame.pop()?;
                if let Value::Int(n) = v {
                    frame.push(Value::Long(n as i64))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // i2f — int to float
            0x86 => {
                let v = frame.pop()?;
                if let Value::Int(n) = v {
                    frame.push(Value::Float(n as f32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // l2i — long to int (truncate lower 32 bits)
            0x88 => {
                let v = frame.pop()?;
                if let Value::Long(n) = v {
                    frame.push(Value::Int(n as i32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // l2f — long to float
            0x89 => {
                let v = frame.pop()?;
                if let Value::Long(n) = v {
                    frame.push(Value::Float(n as f32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // f2i — float to int (truncate toward zero, JVM spec)
            0x8b => {
                let v = frame.pop()?;
                if let Value::Float(f) = v {
                    frame.push(Value::Int(f as i32))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // f2l — float to long (truncate toward zero)
            0x8c => {
                let v = frame.pop()?;
                if let Value::Float(f) = v {
                    frame.push(Value::Long(f as i64))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // lcmp — compare two longs; push Int(-1 / 0 / 1)
            0x94 => match (frame.pop()?, frame.pop()?) {
                (Value::Long(b), Value::Long(a)) => {
                    let r = if a > b {
                        1
                    } else if a == b {
                        0
                    } else {
                        -1
                    };
                    frame.push(Value::Int(r))?;
                }
                _ => return Err(JvmError::InvalidBytecode),
            },

            // fcmpl — float compare, NaN → -1
            0x95 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => {
                        // a < b and NaN both map to -1 (JVM spec)
                        let result = if a > b {
                            1
                        } else if a == b {
                            0
                        } else {
                            -1
                        };
                        frame.push(Value::Int(result))?;
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // fcmpg — float compare, NaN → +1
            0x96 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => {
                        let result = if a > b {
                            1
                        } else if a == b {
                            0
                        } else if a < b {
                            -1
                        } else {
                            1 // NaN
                        };
                        frame.push(Value::Int(result))?;
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
