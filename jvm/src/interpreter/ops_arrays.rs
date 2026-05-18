// SPDX-License-Identifier: GPL-3.0-only
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

            // laload — load long from long array
            0x2f => {
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
                    .load64(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Long(v))?;
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

            // daload — load double from double array
            0x31 => {
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
                    .load64(arr_idx, index as usize)
                    .ok_or(JvmError::ArrayIndexOutOfBounds)?;
                frame.push(Value::Double(f64::from_bits(raw as u64)))?;
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
                // Encoding: 0 = Null, positive untagged = ObjectRef,
                // REF_TAG set = Reference, ARRAY_TAG set = ArrayRef.
                let u = raw as u32;
                let v = if raw == 0 {
                    Value::Null
                } else if u & crate::array_heap::REF_TAG != 0 {
                    Value::Reference((u & !crate::array_heap::REF_TAG) as u16)
                } else if u & crate::array_heap::ARRAY_TAG != 0 {
                    Value::ArrayRef((u & !crate::array_heap::ARRAY_TAG) as u16)
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

            // lastore — store long into long array
            0x50 => {
                let value = match frame.pop()? {
                    Value::Long(v) => v,
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
                    .store64(arr_idx, index as usize, value)
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

            // dastore — store double into double array
            0x52 => {
                let value = match frame.pop()? {
                    Value::Double(v) => v,
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
                    .store64(arr_idx, index as usize, value.to_bits() as i64)
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
                    Value::ArrayRef(i) => ((i as u32) | crate::array_heap::ARRAY_TAG) as i32,
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
                        self.bump_alloc_count(1);
                        frame.push(Value::ArrayRef(arr_idx))?;
                    }
                    None => {
                        // OOM — rewind to inst_pc so the main loop can GC and
                        // re-execute this opcode.
                        frame.pc = frame.inst_pc;
                        frame.push(Value::Int(count))?;
                        self.set_need_gc(true);
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
                        self.bump_alloc_count(1);
                        frame.push(Value::ArrayRef(arr_idx))?;
                    }
                    None => {
                        frame.pc = frame.inst_pc;
                        frame.push(Value::Int(count))?;
                        self.set_need_gc(true);
                        return Ok(());
                    }
                }
            }

            // multianewarray — create multi-dimensional array
            // Operands: u16 class-cp-index (descriptor like "[[I"), u8 dimensions.
            // Stack before: .., count_outer, ..., count_inner
            // Stack after:  .., arrayref
            0xc5 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                let dims = code[frame.pc + 2];
                frame.pc += 3;

                if dims == 0 {
                    return Err(JvmError::InvalidBytecode);
                }

                // Pop counts. Stack order is outermost...innermost (top),
                // so pop gives innermost first; reverse to get [outer..inner].
                let mut counts: alloc::vec::Vec<i32> =
                    alloc::vec::Vec::with_capacity(dims as usize);
                for _ in 0..dims {
                    match frame.pop()? {
                        Value::Int(n) => counts.push(n),
                        _ => return Err(JvmError::InvalidBytecode),
                    }
                }
                counts.reverse();

                for &c in &counts {
                    if c < 0 {
                        return Err(JvmError::NegativeArraySize);
                    }
                }

                // Resolve the array descriptor to determine the innermost atype.
                let cf = &self.classes[frame.class_idx];
                let desc = cf.cp_class_name(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                let inner_atype = innermost_atype(desc, dims);

                match alloc_multi(self.arrays, &counts, 0, inner_atype) {
                    Some(arr_idx) => {
                        self.bump_alloc_count(counts.len() as u16);
                        frame.push(Value::ArrayRef(arr_idx))?;
                    }
                    None => {
                        // OOM: rewind and push counts back in original stack order
                        // (outermost at bottom, innermost at top).
                        frame.pc = frame.inst_pc;
                        for c in counts {
                            frame.push(Value::Int(c))?;
                        }
                        self.set_need_gc(true);
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

/// Return the atype to use for the innermost array level of a multianewarray.
/// `desc` is a JVM class descriptor like `[[I` or `[[[Ljava/lang/Object;`.
/// If `dims` is less than the descriptor's bracket count, the innermost level
/// we allocate is itself an array-of-arrays (ATYPE_REF).
fn innermost_atype(desc: &[u8], dims: u8) -> u8 {
    let bracket_count = desc.iter().take_while(|&&b| b == b'[').count();
    if (dims as usize) < bracket_count {
        return crate::array_heap::ATYPE_REF;
    }
    match desc.get(bracket_count) {
        Some(b'I') => crate::array_heap::ATYPE_INT,
        Some(b'J') => crate::array_heap::ATYPE_LONG,
        Some(b'F') => crate::array_heap::ATYPE_FLOAT,
        Some(b'D') => crate::array_heap::ATYPE_DOUBLE,
        Some(b'B') => crate::array_heap::ATYPE_BYTE,
        Some(b'C') => crate::array_heap::ATYPE_CHAR,
        Some(b'S') => crate::array_heap::ATYPE_SHORT,
        Some(b'Z') => crate::array_heap::ATYPE_BOOLEAN,
        _ => crate::array_heap::ATYPE_REF,
    }
}

/// Recursively allocate a multi-dimensional array and wire child arrays into
/// parents. Returns the outermost array's heap index, or `None` on OOM at
/// any level (caller must rewind).
fn alloc_multi(
    arrays: &mut crate::array_heap::ArrayHeap,
    counts: &[i32],
    depth: usize,
    inner_atype: u8,
) -> Option<u16> {
    let count = counts[depth] as u16;
    let is_innermost = depth + 1 == counts.len();
    let atype = if is_innermost {
        inner_atype
    } else {
        crate::array_heap::ATYPE_REF
    };
    let arr_idx = arrays.alloc(atype, count)?;
    if !is_innermost {
        for i in 0..(count as usize) {
            let child = alloc_multi(arrays, counts, depth + 1, inner_atype)?;
            let raw = ((child as u32) | crate::array_heap::ARRAY_TAG) as i32;
            arrays.store(arr_idx, i, raw)?;
        }
    }
    Some(arr_idx)
}
