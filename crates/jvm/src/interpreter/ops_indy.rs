// SPDX-License-Identifier: GPL-3.0-only
//! `invokedynamic` and lambdas: linking a call site to a `LambdaProxy`,
//! dispatching an interface call on one, and adapting its arguments.

use super::ops_invoke::widen;
use super::{helpers, Executor};
use crate::names::c;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    object_heap::{LambdaProxy, LambdaTarget},
    types::{JvmError, Slot, Value},
};
use alloc::vec::Vec;

/// What a SAM invocation on a lambda proxy amounts to.
pub(super) enum LambdaCall {
    /// Push this frame; its return value is the call's result.
    Frame(Frame),
    /// Push this `<init>` frame; `obj`, already allocated, is the call's
    /// result once it returns.
    Ctor { frame: Frame, obj: u16 },
    /// Ran natively; this is the call's result.
    Done(Option<Value>),
}

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// If the receiver at `stack[stack_len - arg_count]` is a lambda proxy
    /// and `name` is its SAM, pop the receiver and the arguments, run the
    /// lambda (see [`Self::lambda_call`]) and return `Ok(true)`. `Ok(false)`
    /// when the receiver isn't a lambda, or the call is to a default or
    /// `Object` method on one — the caller then resolves it like any other
    /// method, through the interface and `java/lang/Object`.
    pub(super) fn try_lambda_dispatch(
        &mut self,
        frames: &mut Vec<Frame>,
        arg_count: usize,
        name: &str,
        sam_desc: &str,
    ) -> Result<bool, JvmError> {
        let (obj_idx, start) = {
            let frame = frames.last().ok_or(JvmError::InvalidBytecode)?;
            let stack_len = frame.stack.len();
            if stack_len < arg_count {
                return Ok(false);
            }
            let Value::ObjectRef(obj_idx) = frame.stack[stack_len - arg_count] else {
                return Ok(false);
            };
            (obj_idx, stack_len - arg_count)
        };
        let Some(lambda) = self.objects.get_lambda(obj_idx) else {
            return Ok(false);
        };
        if lambda.sam_name != name.as_bytes() {
            return Ok(false);
        }
        // A body on a class that is not initialised yet (`Foo::new`,
        // `Util::helper`): run `<clinit>` first and re-execute this invoke,
        // the `new` / `invokestatic` pattern. The arguments are still on the
        // operand stack, so nothing is lost.
        let init_class: Option<&'static [u8]> = match lambda.target {
            LambdaTarget::Ctor {
                class_bytes,
                init: Some(_),
                ..
            } => Some(class_bytes),
            LambdaTarget::Java {
                class_idx,
                method_idx,
            } if self.classes[class_idx].methods()[method_idx].access_flags & 0x0008 != 0 => {
                self.classes[class_idx].class_name()
            }
            _ => None,
        };
        if let Some(cb) = init_class {
            if self.ensure_class_initialized(cb)? {
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                frame.pc = frame.inst_pc;
                return Ok(true);
            }
        }
        let method_args: Vec<Value> = {
            let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
            let args = frame.stack[start + 1..].to_vec();
            frame.stack.truncate(start);
            args
        };
        match self.lambda_call(frames, obj_idx, &method_args, sam_desc)? {
            LambdaCall::Frame(f) => self.pending_frame = Some(f),
            LambdaCall::Ctor { frame: f, obj } => {
                // `<init>` returns void; the object pushed underneath it is
                // what the call site sees once the constructor has run.
                let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                frame.push(Value::ObjectRef(obj))?;
                self.pending_frame = Some(f);
            }
            LambdaCall::Done(result) => {
                if let Some(v) = result {
                    let frame = frames.last_mut().ok_or(JvmError::InvalidBytecode)?;
                    frame.push(v)?;
                }
            }
        }
        Ok(true)
    }

    /// Handle `invokedynamic` (0xBA) for lambda expressions.
    pub(super) fn op_invokedynamic(
        &mut self,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
        frame.pc += 4; // skip index (2) + padding (2)

        let cf = &self.classes[frame.class_idx];

        // 1. Resolve CONSTANT_InvokeDynamic -> (bootstrap_idx, name_and_type_idx)
        let (bsm_idx, nat_idx) = cf
            .cp_invoke_dynamic(cp_idx)
            .ok_or(JvmError::InvalidBytecode)?;

        // 2. Get the NameAndType: the SAM's name, and the factory descriptor
        //    (captures in, functional interface out)
        let (sam_name, desc_bytes) = cf
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

        // 5. Resolve the MethodHandle's Methodref: what the proxy's SAM runs.
        let (target_class_bytes, target_name_bytes, target_desc_bytes) =
            cf.cp_methodref(ref_idx).ok_or(JvmError::InvalidBytecode)?;
        let target_class =
            core::str::from_utf8(target_class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let target_name =
            core::str::from_utf8(target_name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let target_desc =
            core::str::from_utf8(target_desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;
        let target = match ref_kind {
            // REF_invokeVirtual / REF_invokeInterface. javac names its own
            // synthetic body this way too: a private body is fixed here (a
            // subclass may carry a same-named `lambda$…`), everything else —
            // `String::length`, `Shape::area`, `s::trim` — dispatches on the
            // receiver's runtime class at each call, native when the class is
            // a builtin.
            5 | 9 => match helpers::find_method_walking(
                self.classes,
                target_class,
                target_name,
                target_desc,
            ) {
                Some((ci, mi)) if self.classes[ci].methods()[mi].access_flags & 0x0002 != 0 => {
                    LambdaTarget::Java {
                        class_idx: ci,
                        method_idx: mi,
                    }
                }
                _ => LambdaTarget::Virtual {
                    name: target_name,
                    desc: target_desc,
                },
            },
            // REF_invokeStatic / REF_invokeSpecial: a fixed body. A static
            // reference to a builtin (`Integer::parseInt`) is a native arm.
            6 | 7 => match helpers::find_method_walking(
                self.classes,
                target_class,
                target_name,
                target_desc,
            ) {
                Some((ci, mi)) => LambdaTarget::Java {
                    class_idx: ci,
                    method_idx: mi,
                },
                None if ref_kind == 6 => LambdaTarget::NativeStatic {
                    class: target_class,
                    name: target_name,
                    desc: target_desc,
                },
                None => LambdaTarget::Virtual {
                    name: target_name,
                    desc: target_desc,
                },
            },
            // REF_newInvokeSpecial: `Foo::new`. A loaded class runs its
            // `<init>` bytecode; a builtin (`ArrayList::new`) its native
            // constructor. A handle of this kind naming anything but a
            // constructor is a malformed class file.
            8 if target_name != "<init>" => {
                return Err(JvmError::UnsupportedInvokeDynamic(
                    "LambdaMetafactory(newInvokeSpecial on a non-constructor)",
                ))
            }
            8 => LambdaTarget::Ctor {
                class: helpers::class_name_to_static_in(
                    self.classes,
                    self.handler.native_class_names(),
                    target_class,
                ),
                class_bytes: target_class_bytes,
                init: helpers::find_method(self.classes, target_class, target_name, target_desc),
                desc: target_desc,
            },
            _ => {
                return Err(JvmError::UnsupportedInvokeDynamic(
                    "LambdaMetafactory(unsupported method handle kind)",
                ))
            }
        };

        // 6. Pop captured values from the operand stack, stored as 8 B slots
        // (a captured `long` is two). Reserved fallibly: the copy is one
        // more allocation on a path the heap may already have refused.
        let capture_count = helpers::count_args(factory_desc);
        let stack_len = frame.stack.len();
        // Stored as 8 B slots (a captured `long` is two). Reserved fallibly:
        // the copy is one more allocation on a path the heap may already
        // have refused (see `register_lambda` below).
        let captures: Vec<Slot> = if capture_count > 0 {
            let start = stack_len
                .checked_sub(capture_count)
                .ok_or(JvmError::StackUnderflow)?;
            let caps = Value::to_slot_vec(&frame.stack[start..])?;
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

        // 8. Register lambda metadata. A registry that cannot grow is the
        // crate's allocation-failure signal (a catchable OutOfMemoryError
        // upstairs); the proxy object just allocated is garbage then.
        self.objects
            .register_lambda(
                obj_idx,
                LambdaProxy {
                    target,
                    captures,
                    sam_name,
                },
            )
            .map_err(|_| JvmError::StackOverflow)?;

        // 9. Push the proxy object reference
        frame.push(Value::ObjectRef(obj_idx))?;
        Ok(())
    }

    /// Run the SAM of lambda proxy `obj_idx` on `args` (the interface-method
    /// arguments, excluding the proxy itself), applying `LambdaMetafactory`'s
    /// boxing adaptation: an argument is unboxed where the body takes a
    /// primitive, and a primitive result is boxed where the SAM returns a
    /// reference — kotlinc keeps a body primitive (`(I)I`) behind the erased
    /// `Function1.invoke(Object)Object`; a method reference such as
    /// `String::length` behind `Fn<String, Integer>` needs the same. Captured
    /// values lead the body's parameters; for an instance body the receiver
    /// (captured `this`, or the first SAM argument of `String::length`) is
    /// not a descriptor parameter and steps past none.
    ///
    /// Stack-independent: reads no operand stack and pushes no frame, so
    /// both `op_invoke`'s marshalling path and the native→Java upcall share
    /// it.
    pub(super) fn lambda_call(
        &mut self,
        frames: &mut Vec<Frame>,
        obj_idx: u16,
        args: &[Value],
        sam_desc: &str,
    ) -> Result<LambdaCall, JvmError> {
        let (target, mut actual) = {
            let lambda = self
                .objects
                .get_lambda(obj_idx)
                .ok_or(JvmError::InvalidReference)?;
            let mut actual: Vec<Value> = Vec::new();
            actual
                .try_reserve(lambda.captures.len() + args.len())
                .map_err(|_| JvmError::StackOverflow)?;
            Slot::to_values(&lambda.captures, &mut actual)?;
            (lambda.target, actual)
        };
        actual.extend_from_slice(args);

        // The body's descriptor, and whether `actual[0]` is a receiver the
        // descriptor does not list.
        let (impl_desc, has_receiver): (&[u8], bool) = match target {
            LambdaTarget::Java {
                class_idx,
                method_idx,
            } => {
                let tm = &self.classes[class_idx].methods()[method_idx];
                if tm.code_offset == 0 {
                    return Err(JvmError::NoSuchMethod);
                }
                let desc = self.classes[class_idx]
                    .cp_utf8(tm.descriptor_index)
                    .ok_or(JvmError::InvalidBytecode)?;
                // ACC_STATIC = 0x0008.
                (desc, tm.access_flags & 0x0008 == 0)
            }
            LambdaTarget::NativeStatic { desc, .. } => (desc.as_bytes(), false),
            LambdaTarget::Virtual { desc, .. } => (desc.as_bytes(), true),
            LambdaTarget::Ctor { desc, .. } => (desc.as_bytes(), false),
        };
        adapt_lambda_args(self.objects, &mut actual, impl_desc, has_receiver)?;
        let body_ret = helpers::return_kind(impl_desc);
        let box_return = if body_ret != b'L'
            && body_ret != b'V'
            && helpers::return_kind(sam_desc.as_bytes()) == b'L'
        {
            body_ret
        } else {
            0
        };

        match target {
            LambdaTarget::Java {
                class_idx,
                method_idx,
            } => {
                let tm = &self.classes[class_idx].methods()[method_idx];
                let mut f =
                    Frame::new(class_idx, method_idx, &actual, tm.max_locals, tm.max_stack)?;
                f.box_return = box_return;
                Ok(LambdaCall::Frame(f))
            }
            LambdaTarget::NativeStatic { class, name, desc } => {
                self.stringify_native_args(frames, class, name, desc, &mut actual)?;
                let mark = self.gc_state.push_shadow_roots(&actual);
                let r = self.dispatch_native(class, name, desc, &actual, frames);
                self.gc_state.truncate_shadow_roots(mark);
                Ok(LambdaCall::Done(self.box_native_result(r?, box_return)?))
            }
            LambdaTarget::Virtual { name, desc } => {
                let recv = actual.first().copied().unwrap_or(Value::Null);
                if matches!(recv, Value::Null) {
                    return Err(self.runtime_fault(c::java_lang_NullPointerException));
                }
                // A reference to another proxy's SAM (`Runnable::run` over a
                // lambda): the inner body, not the interface's empty method.
                if let Value::ObjectRef(ri) = recv {
                    let inner_sam = self
                        .objects
                        .get_lambda(ri)
                        .is_some_and(|l| l.sam_name == name.as_bytes());
                    if inner_sam {
                        let rest: Vec<Value> = actual[1..].to_vec();
                        return self.lambda_call(frames, ri, &rest, desc);
                    }
                }
                let class = self.runtime_class_of(recv)?;
                let resolved = helpers::find_method_walking_cached(
                    &mut self.class_objects.resolve,
                    self.classes,
                    class,
                    name,
                    desc,
                );
                match resolved {
                    Some((ci, mi)) if self.classes[ci].methods()[mi].code_offset != 0 => {
                        let tm = &self.classes[ci].methods()[mi];
                        let mut f = Frame::new(ci, mi, &actual, tm.max_locals, tm.max_stack)?;
                        f.box_return = box_return;
                        Ok(LambdaCall::Frame(f))
                    }
                    _ => {
                        self.stringify_native_args(frames, class, name, desc, &mut actual)?;
                        let mark = self.gc_state.push_shadow_roots(&actual);
                        let r = self.dispatch_native(class, name, desc, &actual, frames);
                        self.gc_state.truncate_shadow_roots(mark);
                        Ok(LambdaCall::Done(self.box_native_result(r?, box_return)?))
                    }
                }
            }
            LambdaTarget::Ctor {
                class,
                class_bytes,
                init,
                desc,
            } => match init {
                Some((ci, mi)) => {
                    // `op_invoke` initialises the class before coming here;
                    // an upcall cannot re-execute, so it refuses instead of
                    // constructing an uninitialised class.
                    if !self.statics.is_initialized(class_bytes) {
                        return Err(JvmError::UnsupportedInvokeDynamic(
                            "constructor reference to an uninitialised class from a native upcall",
                        ));
                    }
                    let obj = self
                        .objects
                        .alloc_with_defaults(class, self.classes)
                        .ok_or(JvmError::StackOverflow)?;
                    let mut all: Vec<Value> = Vec::with_capacity(actual.len() + 1);
                    all.push(Value::ObjectRef(obj));
                    all.extend_from_slice(&actual);
                    let tm = &self.classes[ci].methods()[mi];
                    let f = Frame::new(ci, mi, &all, tm.max_locals, tm.max_stack)?;
                    Ok(LambdaCall::Ctor { frame: f, obj })
                }
                None => {
                    let obj = self.objects.alloc(class).ok_or(JvmError::StackOverflow)?;
                    let mut all: Vec<Value> = Vec::with_capacity(actual.len() + 1);
                    all.push(Value::ObjectRef(obj));
                    all.extend_from_slice(&actual);
                    let mark = self.gc_state.push_shadow_roots(&all);
                    let r = self.dispatch_native(class, "<init>", desc, &all, frames);
                    self.gc_state.truncate_shadow_roots(mark);
                    // A native `<init>` is void, except `String`'s, which
                    // hands back the interned string in place of the
                    // placeholder (see `finalize_invoke`).
                    Ok(LambdaCall::Done(Some(r?.unwrap_or(Value::ObjectRef(obj)))))
                }
            },
        }
    }
}

/// Unbox every argument whose body parameter is primitive. `actual` holds
/// the captures followed by the SAM arguments; when `has_receiver`, its
/// first element is the receiver and lines up with no descriptor parameter.
pub(super) fn adapt_lambda_args(
    objects: &mut crate::object_heap::ObjectHeap,
    actual: &mut [Value],
    impl_desc: &[u8],
    has_receiver: bool,
) -> Result<(), JvmError> {
    let skip = usize::from(has_receiver);
    let kinds = helpers::ParamKinds::new(impl_desc);
    for (arg, kind) in actual.iter_mut().skip(skip).zip(kinds) {
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
    Ok(())
}
