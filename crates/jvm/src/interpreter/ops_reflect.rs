// SPDX-License-Identifier: GPL-3.0-only
//! Reflection-lite: `Class.forName(String)`, `Class.newInstance()` and the
//! framework's `LayoutInflater.nativeNewView(Class, Context, AttributeSet)`
//! (docs/designs/class-newinstance-2026-10.md).
//!
//! These run as interpreter prechecks, like `Enum.valueOf` and
//! `ArrayList.sort`: a handler arm sees only a `NativeContext`, which has no
//! way to run `<init>` or `<clinit>`, while here the whole `Executor` is
//! still in hand. No `Constructor`, `Method` or `Field` objects: a Class is
//! a name, and a constructor is found by descriptor and run as a frame.

use super::{helpers, Executor, MAX_FRAME_DEPTH, MAX_UPCALL_DEPTH};
use crate::class_file::find_class;
use crate::frame::Frame;
use crate::names::{c, d};
use crate::native::NativeMethodHandler;
use crate::types::{JvmError, Value};
use alloc::vec::Vec;

/// What a failed construction throws.
#[derive(Clone, Copy)]
enum Refusal {
    /// `InstantiationException`, as `Class.newInstance()` specifies for an
    /// interface, an abstract class or a class with no such constructor.
    Instantiation,
    /// `null`: the framework caller (`LayoutInflater`) throws its own
    /// `InflateException` naming the layout element.
    Null,
}

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    /// `Class.forName(String)`: the canonical `Class` of a loaded class or a
    /// classfile-less builtin, by its binary (dotted) name, as `getName()`
    /// spelled it. Under `--shrink-app` both sides carry the shrunk name, so
    /// the round trip holds. `ClassNotFoundException` with the name as its
    /// message otherwise.
    pub(super) fn class_for_name(&mut self, args: &[Value]) -> Result<Option<Value>, JvmError> {
        let Some(Value::Reference(name_idx)) = args.first().copied() else {
            return Err(self.runtime_fault(c::java_lang_NullPointerException));
        };
        let dotted = self
            .strings
            .resolve(name_idx)
            .ok_or(JvmError::InvalidReference)?;
        let slashed: Vec<u8> = dotted
            .bytes()
            .map(|b| if b == b'.' { b'/' } else { b })
            .collect();
        let name: Option<&'static [u8]> = match find_class(self.classes, &slashed) {
            Some(ci) => self.classes[ci].class_name(),
            None => crate::native::BUILTIN_CLASS_NAMES
                .iter()
                .find(|n| n.as_bytes() == slashed.as_slice())
                .map(|n| n.as_bytes()),
        };
        match name {
            Some(name) => helpers::class_object_for_name(
                self.classes,
                self.strings,
                self.objects,
                self.class_objects,
                name,
            )
            .map(Some),
            None => Err(self.reflect_fault(c::java_lang_ClassNotFoundException, Some(name_idx))),
        }
    }

    /// `Class.newInstance()`: a new instance through the no-argument
    /// constructor, the class initialised first if it never was.
    pub(super) fn class_new_instance(
        &mut self,
        frames: &mut Vec<Frame>,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let Some(Value::ObjectRef(class_obj)) = args.first().copied() else {
            return Err(self.runtime_fault(c::java_lang_NullPointerException));
        };
        self.construct(frames, class_obj, d::__V, &[], Refusal::Instantiation)
    }

    /// `LayoutInflater.nativeNewView(Class, Context, AttributeSet)`: a view
    /// of the app's own through its `(Context, AttributeSet)` constructor,
    /// or `null` when the class has none.
    pub(super) fn inflater_new_view(
        &mut self,
        frames: &mut Vec<Frame>,
        args: &[Value],
    ) -> Result<Option<Value>, JvmError> {
        let Some(Value::ObjectRef(class_obj)) = args.first().copied() else {
            return Err(self.runtime_fault(c::java_lang_NullPointerException));
        };
        let extra = [
            args.get(1).copied().unwrap_or(Value::Null),
            args.get(2).copied().unwrap_or(Value::Null),
        ];
        self.construct(
            frames,
            class_obj,
            d::Context_AttributeSet__V,
            &extra,
            Refusal::Null,
        )
    }

    /// Allocate an instance of the class `class_obj` names and run the
    /// constructor with descriptor `desc` on it, with `extra` after the
    /// receiver, synchronously; the object is the result. A class file is
    /// required (a builtin such as `String` cannot be constructed this way),
    /// and so is a concrete class with that constructor.
    fn construct(
        &mut self,
        frames: &mut Vec<Frame>,
        class_obj: u16,
        desc: &str,
        extra: &[Value],
        refusal: Refusal,
    ) -> Result<Option<Value>, JvmError> {
        let Some(Value::Reference(name_idx)) = self.objects.get_field(class_obj, 0) else {
            return Err(JvmError::InvalidReference);
        };
        let ci = {
            let class_name = self
                .strings
                .resolve(name_idx)
                .ok_or(JvmError::InvalidReference)?;
            find_class(self.classes, class_name.as_bytes())
        };
        let Some(ci) = ci else {
            return self.refuse(refusal, name_idx);
        };
        {
            let cf = &self.classes[ci];
            if cf.is_interface() || cf.is_abstract() {
                return self.refuse(refusal, name_idx);
            }
        }
        let Some(mi) = helpers::find_method_in(self.classes, ci, "<init>", desc) else {
            return self.refuse(refusal, name_idx);
        };
        if self.upcall_depth >= MAX_UPCALL_DEPTH {
            let e = self.stack_overflow_error()?;
            return Err(JvmError::Exception(e));
        }
        // JVMS §5.5: the class is initialised before its constructor runs.
        // `op_new` rewinds and lets the main loop run the queued `<clinit>`
        // frames; a precheck has no instruction to re-execute (its arguments
        // are already off the operand stack), so it runs them here, to
        // completion, the way an upcall runs a callee.
        if self.ensure_class_initialized_at(ci)? {
            let pending = core::mem::take(&mut self.pending_clinit_frames);
            self.run_frames(frames, pending)?;
        }
        // Allocation failure is the hard `StackOverflow`: `finalize_native`
        // turns it into a collection and a re-execution of the invoke, and
        // the class is initialised by then.
        let obj = self
            .objects
            .alloc_instance(ci, self.classes)
            .ok_or(JvmError::StackOverflow)?;
        let mut all: Vec<Value> = Vec::with_capacity(extra.len() + 1);
        all.push(Value::ObjectRef(obj));
        all.extend_from_slice(extra);
        let frame = {
            let cf = &self.classes[ci];
            let m = &cf.methods()[mi];
            Frame::new(
                ci,
                mi,
                &all,
                cf.method_max_locals(m),
                cf.method_max_stack(m),
            )?
        };
        // The receiver and the arguments are rooted by the frame once it is
        // pushed; nothing allocates between here and there.
        self.run_frames(frames, alloc::vec![frame])?;
        Ok(Some(Value::ObjectRef(obj)))
    }

    /// Push `pushed` (top of the list runs first) and run until every one
    /// has returned, restoring the caller's frame stack on a hard error.
    fn run_frames(&mut self, frames: &mut Vec<Frame>, pushed: Vec<Frame>) -> Result<(), JvmError> {
        let base = frames.len();
        if base + pushed.len() > MAX_FRAME_DEPTH {
            let e = self.stack_overflow_error()?;
            return Err(JvmError::Exception(e));
        }
        if frames.try_reserve(pushed.len()).is_err() {
            return Err(JvmError::StackOverflow);
        }
        frames.extend(pushed);
        self.upcall_depth += 1;
        let r = self.run(frames, base);
        self.upcall_depth -= 1;
        if r.is_err() {
            frames.truncate(base);
        }
        r.map(|_| ())
    }

    fn refuse(&mut self, refusal: Refusal, name_idx: u16) -> Result<Option<Value>, JvmError> {
        match refusal {
            Refusal::Instantiation => {
                Err(self.reflect_fault(c::java_lang_InstantiationException, Some(name_idx)))
            }
            Refusal::Null => Ok(Some(Value::Null)),
        }
    }

    /// A reflective exception, with the class name as its message when the
    /// heap can spare the entry.
    fn reflect_fault(&mut self, class: &'static str, msg_idx: Option<u16>) -> JvmError {
        let Some(exc) = self.objects.alloc(class) else {
            return JvmError::StackOverflow;
        };
        if let Some(msg) = msg_idx {
            let _ = self.objects.register_exception_message(exc, msg);
        }
        JvmError::Exception(exc)
    }
}
