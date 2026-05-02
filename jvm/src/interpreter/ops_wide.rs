// SPDX-License-Identifier: GPL-3.0-only
use super::Executor;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// `wide` (0xc4) — widen the next opcode's local-variable index from
    /// u8 to u16. For `iinc`, also widens the constant from i8 to i16.
    pub(super) fn op_wide(&mut self, code: &[u8], frame: &mut Frame) -> Result<(), JvmError> {
        let real_op = code[frame.pc];
        frame.pc += 1;
        let idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;

        match real_op {
            // iload / lload / fload / dload / aload
            0x15..=0x19 => {
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }
            // istore / lstore / fstore / dstore / astore
            0x36..=0x3a => {
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }
            // iinc — followed by a signed 16-bit constant
            0x84 => {
                let inc = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.load_local(idx)? {
                    frame.store_local(idx, Value::Int(n.wrapping_add(inc as i32)))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }
            _ => return Err(JvmError::UnsupportedOpcode(real_op)),
        }
        Ok(())
    }
}
