use super::{helpers, Executor};
use crate::{
    frame::Frame,
    native::{BuiltinHandler, NativeContext, NativeMethodHandler},
    types::{JvmError, Value},
};
use alloc::vec::Vec;

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_invoke(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;
        // invokeinterface has 2 extra bytes: count (arg count hint) and a reserved 0 byte
        if opcode == 0xb9 {
            frame.pc += 2;
        }

        let cf = &self.classes[frame.class_idx];
        let (class_bytes, name_bytes, desc_bytes) =
            cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
        let class_str = core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let name_str = core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let desc_str = core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;

        let arg_count = match opcode {
            // invokevirtual / invokespecial / invokeinterface: +1 for `this`
            0xb6 | 0xb7 | 0xb9 => 1 + helpers::count_args(desc_str),
            // invokestatic: no `this`
            0xb8 => helpers::count_args(desc_str),
            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        };

        // Determine dispatch class (virtual uses runtime class of `this`)
        let is_virtual = opcode == 0xb6 || opcode == 0xb9;
        let dispatch_class = if is_virtual {
            let stack_len = frame.stack.len();
            if stack_len >= arg_count {
                match frame.stack[stack_len - arg_count] {
                    Value::ObjectRef(idx) => self.objects.class_name(idx).unwrap_or(class_str),
                    _ => class_str,
                }
            } else {
                class_str
            }
        } else {
            class_str
        };

        // Resolve method
        let resolved = if is_virtual {
            helpers::find_method_virtual_cached(
                &mut self.method_cache,
                self.classes,
                dispatch_class,
                name_str,
                desc_str,
            )
        } else {
            helpers::find_method_cached(
                &mut self.method_cache,
                self.classes,
                class_str,
                name_str,
                desc_str,
            )
        };

        // Pop arguments from caller's stack
        let stack_len = frame.stack.len();
        if stack_len < arg_count {
            return Err(JvmError::StackUnderflow);
        }
        let start = stack_len - arg_count;
        let args_buf: Vec<Value> = frame.stack[start..].to_vec();
        for _ in 0..arg_count {
            frame.stack.pop();
        }

        let native_class = if is_virtual {
            dispatch_class
        } else {
            class_str
        };

        if let Some((ci, mi)) = resolved {
            let is_native = self.classes[ci].methods[mi].code_offset == 0;
            if is_native {
                let result = self.dispatch_native(native_class, name_str, desc_str, &args_buf)?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
                Ok(())
            } else {
                // Java method — push new frame for the iterative interpreter loop
                let new_frame = Frame::new(ci, mi, &args_buf)?;
                self.pending_frame = Some(new_frame);
                Ok(())
            }
        } else {
            // Not found in loaded classes — try native dispatch
            let result = self.dispatch_native(native_class, name_str, desc_str, &args_buf)?;
            if let Some(v) = result {
                frame.push(v)?;
            }
            Ok(())
        }
    }

    /// Dispatch a native method call through the handler chain.
    fn dispatch_native(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let mut ctx = NativeContext {
            descriptor,
            args,
            strings: self.strings,
            objects: self.objects,
            arrays: self.arrays,
        };
        self.handler
            .dispatch(class_name, method_name, &mut ctx)
            .or_else(|| BuiltinHandler.dispatch(class_name, method_name, &mut ctx))
            .unwrap_or(Err(JvmError::NoSuchMethod))
    }

    pub(super) fn op_new(&mut self, code: &[u8], frame: &mut Frame) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;
        let cf = &self.classes[frame.class_idx];
        let class_name = cf
            .cp_class_name(cp_idx)
            .and_then(|b| core::str::from_utf8(b).ok())
            .ok_or(JvmError::InvalidBytecode)?;
        // Refuse to instantiate abstract classes or interfaces
        if let Some(target_cf) = self
            .classes
            .iter()
            .find(|c| c.class_name().is_some_and(|n| n == class_name.as_bytes()))
        {
            if target_cf.is_interface() || target_cf.is_abstract() {
                return Err(JvmError::AbstractMethodError);
            }
        }
        let static_name = helpers::class_name_to_static_in(self.classes, class_name);
        let obj_idx = self
            .objects
            .alloc(static_name)
            .ok_or(JvmError::StackOverflow)?;
        self.alloc_count = self.alloc_count.saturating_add(1);
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
    }
}
