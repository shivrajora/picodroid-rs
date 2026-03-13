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

            // checkcast — no-op for M2
            0xc0 => {
                frame.pc += 2;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
