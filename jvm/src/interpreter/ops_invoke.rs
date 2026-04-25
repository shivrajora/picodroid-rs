use super::{helpers, Executor};
use crate::{
    frame::Frame,
    native::{BuiltinHandler, NativeContext, NativeMethodHandler},
    object_heap::LambdaProxy,
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
        // invokedynamic (0xBA) has a completely different format — handle separately.
        if opcode == 0xba {
            return self.op_invokedynamic(code, frame);
        }

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

        // invokestatic triggers class initialization.
        if opcode == 0xb8 && self.ensure_class_initialized(class_bytes)? {
            frame.pc = frame.inst_pc;
            return Ok(());
        }

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

        // Lambda proxy intercept: if receiver is a lambda, dispatch to the target method directly.
        if is_virtual {
            let stack_len = frame.stack.len();
            if stack_len >= arg_count {
                if let Value::ObjectRef(obj_idx) = frame.stack[stack_len - arg_count] {
                    if let Some(lambda) = self.objects.get_lambda(obj_idx) {
                        let target_ci = lambda.target_class_idx;
                        let target_mi = lambda.target_method_idx;
                        let captures: Vec<Value> = lambda.captures.clone();

                        // Pop all args (including "this")
                        let start = stack_len - arg_count;
                        let method_args: Vec<Value> = frame.stack[start + 1..].to_vec();
                        frame.stack.truncate(start);

                        // Build actual args: captures first, then interface method args
                        let mut actual_args = captures;
                        actual_args.extend_from_slice(&method_args);

                        let is_native =
                            self.classes[target_ci].methods()[target_mi].code_offset == 0;
                        if is_native {
                            return Err(JvmError::NoSuchMethod);
                        }
                        let tm = &self.classes[target_ci].methods()[target_mi];
                        let new_frame = Frame::new(
                            target_ci,
                            target_mi,
                            &actual_args,
                            tm.max_locals,
                            tm.max_stack,
                        )?;
                        self.pending_frame = Some(new_frame);
                        return Ok(());
                    }
                }
            }
        }

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

        // Pop arguments from caller's stack into an inline buffer (avoids heap alloc).
        let stack_len = frame.stack.len();
        if stack_len < arg_count {
            return Err(JvmError::StackUnderflow);
        }
        let start = stack_len - arg_count;

        const MAX_INLINE_ARGS: usize = 8;
        let mut inline_buf = [Value::Null; MAX_INLINE_ARGS];
        let args: &[Value] = if arg_count <= MAX_INLINE_ARGS {
            inline_buf[..arg_count].copy_from_slice(&frame.stack[start..]);
            frame.stack.truncate(start);
            &inline_buf[..arg_count]
        } else {
            let heap_buf: Vec<Value> = frame.stack[start..].to_vec();
            frame.stack.truncate(start);
            // SAFETY: heap_buf lives until end of this block; we return before drop.
            // Use a Vec and pass slices from it below.
            let native_class = if is_virtual {
                dispatch_class
            } else {
                class_str
            };
            return self.invoke_with_heap_args(
                heap_buf,
                resolved,
                native_class,
                name_str,
                desc_str,
                frame,
            );
        };

        let native_class = if is_virtual {
            dispatch_class
        } else {
            class_str
        };

        if let Some((ci, mi)) = resolved {
            let is_native = self.classes[ci].methods()[mi].code_offset == 0;
            if is_native {
                let result = self.dispatch_native(native_class, name_str, desc_str, args)?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
                Ok(())
            } else {
                // Java method — push new frame for the iterative interpreter loop
                let jm = &self.classes[ci].methods()[mi];
                let new_frame = Frame::new(ci, mi, args, jm.max_locals, jm.max_stack)?;
                self.pending_frame = Some(new_frame);
                Ok(())
            }
        } else {
            // Not found in loaded classes — try native dispatch
            let result = self.dispatch_native(native_class, name_str, desc_str, args)?;
            if let Some(v) = result {
                frame.push(v)?;
            }
            Ok(())
        }
    }

    /// Fallback path for methods with >8 arguments (extremely rare).
    #[cold]
    #[allow(clippy::too_many_arguments)]
    fn invoke_with_heap_args(
        &mut self,
        args: Vec<Value>,
        resolved: Option<(usize, usize)>,
        native_class: &str,
        name_str: &str,
        desc_str: &str,
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        if let Some((ci, mi)) = resolved {
            let is_native = self.classes[ci].methods()[mi].code_offset == 0;
            if is_native {
                let result = self.dispatch_native(native_class, name_str, desc_str, &args)?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
                Ok(())
            } else {
                let jm = &self.classes[ci].methods()[mi];
                let new_frame = Frame::new(ci, mi, &args, jm.max_locals, jm.max_stack)?;
                self.pending_frame = Some(new_frame);
                Ok(())
            }
        } else {
            let result = self.dispatch_native(native_class, name_str, desc_str, &args)?;
            if let Some(v) = result {
                frame.push(v)?;
            }
            Ok(())
        }
    }

    /// Handle `invokedynamic` (0xBA) for lambda expressions.
    fn op_invokedynamic(&mut self, code: &[u8], frame: &mut Frame) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 4; // skip index (2) + padding (2)

        let cf = &self.classes[frame.class_idx];

        // 1. Resolve CONSTANT_InvokeDynamic -> (bootstrap_idx, name_and_type_idx)
        let (bsm_idx, nat_idx) = cf
            .cp_invoke_dynamic(cp_idx)
            .ok_or(JvmError::InvalidBytecode)?;

        // 2. Get the NameAndType to know the factory descriptor (return type = functional interface)
        let (_name_bytes, desc_bytes) = cf
            .cp_name_and_type(nat_idx)
            .ok_or(JvmError::InvalidBytecode)?;
        let factory_desc =
            core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;

        // 3. Get the BootstrapMethod entry
        let bsm = cf
            .bootstrap_methods()
            .get(bsm_idx as usize)
            .ok_or(JvmError::InvalidBytecode)?;

        // 4. Bootstrap arguments for LambdaMetafactory:
        //    [0] = MethodType (samMethodType)
        //    [1] = MethodHandle (implMethod) — the target lambda$ method
        //    [2] = MethodType (instantiatedMethodType)
        let impl_method_cp = *bsm.arguments.get(1).ok_or(JvmError::InvalidBytecode)?;
        let (_ref_kind, ref_idx) = cf
            .cp_method_handle(impl_method_cp)
            .ok_or(JvmError::InvalidBytecode)?;

        // 5. Resolve the MethodHandle's Methodref to find the target method
        let (target_class_bytes, target_name_bytes, target_desc_bytes) =
            cf.cp_methodref(ref_idx).ok_or(JvmError::InvalidBytecode)?;
        let target_class =
            core::str::from_utf8(target_class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let target_name =
            core::str::from_utf8(target_name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let target_desc =
            core::str::from_utf8(target_desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;

        let (target_ci, target_mi) =
            helpers::find_method(self.classes, target_class, target_name, target_desc)
                .ok_or(JvmError::NoSuchMethod)?;

        // 6. Pop captured values from the operand stack
        let capture_count = helpers::count_args(factory_desc);
        let stack_len = frame.stack.len();
        let captures: Vec<Value> = if capture_count > 0 {
            let start = stack_len
                .checked_sub(capture_count)
                .ok_or(JvmError::StackUnderflow)?;
            let caps = frame.stack[start..].to_vec();
            frame.stack.truncate(start);
            caps
        } else {
            Vec::new()
        };

        // 7. Allocate a proxy object with the functional interface class name
        let iface_class =
            helpers::descriptor_return_class(factory_desc).ok_or(JvmError::InvalidBytecode)?;
        let static_name = helpers::class_name_to_static_in(
            self.classes,
            self.handler.native_class_names(),
            iface_class,
        );
        let obj_idx = self
            .objects
            .alloc(static_name)
            .ok_or(JvmError::StackOverflow)?;
        self.alloc_count = self.alloc_count.saturating_add(1);

        // 8. Register lambda metadata
        self.objects.register_lambda(
            obj_idx,
            LambdaProxy {
                target_class_idx: target_ci,
                target_method_idx: target_mi,
                captures,
            },
        );

        // 9. Push the proxy object reference
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
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
        // Try the exact class first.
        if let Some(result) = self
            .handler
            .dispatch(class_name, method_name, &mut ctx)
            .or_else(|| BuiltinHandler.dispatch(class_name, method_name, &mut ctx))
        {
            return result;
        }
        // Walk the superclass chain: the method may be inherited from a native
        // base class (e.g. enumdemo/Color extends java/lang/Enum).
        let mut current = class_name;
        while let Some(super_str) = find_super_class(self.classes, current) {
            if let Some(result) = self
                .handler
                .dispatch(super_str, method_name, &mut ctx)
                .or_else(|| BuiltinHandler.dispatch(super_str, method_name, &mut ctx))
            {
                return result;
            }
            current = super_str;
        }
        Err(JvmError::NoSuchMethod)
    }

    pub(super) fn op_new(&mut self, code: &[u8], frame: &mut Frame) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 2;
        let cf = &self.classes[frame.class_idx];
        let class_name_bytes = cf.cp_class_name(cp_idx).ok_or(JvmError::InvalidBytecode)?;
        if self.ensure_class_initialized(class_name_bytes)? {
            frame.pc = frame.inst_pc;
            return Ok(());
        }
        let class_name =
            core::str::from_utf8(class_name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
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
        let static_name = helpers::class_name_to_static_in(
            self.classes,
            self.handler.native_class_names(),
            class_name,
        );
        let obj_idx = self
            .objects
            .alloc_with_defaults(static_name, self.classes)
            .ok_or(JvmError::StackOverflow)?;
        self.alloc_count = self.alloc_count.saturating_add(1);
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
    }
}

/// Return the super class name of `class_name` if it's in the loaded set.
fn find_super_class<'a>(
    classes: &'a [crate::class_file::ClassFile],
    class_name: &str,
) -> Option<&'a str> {
    let cf = classes
        .iter()
        .find(|cf| cf.class_name().is_some_and(|n| n == class_name.as_bytes()))?;
    let super_bytes = cf.super_class_name()?;
    core::str::from_utf8(super_bytes).ok()
}
