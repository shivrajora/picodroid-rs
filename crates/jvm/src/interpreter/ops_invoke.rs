// SPDX-License-Identifier: GPL-3.0-only
use super::{helpers, Executor, MAX_FRAME_DEPTH, MAX_UPCALL_DEPTH};
use crate::class_file::find_class;
use crate::names::{c, d, m};
use crate::{
    frame::Frame,
    native::{BuiltinHandler, NativeContext, NativeMethodHandler},
    object_heap::LambdaProxy,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// Takes the whole frame stack rather than the current frame: native
    /// dispatch below can re-enter the interpreter (a synchronous native→Java
    /// upcall), which needs to push and pop frames. The current frame is
    /// re-derived at each use and never held across a call that might push.
    pub(super) fn op_invoke(
        &mut self,
        opcode: u8,
        code: &[u8],
        frames: &mut Vec<Frame>,
    ) -> Result<(), JvmError> {
        // invokedynamic (0xBA) has a completely different format — handle separately.
        if opcode == 0xba {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
            return self.op_invokedynamic(code, frame);
        }

        let (cp_idx, class_idx) = {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
            let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
            frame.pc += 2;
            // invokeinterface has 2 extra bytes: count (arg count hint) and a reserved 0 byte
            if opcode == 0xb9 {
                frame.pc += 2;
            }
            (cp_idx, frame.class_idx)
        };

        let cf = &self.classes[class_idx];
        let (class_bytes, name_bytes, desc_bytes) =
            cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
        let class_str = core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let name_str = core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let desc_str = core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;

        // invokestatic triggers class initialization.
        if opcode == 0xb8 && self.ensure_class_initialized(class_bytes)? {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
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
            let frame = frames.last().ok_or(JvmError::InvalidBytecode)?;
            let stack_len = frame.stack.len();
            if stack_len >= arg_count {
                match frame.stack[stack_len - arg_count] {
                    Value::ObjectRef(idx) => self.objects.class_name(idx).unwrap_or(class_str),
                    Value::Reference(_) => c::java_lang_String,
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
        if is_virtual && self.objects.has_lambdas() {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
            if self.try_lambda_dispatch(frame, arg_count, desc_str)? {
                return Ok(());
            }
        }

        // `StringBuilder.append(Object)` / `String.valueOf(Object)` take an
        // arbitrary object; run its `toString()` before the native arm sees it.
        if desc_str.starts_with(crate::names::d::p_Object__)
            && self.stringify_object_arg(class_str, name_str, desc_str, frames)?
        {
            return Ok(());
        }
        // Same for the objects inside `String.format`'s varargs array
        // (bugbash S4) — done here, on this executor, so no second
        // interpreter is monomorphised for the builtin handler.
        if class_str == c::java_lang_String && name_str == m::format {
            self.stringify_format_args(desc_str, frames)?;
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

        // Pop arguments from caller's stack into an inline buffer (avoids heap
        // alloc). The buffer is a local, so it outlives the borrow of `frames`
        // and stays valid across the dispatch below — which may re-enter the
        // interpreter and push frames.
        const MAX_INLINE_ARGS: usize = 8;
        let mut inline_buf = [Value::Null; MAX_INLINE_ARGS];
        let heap_args: Option<Vec<Value>> = {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
            let stack_len = frame.stack.len();
            if stack_len < arg_count {
                return Err(JvmError::StackUnderflow);
            }
            let start = stack_len - arg_count;
            if arg_count <= MAX_INLINE_ARGS {
                inline_buf[..arg_count].copy_from_slice(&frame.stack[start..]);
                frame.stack.truncate(start);
                None
            } else {
                let heap_buf: Vec<Value> = frame.stack[start..].to_vec();
                frame.stack.truncate(start);
                Some(heap_buf)
            }
        };

        let native_class = if is_virtual {
            dispatch_class
        } else {
            class_str
        };

        match heap_args {
            Some(heap_buf) => self.invoke_with_heap_args(
                heap_buf,
                resolved,
                native_class,
                name_str,
                desc_str,
                frames,
            ),
            None => self.finalize_invoke(
                &inline_buf[..arg_count],
                resolved,
                native_class,
                name_str,
                desc_str,
                frames,
            ),
        }
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
        frames: &mut Vec<Frame>,
    ) -> Result<bool, JvmError> {
        let target = match (class_str, name_str) {
            (c::java_lang_StringBuilder, m::append) => d::Object__StringBuilder,
            (c::java_lang_String, m::valueOf) => d::Object__String,
            _ => return Ok(false),
        };
        if desc_str != target {
            return Ok(false);
        }
        let Some(&arg) = frames.last().ok_or(JvmError::InvalidBytecode)?.stack.last() else {
            return Ok(false);
        };
        let class = match arg {
            Value::ObjectRef(idx) => self
                .objects
                .class_name(idx)
                .ok_or(JvmError::InvalidReference)?,
            Value::ArrayRef(_) => c::java_lang_Object,
            _ => return Ok(false),
        };
        const TO_STRING: &str = m::toString;
        const TO_STRING_DESC: &str = d::__String;
        if let Some((ci, mi)) = helpers::find_method_walking_cached(
            &mut self.method_cache,
            self.classes,
            class,
            TO_STRING,
            TO_STRING_DESC,
        ) {
            let m = &self.classes[ci].methods()[mi];
            if m.code_offset != 0 {
                // Build the frame before mutating the caller's stack, so an
                // allocation failure here leaves the frame untouched.
                let new_frame = Frame::new(ci, mi, &[arg], m.max_locals, m.max_stack)?;
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                frame.stack.pop();
                frame.pc = frame.inst_pc;
                self.pending_frame = Some(new_frame);
                return Ok(true);
            }
        }
        let s = self.dispatch_native(class, TO_STRING, TO_STRING_DESC, &[arg], frames)?;
        let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
        if let (Some(slot), Some(s)) = (frame.stack.last_mut(), s) {
            *slot = s;
        }
        Ok(false)
    }

    /// Replace each plain object inside `String.format`'s `Object[]` with
    /// its `toString()` before the native arm runs (bugbash S4): the
    /// builtins format primitives, strings and boxes themselves, but a
    /// user object's override can only run here, where the real handler
    /// drives [`Self::invoke_java`]. Elements are replaced in the varargs
    /// array itself — javac builds a fresh temporary per call site, so the
    /// mutation is unobservable for compiled Java; a hand-reused Object[]
    /// would see its object elements become Strings (documented
    /// divergence, avoids rooting a copy across the upcalls).
    fn stringify_format_args(
        &mut self,
        desc_str: &str,
        frames: &mut Vec<Frame>,
    ) -> Result<(), JvmError> {
        if desc_str != crate::names::d::String_aObject__String {
            return Ok(());
        }
        let Some(&Value::ArrayRef(arr)) =
            frames.last().ok_or(JvmError::InvalidBytecode)?.stack.last()
        else {
            return Ok(());
        };
        let len = self.arrays.length(arr).unwrap_or(0) as usize;
        for i in 0..len {
            let Some(raw) = self.arrays.load(arr, i) else {
                continue;
            };
            let Value::ObjectRef(obj) = crate::array_heap::decode_ref(raw) else {
                continue;
            };
            // Boxed numerics must reach the native intact: %d/%x/%f consume
            // the box, not a string.
            if matches!(
                self.objects.class_name(obj),
                Some(
                    c::java_lang_Integer
                        | c::java_lang_Long
                        | c::java_lang_Float
                        | c::java_lang_Double
                        | c::java_lang_Boolean
                        | c::java_lang_Character
                        | c::java_lang_Short
                        | c::java_lang_Byte
                )
            ) {
                continue;
            }
            // The array is rooted through the operand stack; the returned
            // Reference is stored straight back into it.
            if let Some(s @ Value::Reference(_)) =
                self.invoke_java(frames, Value::ObjectRef(obj), m::toString, d::__String, &[])?
            {
                if let Some(enc) = crate::array_heap::encode_ref(s) {
                    let _ = self.arrays.store(arr, i, enc);
                }
            }
        }
        Ok(())
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
        if self.objects.get_lambda(obj_idx).is_none() {
            return Ok(false);
        }

        // Pop all args (including "this") and grab the interface-method args
        // after the lambda receiver itself.
        let start = stack_len - arg_count;
        let method_args: Vec<Value> = frame.stack[start + 1..].to_vec();
        frame.stack.truncate(start);

        let Some(new_frame) =
            lambda_frame(self.objects, self.classes, obj_idx, &method_args, sam_desc)?
        else {
            return Ok(false);
        };
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
        frames: &mut Vec<Frame>,
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
            if native_class != c::java_lang_String || name_str != "<init>" {
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
                let result =
                    self.dispatch_native(native_class, name_str, desc_str, args, frames)?;
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
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
                let result =
                    self.dispatch_native(native_class, name_str, desc_str, args, frames)?;
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
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
        frames: &mut Vec<Frame>,
    ) -> Result<(), JvmError> {
        self.finalize_invoke(&args, resolved, native_class, name_str, desc_str, frames)
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
        if bsm_owner != c::java_lang_invoke_LambdaMetafactory.as_bytes() {
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
                "LambdaMetafactory(newInvokeSpecial)",
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
        // Carried so the pre-dispatch seam below can re-enter the interpreter.
        frames: &mut Vec<Frame>,
    ) -> Result<Option<Value>, JvmError> {
        // `Object.getClass()` resolves here rather than in a handler: it needs
        // the class-object cache (not part of NativeContext) so that
        // `obj.getClass() == MyClass.class` identity holds against `ldc`.
        if method_name == m::getClass && descriptor == crate::names::d::__Class {
            let name: Option<&'static str> = match args.first().copied() {
                Some(Value::ObjectRef(idx)) => self.objects.class_name(idx),
                Some(Value::Reference(_)) => Some(c::java_lang_String),
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
        // `ArrayList.sort(Comparator)` resolves here rather than in a handler
        // arm, for two reasons. `java/util/ArrayList` is classfile-less, so
        // unlike `Collections.sort` there is no Java body this could live in;
        // and a handler arm receives only a `NativeContext`, which carries no
        // way back into the interpreter. Here the whole `Executor` — the real
        // handler included — is still in hand, and `ctx` has not been built
        // yet, so nothing is borrowed across the upcall.
        if method_name == m::sort
            && class_name == c::java_util_ArrayList
            && descriptor == crate::names::d::Comparator__V
        {
            self.sort_list_with_comparator(frames, args)?;
            return Ok(None);
        }
        // Everything the arm might need to re-enter the interpreter, minus
        // the handler — which it already holds as its own `&mut self` and
        // lends back through `invoke_java`. These are disjoint fields of
        // `self`, so `self.handler` stays separately borrowable below.
        let mut env = crate::native::UpcallEnv {
            statics: self.statics,
            gc_state: self.gc_state,
            class_objects: self.class_objects,
            frames,
            upcall_depth: self.upcall_depth,
        };
        let mut ctx = NativeContext {
            descriptor,
            args,
            strings: self.strings,
            objects: self.objects,
            arrays: self.arrays,
            classes: self.classes,
            upcall: Some(&mut env),
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
                    None if current != c::java_lang_Object => c::java_lang_Object,
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

    /// Synchronously invoke a Java method from inside a native context and
    /// return its value — the sole native→Java upcall primitive.
    ///
    /// `args` excludes `recv`. The receiver and arguments are GC-rooted for
    /// the duration; **any other `Value` the caller holds across this call
    /// must be shadow-rooted too, or re-read from the heap afterwards** —
    /// the callee runs arbitrary Java, which allocates, which collects.
    ///
    /// Two further obligations on callers, both consequences of the callee
    /// being able to throw:
    /// - An arm holding side state (a slot-table entry, a half-mutated
    ///   buffer) must not `?` straight out of this call — an `Err` skips
    ///   whatever cleanup follows it.
    /// - This must never be called from inside an
    ///   [`crate::atomic_section`] guard. Those suspend the scheduler and
    ///   must not block; arbitrary Java can do both.
    pub(super) fn invoke_java(
        &mut self,
        frames: &mut Vec<Frame>,
        recv: Value,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        if self.upcall_depth >= MAX_UPCALL_DEPTH {
            let e = self.stack_overflow_error()?;
            return Err(JvmError::Exception(e));
        }
        // Root the receiver and arguments. `op_invoke` popped them off the
        // operand stack before dispatching here, so until this returns they
        // exist only in the caller's Rust locals.
        let mark = self
            .gc_state
            .push_shadow_roots(core::slice::from_ref(&recv));
        self.gc_state.push_shadow_roots(args);
        let result = self.invoke_java_inner(frames, recv, method_name, descriptor, args);
        self.gc_state.truncate_shadow_roots(mark);
        result
    }

    fn invoke_java_inner(
        &mut self,
        frames: &mut Vec<Frame>,
        recv: Value,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        // Lambda proxies first. A proxy's nominal class is the functional
        // interface, whose SAM has no bytecode, so any name-based lookup
        // resolves to an empty method and silently does nothing — the exact
        // failure that forced the deferred main-queue path to route
        // `Runnable.run` through an `Executors.dispatchRunnable` bytecode
        // bridge. Running inside the Executor, this can consult the proxy
        // directly and needs no bridge.
        let new_frame = match recv {
            Value::ObjectRef(obj_idx) => {
                match lambda_frame(self.objects, self.classes, obj_idx, args, descriptor)? {
                    Some(f) => Some(f),
                    None => self.resolve_upcall_frame(recv, method_name, descriptor, args)?,
                }
            }
            _ => self.resolve_upcall_frame(recv, method_name, descriptor, args)?,
        };

        let Some(new_frame) = new_frame else {
            // No bytecode body — fall through to native dispatch, which is
            // what an ordinary invoke of this method would have done.
            let class = self.runtime_class_of(recv)?;
            let mut all: Vec<Value> = Vec::with_capacity(args.len() + 1);
            all.push(recv);
            all.extend_from_slice(args);
            return self.dispatch_native(class, method_name, descriptor, &all, frames);
        };

        let base = frames.len();
        if base >= MAX_FRAME_DEPTH {
            let e = self.stack_overflow_error()?;
            return Err(JvmError::Exception(e));
        }
        frames.push(new_frame);
        self.upcall_depth += 1;
        let r = self.run(frames, base);
        self.upcall_depth -= 1;
        if r.is_err() {
            // A caught exception was already unwound to `base` by
            // `handle_exception`'s floor; a hard error (uncaught, interrupted,
            // allocation failure) was not. Restore the caller's frame stack
            // exactly either way.
            frames.truncate(base);
        }
        r
    }

    /// Resolve `method_name`/`descriptor` against the receiver's *runtime*
    /// class, per JVMS §5.4.3.3. `Ok(None)` when there is no bytecode body
    /// (unresolved, or a native method).
    fn resolve_upcall_frame(
        &mut self,
        recv: Value,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Frame>, JvmError> {
        let class = self.runtime_class_of(recv)?;
        let Some((ci, mi)) = helpers::find_method_walking_cached(
            &mut self.method_cache,
            self.classes,
            class,
            method_name,
            descriptor,
        ) else {
            return Ok(None);
        };
        let m = &self.classes[ci].methods()[mi];
        if m.code_offset == 0 {
            return Ok(None);
        }
        let mut all: Vec<Value> = Vec::with_capacity(args.len() + 1);
        all.push(recv);
        all.extend_from_slice(args);
        Ok(Some(Frame::new(ci, mi, &all, m.max_locals, m.max_stack)?))
    }

    /// Sort a builtin `ArrayList` under a Java `Comparator`, one
    /// [`Self::invoke_java`] upcall per comparison.
    fn sort_list_with_comparator(
        &mut self,
        frames: &mut Vec<Frame>,
        args: &[Value],
    ) -> Result<(), JvmError> {
        let recv = args.first().copied().unwrap_or(Value::Null);
        let Value::ObjectRef(obj_idx) = recv else {
            return Err(JvmError::InvalidReference);
        };
        let cmp = args.get(1).copied().unwrap_or(Value::Null);
        if matches!(cmp, Value::Null) {
            // The JDK reads a null comparator as "natural ordering". That
            // needs a Comparable.compareTo upcall of its own; until then
            // reject it rather than silently leaving the list unsorted.
            let npe = self
                .objects
                .alloc(c::java_lang_NullPointerException)
                .ok_or(JvmError::StackOverflow)?;
            return Err(JvmError::Exception(npe));
        }
        let Some(Value::Int(buf)) = self.objects.get_field(obj_idx, 0) else {
            return Err(JvmError::InvalidReference);
        };
        // The list and the comparator are reachable only from this function's
        // Rust locals for the whole sort — `op_invoke` popped them off the
        // operand stack before dispatching here. Without rooting them, a
        // collection triggered by the comparator would sweep the list and
        // `list_free` the backing buffer out from under the loop.
        let mark = self.gc_state.push_shadow_roots(&[recv, cmp]);
        let r = self.insertion_sort(frames, buf as u16, cmp);
        self.gc_state.truncate_shadow_roots(mark);
        r
    }

    /// Insertion sort, deliberately, rather than the merge sort
    /// `Arrays.sort(Object[], Comparator)` uses: no auxiliary buffer means no
    /// second heap object to root, and the list is in a valid partially-sorted
    /// state between every comparison — so an exception escaping the
    /// comparator leaves a well-formed list rather than a half-merged one.
    /// O(n²) is fine at the list sizes an embedded screen holds; revisit if a
    /// caller ever sorts more than a screenful.
    fn insertion_sort(
        &mut self,
        frames: &mut Vec<Frame>,
        buf_idx: u16,
        cmp: Value,
    ) -> Result<(), JvmError> {
        const COMPARE: &str = m::compare;
        const COMPARE_DESC: &str = d::Object_Object__I;
        let len = self.objects.list_len(buf_idx);
        for i in 1..len {
            let mut j = i;
            while j > 0 {
                let (Some(prev), Some(cur)) = (
                    self.objects.list_get(buf_idx, j - 1),
                    self.objects.list_get(buf_idx, j),
                ) else {
                    return Err(JvmError::InvalidReference);
                };
                let ord = self.invoke_java(frames, cmp, COMPARE, COMPARE_DESC, &[prev, cur])?;
                let Some(Value::Int(ord)) = ord else {
                    return Err(JvmError::InvalidReference);
                };
                if ord <= 0 {
                    break;
                }
                // Re-read across the upcall rather than reusing `prev`/`cur`:
                // the comparator ran arbitrary Java, which may have collected
                // (compacting the store) or mutated the list itself. A shrunk
                // list surfaces as `None` here rather than a bad write.
                let (Some(prev), Some(cur)) = (
                    self.objects.list_get(buf_idx, j - 1),
                    self.objects.list_get(buf_idx, j),
                ) else {
                    return Err(JvmError::InvalidReference);
                };
                self.objects.list_set(buf_idx, j - 1, cur);
                self.objects.list_set(buf_idx, j, prev);
                j -= 1;
            }
        }
        Ok(())
    }

    fn runtime_class_of(&self, recv: Value) -> Result<&'static str, JvmError> {
        match recv {
            Value::ObjectRef(idx) => self
                .objects
                .class_name(idx)
                .ok_or(JvmError::InvalidReference),
            Value::Reference(_) => Ok(c::java_lang_String),
            _ => Err(JvmError::InvalidReference),
        }
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
        if let Some(target_cf) =
            find_class(self.classes, class_name.as_bytes()).map(|i| &self.classes[i])
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

/// Build the frame a lambda proxy's SAM invocation targets, applying
/// `LambdaMetafactory`'s boxing adaptation. `args` are the interface-method
/// arguments, *excluding* the proxy receiver itself. Returns `Ok(None)` when
/// `obj_idx` is not a lambda proxy.
///
/// Stack-independent by construction — it reads no operand stack and pushes
/// no frame — so both `op_invoke`'s stack-marshalling path and the
/// native→Java upcall primitive can share it.
///
/// The adaptation: kotlinc keeps a lambda body primitive (`(I)I`) behind the
/// erased SAM (`Function1.invoke(Object)Object`) and leaves unboxing the
/// arguments and boxing the return to the metafactory — javac boxes inside
/// the body, so Java apps never hit this. Captured values are passed as-is
/// (their types already match the body's leading parameters).
fn lambda_frame(
    objects: &mut crate::object_heap::ObjectHeap,
    classes: &[crate::class_file::ClassFile],
    obj_idx: u16,
    args: &[Value],
    sam_desc: &str,
) -> Result<Option<Frame>, JvmError> {
    let Some(lambda) = objects.get_lambda(obj_idx) else {
        return Ok(None);
    };
    let target_ci = lambda.target_class_idx;
    let target_mi = lambda.target_method_idx;
    let captures: Vec<Value> = lambda.captures.clone();

    let tm = &classes[target_ci].methods()[target_mi];
    if tm.code_offset == 0 {
        return Err(JvmError::NoSuchMethod);
    }
    // ACC_STATIC = 0x0008.
    let body_is_static = tm.access_flags & 0x0008 != 0;
    let impl_desc = classes[target_ci]
        .cp_utf8(tm.descriptor_index)
        .ok_or(JvmError::InvalidBytecode)?;
    let (max_locals, max_stack) = (tm.max_locals, tm.max_stack);

    let mut method_args: Vec<Value> = args.to_vec();

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
                let raw = objects
                    .get_field(idx, 0)
                    .ok_or(JvmError::InvalidReference)?;
                *arg = widen(raw, kind);
            }
            Value::Null => {
                let npe = objects
                    .alloc(c::java_lang_NullPointerException)
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
    Ok(Some(new_frame))
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
    let cf = find_class(classes, class_name.as_bytes()).map(|i| &classes[i])?;
    let super_bytes = cf.super_class_name()?;
    core::str::from_utf8(super_bytes).ok()
}
