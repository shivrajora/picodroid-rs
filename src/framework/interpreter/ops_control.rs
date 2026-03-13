use super::{helpers, Executor};
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_control(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // ifeq: branch if TOS == 0
            0x99 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = frame.pop()?;
                if matches!(v, Value::Int(0) | Value::Null) {
                    frame.pc = helpers::branch_target(frame.pc, offset);
                }
            }

            // ifne: branch if TOS != 0
            0x9a => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = frame.pop()?;
                if !matches!(v, Value::Int(0) | Value::Null) {
                    frame.pc = helpers::branch_target(frame.pc, offset);
                }
            }

            // iflt
            0x9b => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n < 0 {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // ifge
            0x9c => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n >= 0 {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // ifgt
            0x9d => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n > 0 {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // ifle
            0x9e => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n <= 0 {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpeq
            0x9f => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let b = frame.pop()?;
                let a = frame.pop()?;
                if a == b {
                    frame.pc = helpers::branch_target(frame.pc, offset);
                }
            }

            // if_icmpne
            0xa0 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let b = frame.pop()?;
                let a = frame.pop()?;
                if a != b {
                    frame.pc = helpers::branch_target(frame.pc, offset);
                }
            }

            // if_icmplt: branch if a < b (a=below TOS, b=TOS)
            0xa1 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a < b {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpge
            0xa2 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a >= b {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpgt
            0xa3 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a > b {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmple
            0xa4 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a <= b {
                        frame.pc = helpers::branch_target(frame.pc, offset);
                    }
                }
            }

            // goto — signed 16-bit offset from opcode start (opcode is at pc-1)
            0xa7 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                // goto instruction starts at frame.pc - 1; offset is from that start
                frame.pc = ((frame.pc as i32) - 1 + offset as i32) as usize;
            }

            // checkcast — peek TOS; error if the object is not an instance of the target class
            0xc0 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Some(Value::ObjectRef(idx)) = frame.stack.last().copied() {
                    let cf = &self.classes[frame.class_idx];
                    if let Some(target_bytes) = cf.cp_class_name(cp_idx) {
                        if let Ok(target) = core::str::from_utf8(target_bytes) {
                            let runtime_class = self.objects.class_name(idx).unwrap_or("");
                            if !helpers::is_instance_of(self.classes, runtime_class, target) {
                                return Err(JvmError::InvalidReference);
                            }
                        }
                    }
                }
            }

            // instanceof — pop objectref; push 1 if instance of target class, else 0
            0xc1 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let obj = frame.pop()?;
                let result = match obj {
                    Value::Null => Value::Int(0),
                    Value::ObjectRef(idx) => {
                        let cf = &self.classes[frame.class_idx];
                        let is_instance = cf
                            .cp_class_name(cp_idx)
                            .and_then(|b| core::str::from_utf8(b).ok())
                            .map(|target| {
                                let runtime_class = self.objects.class_name(idx).unwrap_or("");
                                helpers::is_instance_of(self.classes, runtime_class, target)
                            })
                            .unwrap_or(false);
                        Value::Int(if is_instance { 1 } else { 0 })
                    }
                    _ => Value::Int(0),
                };
                frame.push(result)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
