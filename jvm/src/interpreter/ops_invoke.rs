// SPDX-License-Identifier: GPL-3.0-only
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

        // Determine dispatch class (virtual uses runtime class of `this`).
        // A string Reference has no ObjectHeap class — its runtime class is
        // always `java/lang/String`, whatever the CP declared: an
        // `invokeinterface Comparable.compareTo` / `CharSequence.length` or
        // `invokevirtual Object.equals` on a String must reach the String
        // dispatcher, not a `java/lang/Comparable` arm that does not exist.
        // Likewise an array dispatches as its array class: kotlinc's
        // `values()` clones `$VALUES` through `Object.clone()`, where javac
        // names the array class as the owner.
        let is_virtual = opcode == 0xb6 || opcode == 0xb9;
        let dispatch_class = if is_virtual {
            let stack_len = frame.stack.len();
            if stack_len >= arg_count {
                match frame.stack[stack_len - arg_count] {
                    Value::ObjectRef(idx) => self.objects.class_name(idx).unwrap_or(class_str),
                    Value::Reference(_) => "java/lang/String",
                    Value::ArrayRef(idx) => helpers::array_class_name(
                        self.arrays
                            .atype(idx)
                            .unwrap_or(crate::array_heap::ATYPE_REF),
                    ),
                    _ => class_str,
                }
            } else {
                class_str
            }
        } else {
            class_str
        };

        // Lambda proxy intercept: if receiver is a lambda, dispatch to the
        // target method directly.
        if is_virtual
            && self.objects.has_lambdas()
            && self.try_lambda_dispatch(frame, arg_count, desc_str)?
        {
            return Ok(());
        }

        // `StringBuilder.append(Object)` / `String.valueOf(Object)` take an
        // arbitrary object; run its `toString()` before the native arm sees it.
        if desc_str.starts_with("(Ljava/lang/Object;)")
            && self.stringify_object_arg(class_str, name_str, desc_str, frame)?
        {
            return Ok(());
        }

        // Resolve method. Both branches walk the superclass chain per JVMS §5.4.3.3:
        // invokevirtual / invokeinterface start from the receiver's runtime class,
        // invokestatic / invokespecial start from the CP-declared class.
        let resolved = if is_virtual {
            helpers::find_method_walking_cached(
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

        self.finalize_invoke(args, resolved, native_class, name_str, desc_str, frame)
    }

    /// The one `Object`-typed argument the builtins cannot format themselves:
    /// `StringBuilder.append(Object)` (every `"" + obj` and Kotlin `"$obj"`
    /// template) and `String.valueOf(Object)`. When the top-of-stack argument
    /// is an object or array: if its class (or a superclass) has a Java
    /// `toString()`, pop the argument, push a frame for that method and
    /// rewind `pc` so this same invoke re-executes with the returned String
    /// in the argument slot — the `<clinit>` pattern; otherwise replace the
    /// argument in place with the native `toString` (boxed values, enums,
    /// the identity `Cls@hhhh`) and let the invoke proceed. Strings and
    /// `null` need nothing: the native arms handle them. Returns `Ok(true)`
    /// when a frame was pushed.
    fn stringify_object_arg(
        &mut self,
        class_str: &str,
        name_str: &str,
        desc_str: &str,
        frame: &mut Frame,
    ) -> Result<bool, JvmError> {
        let target = match (class_str, name_str) {
            ("java/lang/StringBuilder", "append") => {
                "(Ljava/lang/Object;)Ljava/lang/StringBuilder;"
            }
            ("java/lang/String", "valueOf") => "(Ljava/lang/Object;)Ljava/lang/String;",
            _ => return Ok(false),
        };
        if desc_str != target {
            return Ok(false);
        }
        let Some(&arg) = frame.stack.last() else {
            return Ok(false);
        };
        let class = match arg {
            Value::ObjectRef(idx) => self
                .objects
                .class_name(idx)
                .ok_or(JvmError::InvalidReference)?,
            Value::ArrayRef(_) => "java/lang/Object",
            _ => return Ok(false),
        };
        const TO_STRING: &str = "toString";
        const TO_STRING_DESC: &str = "()Ljava/lang/String;";
        if let Some((ci, mi)) = helpers::find_method_walking_cached(
            &mut self.method_cache,
            self.classes,
            class,
            TO_STRING,
            TO_STRING_DESC,
        ) {
            let m = &self.classes[ci].methods()[mi];
            if m.code_offset != 0 {
                frame.stack.pop();
                self.pending_frame = Some(Frame::new(ci, mi, &[arg], m.max_locals, m.max_stack)?);
                frame.pc = frame.inst_pc;
                return Ok(true);
            }
        }
        let s = self.dispatch_native(class, TO_STRING, TO_STRING_DESC, &[arg])?;
        if let (Some(slot), Some(s)) = (frame.stack.last_mut(), s) {
            *slot = s;
        }
        Ok(false)
    }

    /// If the receiver at `stack[stack_len - arg_count]` is a lambda proxy,
    /// pop its captures + invocation args, push a frame targeting the
    /// proxy's `target_class_idx::target_method_idx`, and return `Ok(true)`.
    /// Returns `Ok(false)` when the receiver isn't a lambda (caller falls
    /// through to ordinary method resolution).
    ///
    /// Performs `LambdaMetafactory`'s boxing adaptation: kotlinc keeps a
    /// lambda body primitive (`(I)I`) behind the erased SAM
    /// (`Function1.invoke(Object)Object`) and leaves unboxing the arguments
    /// and boxing the return to the metafactory — javac boxes inside the
    /// body, so Java apps never hit this. Captured values are passed as-is
    /// (their types already match the body's leading parameters).
    fn try_lambda_dispatch(
        &mut self,
        frame: &mut Frame,
        arg_count: usize,
        sam_desc: &str,
    ) -> Result<bool, JvmError> {
        let stack_len = frame.stack.len();
        if stack_len < arg_count {
            return Ok(false);
        }
        let Value::ObjectRef(obj_idx) = frame.stack[stack_len - arg_count] else {
            return Ok(false);
        };
        let Some(lambda) = self.objects.get_lambda(obj_idx) else {
            return Ok(false);
        };
        let target_ci = lambda.target_class_idx;
        let target_mi = lambda.target_method_idx;
        let captures: Vec<Value> = lambda.captures.clone();

        let tm = &self.classes[target_ci].methods()[target_mi];
        if tm.code_offset == 0 {
            return Err(JvmError::NoSuchMethod);
        }
        // ACC_STATIC = 0x0008.
        let body_is_static = tm.access_flags & 0x0008 != 0;
        let impl_desc = self.classes[target_ci]
            .cp_utf8(tm.descriptor_index)
            .ok_or(JvmError::InvalidBytecode)?;
        let (max_locals, max_stack) = (tm.max_locals, tm.max_stack);

        // Pop all args (including "this") and grab the interface-method args
        // after the lambda receiver itself.
        let start = stack_len - arg_count;
        let mut method_args: Vec<Value> = frame.stack[start + 1..].to_vec();
        frame.stack.truncate(start);

        // Unbox every boxed argument whose body parameter is primitive. The
        // captures occupy the body's leading parameters; step past them by
        // hand (`Iterator::skip` monomorphises a 500 B `nth` on thumbv6m).
        // A lambda capturing `this` compiles to an *instance* synthetic body
        // (javac's `private void lambda$track$0(...)`): the captured receiver
        // lands in local 0 and is *not* a descriptor parameter, so it steps
        // past no kind. Skipping one per capture regardless shifted every
        // remaining argument onto the wrong kind and unboxed a reference --
        // picodroid.widget.RadioGroup's own `(CompoundButton, boolean)`
        // listener read field 0 off the button and passed it as `buttonView`.
        let mut body_kinds = helpers::ParamKinds::new(impl_desc);
        let desc_captures = if body_is_static {
            captures.len()
        } else {
            captures.len().saturating_sub(1)
        };
        for _ in 0..desc_captures {
            body_kinds.next();
        }
        for (arg, kind) in method_args.iter_mut().zip(body_kinds) {
            if kind == b'L' {
                continue;
            }
            match *arg {
                Value::ObjectRef(idx) => {
                    let raw = self
                        .objects
                        .get_field(idx, 0)
                        .ok_or(JvmError::InvalidReference)?;
                    *arg = widen(raw, kind);
                }
                Value::Null => {
                    let npe = self
                        .objects
                        .alloc("java/lang/NullPointerException")
                        .ok_or(JvmError::StackOverflow)?;
                    return Err(JvmError::Exception(npe));
                }
                _ => {}
            }
        }
        let body_ret = helpers::return_kind(impl_desc);
        let box_return = if body_ret != b'L'
            && body_ret != b'V'
            && helpers::return_kind(sam_desc.as_bytes()) == b'L'
        {
            body_ret
        } else {
            0
        };

        // Build actual args: captures first, then interface method args.
        let mut actual_args = captures;
        actual_args.extend_from_slice(&method_args);

        let mut new_frame = Frame::new(target_ci, target_mi, &actual_args, max_locals, max_stack)?;
        new_frame.box_return = box_return;
        self.pending_frame = Some(new_frame);
        Ok(true)
    }

    /// Shared tail used by both the inline-args fast path and the heap-args
    /// fallback: dispatches `resolved` to a native handler or pushes a new
    /// Java frame, with `resolved == None` falling back to native dispatch.
    fn finalize_invoke(
        &mut self,
        args: &[Value],
        resolved: Option<(usize, usize)>,
        native_class: &str,
        name_str: &str,
        desc_str: &str,
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        let push_native_result =
            |frame: &mut Frame, result: Option<Value>| -> Result<(), JvmError> {
                if let Some(v) = result {
                    frame.push(v)?;
                }
                Ok(())
            };
        // `new String(byte[])`: String has no class file, so `op_new` pushed a
        // placeholder ObjectHeap object and the `<init>` reached native
        // dispatch, which interned the bytes and returned the real string
        // Reference (see `string::dispatch`). A constructor can't "return" a
        // different receiver, so rewrite every occurrence of the placeholder
        // in the creating frame instead. JVMS verification confines an
        // uninitialized-`new` reference to this frame's stack and locals
        // until `<init>` completes, so the rewrite is exhaustive; the
        // placeholder object becomes garbage and is collected normally.
        let string_init_swap = |frame: &mut Frame, args: &[Value], result: Option<Value>| -> bool {
            if native_class != "java/lang/String" || name_str != "<init>" {
                return false;
            }
            let (Some(Value::ObjectRef(placeholder)), Some(Value::Reference(interned))) =
                (args.first().copied(), result)
            else {
                return false;
            };
            for slot in frame.stack.iter_mut().chain(frame.locals.iter_mut()) {
                if *slot == Value::ObjectRef(placeholder) {
                    *slot = Value::Reference(interned);
                }
            }
            true
        };
        match resolved {
            Some((ci, mi)) if self.classes[ci].methods()[mi].code_offset == 0 => {
                let result = self.dispatch_native(native_class, name_str, desc_str, args)?;
                if string_init_swap(frame, args, result) {
                    return Ok(());
                }
                push_native_result(frame, result)
            }
            Some((ci, mi)) => {
                // Java method — push new frame for the iterative interpreter loop.
                let jm = &self.classes[ci].methods()[mi];
                let new_frame = Frame::new(ci, mi, args, jm.max_locals, jm.max_stack)?;
                self.pending_frame = Some(new_frame);
                Ok(())
            }
            None => {
                // Not found in loaded classes — try native dispatch.
                let result = self.dispatch_native(native_class, name_str, desc_str, args)?;
                if string_init_swap(frame, args, result) {
                    return Ok(());
                }
                push_native_result(frame, result)
            }
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
        self.finalize_invoke(&args, resolved, native_class, name_str, desc_str, frame)
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

        // 4. The only bootstraps this JVM implements are LambdaMetafactory's
        //    (metafactory, and altMetafactory — same first three arguments —
        //    which javac/kotlinc emit only for Serializable SAMs); the owner
        //    check is what matters. Anything else — StringConcatFactory from
        //    a class compiled for Java 9+, ObjectMethods from records — would
        //    otherwise have its arguments[1] misread as a lambda
        //    implementation handle.
        //    Bootstrap arguments for LambdaMetafactory:
        //    [0] = MethodType (samMethodType)
        //    [1] = MethodHandle (implMethod) — the target lambda$ method
        //    [2] = MethodType (instantiatedMethodType)
        let (_bsm_kind, bsm_ref) = cf
            .cp_method_handle(bsm.method_ref)
            .ok_or(JvmError::InvalidBytecode)?;
        let (bsm_owner, _, _) = cf.cp_methodref(bsm_ref).ok_or(JvmError::InvalidBytecode)?;
        if bsm_owner != b"java/lang/invoke/LambdaMetafactory" {
            let owner = core::str::from_utf8(bsm_owner).unwrap_or("?");
            return Err(JvmError::UnsupportedInvokeDynamic(owner));
        }
        let impl_method_cp = *bsm.arguments.get(1).ok_or(JvmError::InvalidBytecode)?;
        let (ref_kind, ref_idx) = cf
            .cp_method_handle(impl_method_cp)
            .ok_or(JvmError::InvalidBytecode)?;
        // REF_invokeVirtual/Static/Special/Interface (5/6/7/9) all take the
        // captures as leading arguments, which is how the proxy is invoked
        // below. REF_newInvokeSpecial (8, a `Foo::new` constructor reference)
        // would call `<init>` on nothing — reject it up front.
        if !matches!(ref_kind, 5 | 6 | 7 | 9) {
            return Err(JvmError::UnsupportedInvokeDynamic(
                "java/lang/invoke/LambdaMetafactory(newInvokeSpecial)",
            ));
        }

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
        // `Object.getClass()` resolves here rather than in a handler: it needs
        // the class-object cache (not part of NativeContext) so that
        // `obj.getClass() == MyClass.class` identity holds against `ldc`.
        if method_name == "getClass" && descriptor == "()Ljava/lang/Class;" {
            let name: Option<&'static str> = match args.first().copied() {
                Some(Value::ObjectRef(idx)) => self.objects.class_name(idx),
                Some(Value::Reference(_)) => Some("java/lang/String"),
                _ => None,
            };
            if let Some(name) = name {
                return helpers::class_object_for_name(
                    self.classes,
                    self.strings,
                    self.objects,
                    self.class_objects,
                    name.as_bytes(),
                )
                .map(Some);
            }
        }
        let mut ctx = NativeContext {
            descriptor,
            args,
            strings: self.strings,
            objects: self.objects,
            arrays: self.arrays,
            classes: self.classes,
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
        // base class (e.g. enumdemo/Color extends java/lang/Enum). When the
        // chain leaves the loaded classfiles, follow the builtin throwable
        // hierarchy — getMessage()/getCause() on an alloc-by-name exception
        // (NumberFormatException, ExceptionInInitializerError, ...) resolves
        // through java/lang/RuntimeException / Throwable's dispatcher.
        let mut current = class_name;
        loop {
            let super_str = match find_super_class(self.classes, current) {
                Some(s) => s,
                None => match helpers::builtin_super(current) {
                    Some(s) => s,
                    // Every class ends in java/lang/Object — a loaded class
                    // whose parent is Object reports no super name, and a
                    // name with neither class file nor table row still
                    // inherits Object's identity equals/hashCode/toString.
                    None if current != "java/lang/Object" => "java/lang/Object",
                    None => break,
                },
            };
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
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
    }
}

/// Widen an unboxed value to the body's parameter kind (an `Integer` passed
/// where the body takes `long`, `float` or `double`); every other
/// combination is already the right `Value`.
fn widen(v: Value, kind: u8) -> Value {
    match (kind, v) {
        (b'J', Value::Int(i)) => Value::Long(i as i64),
        (b'F', Value::Int(i)) => Value::Float(i as f32),
        (b'D', Value::Int(i)) => Value::Double(i as f64),
        (b'D', Value::Float(f)) => Value::Double(f as f64),
        _ => v,
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
