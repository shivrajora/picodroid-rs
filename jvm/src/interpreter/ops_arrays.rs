use super::Executor;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_array_load(&mut self, opcode: u8, frame: &mut Frame) -> Result<(), JvmError> {
        match opcode {
            // iaload — load int from int array
            0x2e => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let v = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Int(v))?;
            }

            // faload — load float from float array
            0x30 => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Float(f32::from_bits(raw as u32)))?;
            }

            // aaload — load reference from reference array
            0x32 => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                // Encoding: 0 = Null, positive = ObjectRef, REF_TAG-bit set = Reference.
                let v = if raw == 0 {
                    Value::Null
                } else if (raw as u32) & crate::array_heap::REF_TAG != 0 {
                    Value::Reference(((raw as u32) & !crate::array_heap::REF_TAG) as u16)
                } else {
                    Value::ObjectRef(raw as u16)
                };
                frame.push(v)?;
            }

            // baload — load byte/boolean from array (sign-extend)
            0x33 => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Int(raw as i8 as i32))?;
            }

            // caload — load char from array (zero-extend)
            0x34 => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Int(raw as u16 as i32))?;
            }

            // saload — load short from array (sign-extend)
            0x35 => {
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = self
                    .arrays
                    .load(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Int(raw as i16 as i32))?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }

    pub(super) fn op_array_store(&mut self, opcode: u8, frame: &mut Frame) -> Result<(), JvmError> {
        match opcode {
            // iastore — store int into int array
            0x4f => {
                let value = match frame.pop()? {
                    Value::Int(v) => v,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                self.arrays
                    .store(arr_idx, index as usize, value)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            // fastore — store float into float array
            0x51 => {
                let value = match frame.pop()? {
                    Value::Float(v) => v,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                self.arrays
                    .store(arr_idx, index as usize, value.to_bits() as i32)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            // aastore — store reference into reference array
            0x53 => {
                let value = frame.pop()?;
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                let raw = match value {
                    Value::Null => 0i32,
                    Value::ObjectRef(i) => i as i32,
                    Value::Reference(i) => ((i as u32) | crate::array_heap::REF_TAG) as i32,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                self.arrays
                    .store(arr_idx, index as usize, raw)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            // bastore — store byte/boolean into array (truncate to 8 bits)
            0x54 => {
                let value = match frame.pop()? {
                    Value::Int(v) => v,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                self.arrays
                    .store(arr_idx, index as usize, value as i8 as i32)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            // castore — store char into array (truncate to 16 bits)
            0x55 => {
                let value = match frame.pop()? {
                    Value::Int(v) => v,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                self.arrays
                    .store(arr_idx, index as usize, value as u16 as i32)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            // sastore — store short into array (truncate to 16 bits, sign-extended)
            0x56 => {
                let value = match frame.pop()? {
                    Value::Int(v) => v,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let index = match frame.pop()? {
                    Value::Int(i) => i,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if index < 0 {
                    return Err(JvmError::ArrayIndexOutOfBounds);
                }
                self.arrays
                    .store(arr_idx, index as usize, value as i16 as i32)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }

    pub(super) fn op_array_alloc(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // newarray — create new primitive array
            0xbc => {
                let atype = code[frame.pc];
                frame.pc += 1;
                let count = match frame.pop()? {
                    Value::Int(n) => n,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if count < 0 {
                    return Err(JvmError::NegativeArraySize);
                }
                match self.arrays.alloc(atype, count as u16) {
                    Some(arr_idx) => {
                        self.alloc_count = self.alloc_count.saturating_add(1);
                        frame.push(Value::ArrayRef(arr_idx))?;
                    }
                    None => {
                        // OOM — rewind to inst_pc so the main loop can GC and
                        // re-execute this opcode.
                        frame.pc = frame.inst_pc;
                        frame.push(Value::Int(count))?;
                        self.need_gc = true;
                        return Ok(());
                    }
                }
            }

            // anewarray — create new reference array (class cp_idx consumed but ignored)
            0xbd => {
                frame.pc += 2; // skip cp_idx
                let count = match frame.pop()? {
                    Value::Int(n) => n,
                    _ => return Err(JvmError::InvalidBytecode),
                };
                if count < 0 {
                    return Err(JvmError::NegativeArraySize);
                }
                match self
                    .arrays
                    .alloc(crate::array_heap::ATYPE_REF, count as u16)
                {
                    Some(arr_idx) => {
                        self.alloc_count = self.alloc_count.saturating_add(1);
                        frame.push(Value::ArrayRef(arr_idx))?;
                    }
                    None => {
                        frame.pc = frame.inst_pc;
                        frame.push(Value::Int(count))?;
                        self.need_gc = true;
                        return Ok(());
                    }
                }
            }

            // arraylength — get length of array
            0xbe => {
                let arr_idx = match frame.pop()? {
                    Value::ArrayRef(i) => i,
                    Value::Null => return Err(JvmError::InvalidReference),
                    _ => return Err(JvmError::InvalidBytecode),
                };
                let len = self
                    .arrays
                    .length(arr_idx)
                    .ok_or(JvmError::InvalidReference)?;
                frame.push(Value::Int(len as i32))?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
