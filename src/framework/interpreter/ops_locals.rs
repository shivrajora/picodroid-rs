use super::Executor;
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_locals_load(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // iload (index u8)
            0x15 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }

            // aload (index u8)
            0x19 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }

            // iload_<n>
            0x1a => {
                let v = frame.load_local(0)?;
                frame.push(v)?;
            }
            0x1b => {
                let v = frame.load_local(1)?;
                frame.push(v)?;
            }
            0x1c => {
                let v = frame.load_local(2)?;
                frame.push(v)?;
            }
            0x1d => {
                let v = frame.load_local(3)?;
                frame.push(v)?;
            }

            // aload_<n>
            0x2a => {
                let v = frame.load_local(0)?;
                frame.push(v)?;
            }
            0x2b => {
                let v = frame.load_local(1)?;
                frame.push(v)?;
            }
            0x2c => {
                let v = frame.load_local(2)?;
                frame.push(v)?;
            }
            0x2d => {
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
            // istore (index u8)
            0x36 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }

            // astore (index u8)
            0x3a => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }

            // istore_<n>
            0x3b => {
                let v = frame.pop()?;
                frame.store_local(0, v)?;
            }
            0x3c => {
                let v = frame.pop()?;
                frame.store_local(1, v)?;
            }
            0x3d => {
                let v = frame.pop()?;
                frame.store_local(2, v)?;
            }
            0x3e => {
                let v = frame.pop()?;
                frame.store_local(3, v)?;
            }

            // astore_<n>
            0x4b => {
                let v = frame.pop()?;
                frame.store_local(0, v)?;
            }
            0x4c => {
                let v = frame.pop()?;
                frame.store_local(1, v)?;
            }
            0x4d => {
                let v = frame.pop()?;
                frame.store_local(2, v)?;
            }
            0x4e => {
                let v = frame.pop()?;
                frame.store_local(3, v)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
