use super::{helpers, Executor};
use crate::framework::{
    class_file::ClassFile,
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_constants(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // nop
            0x00 => {}

            // aconst_null
            0x01 => frame.push(Value::Null)?,

            // iconst_<n>: -1..5
            0x02 => frame.push(Value::Int(-1))?,
            0x03 => frame.push(Value::Int(0))?,
            0x04 => frame.push(Value::Int(1))?,
            0x05 => frame.push(Value::Int(2))?,
            0x06 => frame.push(Value::Int(3))?,
            0x07 => frame.push(Value::Int(4))?,
            0x08 => frame.push(Value::Int(5))?,

            // bipush
            0x10 => {
                let b = code[frame.pc] as i8;
                frame.pc += 1;
                frame.push(Value::Int(b as i32))?;
            }

            // sipush
            0x11 => {
                let hi = code[frame.pc] as i16;
                let lo = code[frame.pc + 1] as i16;
                frame.pc += 2;
                frame.push(Value::Int(((hi << 8) | lo) as i32))?;
            }

            // ldc
            0x12 => {
                let cp_idx = code[frame.pc] as u16;
                frame.pc += 1;
                let cf = &self.classes[frame.class_idx];
                let v = helpers::resolve_ldc(cf, self.strings, cp_idx)?;
                frame.push(v)?;
            }

            // ldc_w
            0x13 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let cf = &self.classes[frame.class_idx];
                let v = helpers::resolve_ldc(cf, self.strings, cp_idx)?;
                frame.push(v)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
