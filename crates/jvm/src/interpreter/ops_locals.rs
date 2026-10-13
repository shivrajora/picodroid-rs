// SPDX-License-Identifier: GPL-3.0-only
use super::Executor;
use crate::{frame::Frame, native::NativeMethodHandler, types::JvmError};

// The typed load/store forms (`iload`, `lload`, `fload`, `dload`, `aload`
// and their `_<n>` shorthands) all move one `Value` between a local and the
// stack — the slot's own `Value` carries the type — so the five forms of
// each index share one arm. Separate arms compiled to separate, identical
// copies; an or-pattern runs exactly the same code per opcode.
impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_locals_load(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // iload, lload, fload, dload, aload (index u8)
            0x15..=0x19 => {
                let idx = code[frame.pc] as u16;
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }
            // iload_<n>, lload_<n>, fload_<n>, dload_<n>, aload_<n>
            0x1a | 0x1e | 0x22 | 0x26 | 0x2a => {
                let v = frame.load_local(0)?;
                frame.push(v)?;
            }
            0x1b | 0x1f | 0x23 | 0x27 | 0x2b => {
                let v = frame.load_local(1)?;
                frame.push(v)?;
            }
            0x1c | 0x20 | 0x24 | 0x28 | 0x2c => {
                let v = frame.load_local(2)?;
                frame.push(v)?;
            }
            0x1d | 0x21 | 0x25 | 0x29 | 0x2d => {
                let v = frame.load_local(3)?;
                frame.push(v)?;
            }
            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }

    pub(super) fn op_locals_store(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // istore, lstore, fstore, dstore, astore (index u8)
            0x36..=0x3a => {
                let idx = code[frame.pc] as u16;
                frame.pc += 1;
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }
            // istore_<n>, lstore_<n>, fstore_<n>, dstore_<n>, astore_<n>
            0x3b | 0x3f | 0x43 | 0x47 | 0x4b => {
                let v = frame.pop()?;
                frame.store_local(0, v)?;
            }
            0x3c | 0x40 | 0x44 | 0x48 | 0x4c => {
                let v = frame.pop()?;
                frame.store_local(1, v)?;
            }
            0x3d | 0x41 | 0x45 | 0x49 | 0x4d => {
                let v = frame.pop()?;
                frame.store_local(2, v)?;
            }
            0x3e | 0x42 | 0x46 | 0x4a | 0x4e => {
                let v = frame.pop()?;
                frame.store_local(3, v)?;
            }
            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
