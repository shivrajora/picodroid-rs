use super::{helpers, Executor};
use crate::framework::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_invoke(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;

        let cf = &self.classes[frame.class_idx];
        let (class_bytes, name_bytes, desc_bytes) =
            cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
        let class_str = core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let name_str = core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let desc_str = core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;

        let arg_count = match opcode {
            // invokevirtual / invokespecial: +1 for `this`
            0xb6 | 0xb7 => 1 + helpers::count_args(desc_str),
            // invokestatic: no `this`
            0xb8 => helpers::count_args(desc_str),
            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        };

        let result = helpers::invoke_method(
            self.classes,
            self.strings,
            self.objects,
            self.arrays,
            self.handler,
            class_str,
            name_str,
            desc_str,
            &mut frame.stack,
            arg_count,
        )?;
        if let Some(v) = result {
            frame.push(v)?;
        }
        Ok(())
    }

    pub(super) fn op_new(&mut self, code: &[u8], frame: &mut Frame) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;
        let cf = &self.classes[frame.class_idx];
        let class_name = cf
            .cp_class_name(cp_idx)
            .and_then(|b| core::str::from_utf8(b).ok())
            .ok_or(JvmError::InvalidBytecode)?;
        let static_name = helpers::class_name_to_static(class_name);
        let obj_idx = self
            .objects
            .alloc(static_name)
            .ok_or(JvmError::StackOverflow)?;
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
    }
}
