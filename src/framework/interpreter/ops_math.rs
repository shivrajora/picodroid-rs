use super::Executor;
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_math(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // iadd
            0x60 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_add(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // isub
            0x64 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_sub(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // imul
            0x68 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_mul(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // idiv
            0x6c => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(_), Value::Int(0)) => return Err(JvmError::InvalidBytecode),
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_div(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // irem
            0x70 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(_), Value::Int(0)) => return Err(JvmError::InvalidBytecode),
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_rem(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ineg
            0x74 => {
                let v = frame.pop()?;
                match v {
                    Value::Int(n) => frame.push(Value::Int(n.wrapping_neg()))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ishl
            0x78 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => {
                        frame.push(Value::Int(a.wrapping_shl((b & 0x1f) as u32)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ishr
            0x7a => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => {
                        frame.push(Value::Int(a.wrapping_shr((b & 0x1f) as u32)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // iushr
            0x7c => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(
                        ((a as u32).wrapping_shr((b & 0x1f) as u32)) as i32,
                    ))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // iand
            0x7e => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a & b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ior
            0x80 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a | b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ixor
            0x82 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a ^ b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // iinc: local[index] += const
            0x84 => {
                let idx = code[frame.pc];
                let inc = code[frame.pc + 1] as i8;
                frame.pc += 2;
                if let Value::Int(n) = frame.load_local(idx)? {
                    frame.store_local(idx, Value::Int(n.wrapping_add(inc as i32)))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
