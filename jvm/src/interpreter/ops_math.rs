use super::Executor;
use crate::{
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

            // fadd
            0x62 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => frame.push(Value::Float(a + b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // fsub
            0x66 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => frame.push(Value::Float(a - b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // fmul
            0x6a => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => frame.push(Value::Float(a * b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // fdiv
            0x6e => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => frame.push(Value::Float(a / b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // frem
            0x72 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Float(a), Value::Float(b)) => {
                        frame.push(Value::Float(libm::fmodf(a, b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // fneg
            0x76 => {
                let v = frame.pop()?;
                match v {
                    Value::Float(f) => frame.push(Value::Float(-f))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ladd
            0x61 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => {
                        frame.push(Value::Long(a.wrapping_add(b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lsub
            0x65 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => {
                        frame.push(Value::Long(a.wrapping_sub(b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lmul
            0x69 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => {
                        frame.push(Value::Long(a.wrapping_mul(b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ldiv
            0x6d => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(_), Value::Long(0)) => return Err(JvmError::InvalidBytecode),
                    (Value::Long(a), Value::Long(b)) => {
                        frame.push(Value::Long(a.wrapping_div(b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lrem
            0x71 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(_), Value::Long(0)) => return Err(JvmError::InvalidBytecode),
                    (Value::Long(a), Value::Long(b)) => {
                        frame.push(Value::Long(a.wrapping_rem(b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lneg
            0x75 => {
                let v = frame.pop()?;
                match v {
                    Value::Long(n) => frame.push(Value::Long(n.wrapping_neg()))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lshl — shift amount is Int, value is Long
            0x79 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Int(b)) => {
                        frame.push(Value::Long(a.wrapping_shl((b & 0x3f) as u32)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lshr — arithmetic shift right
            0x7b => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Int(b)) => {
                        frame.push(Value::Long(a.wrapping_shr((b & 0x3f) as u32)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lushr — logical (unsigned) shift right
            0x7d => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Int(b)) => frame.push(Value::Long(
                        ((a as u64).wrapping_shr((b & 0x3f) as u32)) as i64,
                    ))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // land
            0x7f => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => frame.push(Value::Long(a & b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lor
            0x81 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => frame.push(Value::Long(a | b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // lxor
            0x83 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Long(a), Value::Long(b)) => frame.push(Value::Long(a ^ b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // dadd
            0x63 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Double(a), Value::Double(b)) => frame.push(Value::Double(a + b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // dsub
            0x67 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Double(a), Value::Double(b)) => frame.push(Value::Double(a - b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // dmul
            0x6b => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Double(a), Value::Double(b)) => frame.push(Value::Double(a * b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // ddiv
            0x6f => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Double(a), Value::Double(b)) => frame.push(Value::Double(a / b))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // drem
            0x73 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Double(a), Value::Double(b)) => {
                        frame.push(Value::Double(libm::fmod(a, b)))?
                    }
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // dneg
            0x77 => {
                let v = frame.pop()?;
                match v {
                    Value::Double(d) => frame.push(Value::Double(-d))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // iinc: local[index] += const
            0x84 => {
                let idx = code[frame.pc] as u16;
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
