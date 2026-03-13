use super::Executor;
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_fields(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // getstatic — for M2 we only see this for PeripheralManager; push Null placeholder
            0xb2 => {
                frame.pc += 2;
                frame.push(Value::Null)?;
            }

            // getfield — objectref → value
            0xb4 => {
                let _cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        let v = self.objects.get_field(idx, 0).unwrap_or(Value::Null);
                        frame.push(v)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            // putfield — objectref, value →
            0xb5 => {
                let _cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let value = frame.pop()?;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        self.objects
                            .set_field(idx, 0, value)
                            .ok_or(JvmError::InvalidReference)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
