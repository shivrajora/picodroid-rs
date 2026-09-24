// SPDX-License-Identifier: GPL-3.0-only
use super::ops_indy::LambdaCall;
use super::{helpers, Executor, MAX_FRAME_DEPTH, MAX_UPCALL_DEPTH};
use crate::class_file::find_class;
use crate::names::{c, d, m};
use crate::{
    frame::Frame,
    native::{BuiltinHandler, NativeContext, NativeMethodHandler},
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

        // invokestatic triggers class initialization. A site whose entry
        // already says "initialised" skips the probe (a scan of the
        // initialised-class list by name); the flag is set below, after the
        // site resolves, the first time the probe comes back clear.
        let mut mark_static_init = false;
        if opcode == 0xb8
            && !self
                .class_objects
                .resolve
                .method(class_str, name_str, desc_str)
                .is_some_and(|hit| hit.init)
        {
            #[cfg(feature = "parity-metrics")]
            let t0 = self.handler.clock_nanos();
            let pending = self.ensure_class_initialized(class_bytes)?;
            #[cfg(feature = "parity-metrics")]
            crate::parity::count_clinit_time(self.handler.clock_nanos().saturating_sub(t0));
            if pending {
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                frame.pc = frame.inst_pc;
                return Ok(());
            }
            mark_static_init = true;
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

        // Lambda proxy intercept: a call to the proxy's SAM runs the lambda
        // body; any other method on it (a default method, `Object`'s) falls
        // through to ordinary resolution below.
        if is_virtual
            && self.objects.has_lambdas()
            && self.try_lambda_dispatch(frames, arg_count, name_str, desc_str)?
        {
            return Ok(());
        }

        // `StringBuilder.append(Object)` / `String.valueOf(Object)` take an
        // arbitrary object; run its `toString()` before the native arm sees it.
        if (desc_str.starts_with(crate::names::d::p_Object__)
            || desc_str == d::CharSequence__StringBuilder)
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
        #[cfg(feature = "parity-metrics")]
        let resolve_start = self.handler.clock_nanos();
        let resolved = if is_virtual {
            helpers::find_method_walking_cached(
                &mut self.class_objects.resolve,
                self.classes,
                dispatch_class,
                name_str,
                desc_str,
            )
        } else {
            let r = helpers::find_method_cached(
                &mut self.class_objects.resolve,
                self.classes,
                class_str,
                name_str,
                desc_str,
            );
            if mark_static_init {
                self.class_objects
                    .resolve
                    .mark_method_init(class_str, name_str, desc_str);
            }
            r
        };
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_resolve_time(self.handler.clock_nanos().saturating_sub(resolve_start));

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

        // JVMS §6.5: invokevirtual / invokespecial / invokeinterface on a
        // null objectref throw NullPointerException. Without this the null
        // reached the native arm of a builtin (`Integer.intValue` — every
        // unboxing of a null `Integer`) as an uncatchable InvalidReference.
        if opcode != 0xb8 {
            let recv = match &heap_args {
                Some(buf) => buf.first().copied(),
                None => inline_buf.first().copied(),
            };
            if matches!(recv, Some(Value::Null)) {
                return Err(self.runtime_fault(c::java_lang_NullPointerException));
            }
        }

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
    pub(super) fn stringify_object_arg(
        &mut self,
        class_str: &str,
        name_str: &str,
        desc_str: &str,
        frames: &mut Vec<Frame>,
    ) -> Result<bool, JvmError> {
        // `append(CharSequence)` is what javac picks for a StringBuilder
        // argument (`sb.append(other)`, `sb.append(sb)`): the same
        // stringification, through the builder's own `toString`.
        let object_arg = match (class_str, name_str) {
            (c::java_lang_StringBuilder, m::append) => {
                desc_str == d::Object__StringBuilder || desc_str == d::CharSequence__StringBuilder
            }
            (c::java_lang_String, m::valueOf) => desc_str == d::Object__String,
            _ => false,
        };
        if !object_arg {
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
            &mut self.class_objects.resolve,
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
    pub(super) fn stringify_format_args(
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

    /// Shared tail used by both the inline-args fast path and the heap-args
    /// fallback: dispatches `resolved` to a native handler or pushes a new
    /// Java frame, with `resolved == None` falling back to native dispatch.
    pub(super) fn finalize_invoke(
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
                    match self.dispatch_native(native_class, name_str, desc_str, args, frames) {
                        Err(JvmError::StackOverflow) if self.native_retry => {
                            return self.retry_after_gc(args, frames);
                        }
                        r => r?,
                    };
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                if string_init_swap(frame, args, result) {
                    return Ok(());
                }
                push_native_result(frame, result)
            }
            Some((ci, mi)) => {
                // Java method — push new frame for the iterative interpreter loop.
                let jm = &self.classes[ci].methods()[mi];
                #[cfg(feature = "parity-metrics")]
                let t0 = self.handler.clock_nanos();
                let new_frame = Frame::new_in(
                    &mut self.frame_pool,
                    ci,
                    mi,
                    args,
                    jm.max_locals,
                    jm.max_stack,
                )?;
                #[cfg(feature = "parity-metrics")]
                crate::parity::count_frame_time(self.handler.clock_nanos().saturating_sub(t0));
                self.pending_frame = Some(new_frame);
                Ok(())
            }
            None => {
                // Not found in loaded classes — try native dispatch.
                let result =
                    match self.dispatch_native(native_class, name_str, desc_str, args, frames) {
                        Err(JvmError::StackOverflow) if self.native_retry => {
                            return self.retry_after_gc(args, frames);
                        }
                        r => r?,
                    };
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                if string_init_swap(frame, args, result) {
                    return Ok(());
                }
                push_native_result(frame, result)
            }
        }
    }

    /// A builtin arm ran out of heap. By the builtins' contract it changed
    /// nothing first (a side buffer reserves before it writes, a box is
    /// allocated before it is filled), so put the arguments back, let the
    /// main loop collect, and re-execute this invoke — the `new` protocol.
    /// A collection that frees nothing makes it a catchable
    /// `OutOfMemoryError` there; until QA 2026-09-13 the first allocation
    /// to fail inside a native arm ended the app, garbage or no garbage.
    pub(super) fn retry_after_gc(
        &mut self,
        args: &[Value],
        frames: &mut [Frame],
    ) -> Result<(), JvmError> {
        let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
        for &a in args {
            frame.push(a)?;
        }
        frame.pc = frame.inst_pc;
        self.set_need_gc(true);
        Ok(())
    }

    /// Fallback path for methods with >8 arguments (extremely rare).
    #[cold]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn invoke_with_heap_args(
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

    /// Dispatch a native method call through the handler chain.
    pub(super) fn dispatch_native(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
        // Carried so the pre-dispatch seam below can re-enter the interpreter.
        frames: &mut Vec<Frame>,
    ) -> Result<Option<Value>, JvmError> {
        let mut retry = false;
        let r = self.dispatch_native_inner(
            class_name,
            method_name,
            descriptor,
            args,
            frames,
            &mut retry,
        );
        self.native_retry = retry;
        r
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_native_inner(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
        frames: &mut Vec<Frame>,
        retry: &mut bool,
    ) -> Result<Option<Value>, JvmError> {
        // `Object.getClass()` resolves here rather than in a handler: it needs
        // the class-object cache (not part of NativeContext) so that
        // `obj.getClass() == MyClass.class` identity holds against `ldc`.
        if method_name == m::getClass && descriptor == crate::names::d::__Class {
            let name: Option<&'static str> = match args.first().copied() {
                Some(Value::ObjectRef(idx)) => self.objects.class_name(idx),
                Some(Value::Reference(_)) => Some(c::java_lang_String),
                // `arr.getClass()` — the array class, keyed by element kind
                // (`[I`, `[Ljava/lang/Object;`), so two int[] share a Class.
                // Used to fall through to a handler arm that does not exist.
                Some(Value::ArrayRef(idx)) => Some(helpers::array_class_name(
                    self.arrays
                        .atype(idx)
                        .unwrap_or(crate::array_heap::ATYPE_REF),
                )),
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
        // The builtin collections compare keys by value for strings and
        // boxes and by identity for everything else; a class that overrides
        // `equals(Object)` — every hand-written key class — gets its override
        // called here, where a Java method can be run (a handler arm cannot),
        // one upcall per stored candidate. The buffers are linear anyway.
        if let Some(r) =
            self.equals_aware_collection_op(class_name, method_name, descriptor, args, frames)?
        {
            return Ok(r);
        }
        // `Enum.valueOf(Class, String)` — the callee of every enum's own
        // `valueOf(String)` — resolves here: the constants are the static
        // fields of the enum class's own type in the static store, which the
        // `invokestatic` into that class initialised on the way in.
        if class_name == c::java_lang_Enum
            && method_name == m::valueOf
            && descriptor == d::Class_String__Enum
        {
            return self.enum_value_of(args);
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
        #[cfg(feature = "parity-metrics")]
        let native_start = self.handler.clock_nanos();
        let exact = self
            .handler
            .dispatch(class_name, method_name, &mut ctx)
            .or_else(|| {
                let r = BuiltinHandler.dispatch(class_name, method_name, &mut ctx);
                *retry = matches!(r, Some(Err(JvmError::StackOverflow)));
                r
            });
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_native(
            self.handler.clock_nanos().saturating_sub(native_start),
            class_name,
            method_name,
        );
        if let Some(result) = exact {
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
                .or_else(|| {
                    let r = BuiltinHandler.dispatch(super_str, method_name, &mut ctx);
                    *retry = matches!(r, Some(Err(JvmError::StackOverflow)));
                    r
                })
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

    pub(super) fn invoke_java_inner(
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
        let is_sam_call = match recv {
            Value::ObjectRef(obj_idx) => self
                .objects
                .get_lambda(obj_idx)
                .is_some_and(|l| l.sam_name == method_name.as_bytes()),
            _ => false,
        };
        let mut ctor_obj: Option<u16> = None;
        let new_frame = match recv {
            Value::ObjectRef(obj_idx) if is_sam_call => {
                match self.lambda_call(frames, obj_idx, args, descriptor)? {
                    LambdaCall::Frame(f) => Some(f),
                    LambdaCall::Ctor { frame, obj } => {
                        ctor_obj = Some(obj);
                        Some(frame)
                    }
                    LambdaCall::Done(result) => return Ok(result),
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
        if frames.try_reserve(1).is_err() {
            return Err(JvmError::StackOverflow);
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
        match (r, ctor_obj) {
            // `Foo::new`: the constructor returned void; the object is the result.
            (Ok(_), Some(obj)) => Ok(Some(Value::ObjectRef(obj))),
            (r, _) => r,
        }
    }

    /// Resolve `method_name`/`descriptor` against the receiver's *runtime*
    /// class, per JVMS §5.4.3.3. `Ok(None)` when there is no bytecode body
    /// (unresolved, or a native method).
    pub(super) fn resolve_upcall_frame(
        &mut self,
        recv: Value,
        method_name: &str,
        descriptor: &str,
        args: &[Value],
    ) -> Result<Option<Frame>, JvmError> {
        let class = self.runtime_class_of(recv)?;
        let Some((ci, mi)) = helpers::find_method_walking_cached(
            &mut self.class_objects.resolve,
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

    pub(super) fn runtime_class_of(&self, recv: Value) -> Result<&'static str, JvmError> {
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
        // A site the table knows — and knows initialised — needs neither
        // the initialised probe nor the two class-table walks below.
        let static_name = match self.class_objects.resolve.class(class_name_bytes) {
            Some(hit) if hit.init => hit.name,
            _ => {
                #[cfg(feature = "parity-metrics")]
                let t0 = self.handler.clock_nanos();
                let pending = self.ensure_class_initialized(class_name_bytes)?;
                #[cfg(feature = "parity-metrics")]
                crate::parity::count_clinit_time(self.handler.clock_nanos().saturating_sub(t0));
                if pending {
                    frame.pc = frame.inst_pc;
                    return Ok(());
                }
                let class_name = core::str::from_utf8(class_name_bytes)
                    .map_err(|_| JvmError::InvalidBytecode)?;
                // Refuse to instantiate abstract classes or interfaces
                let ci = find_class(self.classes, class_name.as_bytes());
                if let Some(target_cf) = ci.map(|i| &self.classes[i]) {
                    if target_cf.is_interface() || target_cf.is_abstract() {
                        return Err(JvmError::AbstractMethodError);
                    }
                }
                let static_name = helpers::class_name_to_static_in(
                    self.classes,
                    self.handler.native_class_names(),
                    class_name,
                );
                self.class_objects
                    .resolve
                    .insert_class(class_name_bytes, ci, static_name, true);
                static_name
            }
        };
        match self.objects.alloc_with_defaults(static_name, self.classes) {
            Some(obj_idx) => frame.push(Value::ObjectRef(obj_idx))?,
            None => {
                // Heap exhausted: rewind so the main loop collects and
                // re-executes this `new` — the `newarray` protocol. A
                // collection that frees nothing makes it a catchable
                // `OutOfMemoryError` there. Used to be a hard stop.
                frame.pc = frame.inst_pc;
                self.set_need_gc(true);
            }
        }
        Ok(())
    }
}

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// The `Object`-typed argument the builtins cannot format themselves —
    /// `String.valueOf(Object)` and `StringBuilder.append(Object)` — reached
    /// through a method reference: `op_invoke`'s `stringify_object_arg` never
    /// saw the call, so run the object's `toString()` here (a Java override
    /// through the upcall, the native identity form otherwise) before the
    /// arm sees it. Strings and `null` need nothing.
    pub(super) fn stringify_native_args(
        &mut self,
        frames: &mut Vec<Frame>,
        class: &str,
        name: &str,
        desc: &str,
        actual: &mut [Value],
    ) -> Result<(), JvmError> {
        let object_arg = match (class, name) {
            (c::java_lang_String, m::valueOf) => desc == d::Object__String,
            (c::java_lang_StringBuilder, m::append) => {
                desc == d::Object__StringBuilder || desc == d::CharSequence__StringBuilder
            }
            _ => false,
        };
        if !object_arg {
            return Ok(());
        }
        // The object is the last argument (the receiver, if any, precedes it).
        let Some(slot) = actual.last_mut() else {
            return Ok(());
        };
        let s = match *slot {
            Value::ObjectRef(_) => {
                self.invoke_java(frames, *slot, m::toString, d::__String, &[])?
            }
            Value::ArrayRef(_) => self.dispatch_native(
                c::java_lang_Object,
                m::toString,
                d::__String,
                &[*slot],
                frames,
            )?,
            _ => return Ok(()),
        };
        if let Some(s) = s {
            *slot = s;
        }
        Ok(())
    }

    /// `Enum.valueOf(Class<E> enumType, String name)`: the constant of the
    /// named enum class with that name — the static field of the class's
    /// own type whose object's `name` (field 0) matches — or
    /// `IllegalArgumentException`, as on Android (`NullPointerException`
    /// for a null name).
    pub(super) fn enum_value_of(&mut self, args: &[Value]) -> Result<Option<Value>, JvmError> {
        let Some(Value::ObjectRef(class_obj)) = args.first().copied() else {
            return Err(self.runtime_fault(c::java_lang_NullPointerException));
        };
        let Some(Value::Reference(wanted)) = args.get(1).copied() else {
            return Err(self.runtime_fault(c::java_lang_NullPointerException));
        };
        let Some(Value::Reference(name_idx)) = self.objects.get_field(class_obj, 0) else {
            return Err(JvmError::InvalidReference);
        };
        let ci = {
            let class_name = self
                .strings
                .resolve(name_idx)
                .ok_or(JvmError::InvalidReference)?;
            find_class(self.classes, class_name.as_bytes()).ok_or(JvmError::ClassNotFound)?
        };
        let cf = &self.classes[ci];
        let cn: &'static [u8] = cf.class_name().ok_or(JvmError::InvalidBytecode)?;
        for field in cf.static_fields() {
            let Some(desc) = cf.cp_utf8(field.descriptor_index) else {
                continue;
            };
            // The constants are exactly the static fields typed as the enum itself.
            let own_type = desc.len() == cn.len() + 2
                && desc[0] == b'L'
                && desc[desc.len() - 1] == b';'
                && &desc[1..desc.len() - 1] == cn;
            if !own_type {
                continue;
            }
            let Some(field_name) = cf.cp_utf8(field.name_index) else {
                continue;
            };
            let constant = self.statics.get(cn, field_name);
            if let Value::ObjectRef(obj) = constant {
                if let Some(Value::Reference(n)) = self.objects.get_field(obj, 0) {
                    if self.strings.content_eq(n, wanted) {
                        return Ok(Some(constant));
                    }
                }
            }
        }
        let mut msg: Vec<u8> = Vec::with_capacity(cn.len() + 32);
        msg.extend_from_slice(b"No enum constant ");
        msg.extend(cn.iter().map(|&b| if b == b'/' { b'.' } else { b }));
        msg.push(b'.');
        if let Some(w) = self.strings.resolve(wanted) {
            msg.extend_from_slice(w.as_bytes());
        }
        let e = self
            .objects
            .alloc(c::java_lang_IllegalArgumentException)
            .ok_or(JvmError::StackOverflow)?;
        if let Some(m) = self.strings.intern_dyn_owned(msg) {
            let _ = self.objects.register_exception_message(e, m);
        }
        Err(JvmError::Exception(e))
    }

    /// Box a native arm's primitive result when the SAM returns a reference.
    pub(super) fn box_native_result(
        &mut self,
        result: Option<Value>,
        box_return: u8,
    ) -> Result<Option<Value>, JvmError> {
        match result {
            Some(v) if box_return != 0 => {
                let boxed = helpers::box_primitive(self.objects, box_return, widen(v, box_return))
                    .ok_or(JvmError::StackOverflow)?;
                Ok(Some(boxed))
            }
            other => Ok(other),
        }
    }
}

/// Return the super class name of `class_name` if it's in the loaded set.
pub(super) fn find_super_class<'a>(
    classes: &'a [crate::class_file::ClassFile],
    class_name: &str,
) -> Option<&'a str> {
    let cf = find_class(classes, class_name.as_bytes()).map(|i| &classes[i])?;
    let super_bytes = cf.super_class_name()?;
    core::str::from_utf8(super_bytes).ok()
}

/// Widen an unboxed value to the body's parameter kind (an `Integer` passed
/// where the body takes `long`, `float` or `double`); every other
/// combination is already the right `Value`.
pub(super) fn widen(v: Value, kind: u8) -> Value {
    match (kind, v) {
        (b'J', Value::Int(i)) => Value::Long(i as i64),
        (b'F', Value::Int(i)) => Value::Float(i as f32),
        (b'D', Value::Int(i)) => Value::Double(i as f64),
        (b'D', Value::Float(f)) => Value::Double(f as f64),
        _ => v,
    }
}
