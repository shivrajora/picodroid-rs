//! A `no_std` Java bytecode interpreter for bare-metal embedded systems.
//!
//! `pico-jvm` parses and executes Java `.class` files on `no_std + alloc`
//! targets with no OS or hardware dependencies.  It is the core of
//! [Picodroid](https://github.com/shivrajora/picodroid-rs), a stripped-down
//! Android-style runtime for the Raspberry Pi Pico, but can be embedded in
//! any Rust project.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use pico_jvm::{Jvm, SharedJvmHeap, NativeContext, NativeMethodHandler};
//! use pico_jvm::types::{JvmError, Value};
//!
//! // 1. Implement NativeMethodHandler for your platform.
//! struct MyHandler;
//! impl NativeMethodHandler for MyHandler {
//!     fn dispatch(
//!         &mut self,
//!         class_name: &str,
//!         method_name: &str,
//!         _ctx: &mut NativeContext<'_>,
//!     ) -> Option<Result<Option<Value>, JvmError>> {
//!         match (class_name, method_name) {
//!             ("com/example/Io", "println") => {
//!                 // write to your platform's output
//!                 Some(Ok(None))
//!             }
//!             _ => None, // fall through to BuiltinHandler (java/lang/*)
//!         }
//!     }
//! }
//!
//! // 2. Embed compiled .class bytes (e.g. via include_bytes! or build.rs).
//! static MY_CLASS: &[u8] = include_bytes!("MyApp.class");
//!
//! // 3. Run.
//! let mut jvm = Jvm::new();
//! let mut heap = SharedJvmHeap::new();
//! jvm.load_class(MY_CLASS).unwrap();
//! jvm.invoke_static("MyApp", "main", &mut heap, &mut MyHandler).unwrap();
//! ```
//!
//! # Native method dispatch
//!
//! Java `native` methods (and any method not found in loaded `.class` files) are
//! routed to your [`NativeMethodHandler`] implementation via
//! [`NativeMethodHandler::dispatch`].  Return `Some(result)` to handle a call, or
//! `None` to pass it to the built-in [`BuiltinHandler`], which covers the
//! `java/lang/String`, `java/lang/StringBuilder`, and `java/lang/Object` families.
//! If neither handler claims the call, [`JvmError::NoSuchMethod`] is returned.
//!
//! # `no_std` usage
//!
//! The crate is `#![no_std]` and requires only `alloc`.  Add it as a dependency
//! with the default features:
//!
//! ```toml
//! [dependencies]
//! pico-jvm = "0.1"
//! ```

#![no_std]

extern crate alloc;

pub mod apk;
pub mod array_heap;
pub mod class_file;
pub mod frame;
pub mod heap;
pub mod interpreter;
pub mod native;
pub mod object_heap;
pub mod static_fields;
pub mod types;

use alloc::vec::Vec;
use array_heap::ArrayHeap;
use class_file::ClassFile;
use heap::StringTable;
pub use native::{BuiltinHandler, NativeContext, NativeMethodHandler};
use object_heap::ObjectHeap;
use static_fields::StaticFieldStore;
use types::{JvmError, Value};

// ── SharedJvmHeap ─────────────────────────────────────────────────────────────

/// All JVM runtime state bundled into one struct.
///
/// The caller owns and stores this (e.g. as a `static` or on the stack) and
/// passes `&mut SharedJvmHeap` into [`Jvm::invoke_static`] /
/// [`Jvm::invoke_instance`] on each call.  Keeping it separate from [`Jvm`]
/// lets multiple `Jvm` instances (e.g. per-thread) share the same heap.
pub struct SharedJvmHeap {
    /// Object instance storage.
    pub objects: ObjectHeap,
    /// Array storage.
    pub arrays: ArrayHeap,
    /// Interned string storage.
    pub strings: StringTable,
    /// Static field storage.
    pub statics: StaticFieldStore,
}

impl SharedJvmHeap {
    /// Creates an empty heap.  `const`-compatible so it can initialise a
    /// `static` without a runtime constructor.
    pub const fn new() -> Self {
        Self {
            objects: ObjectHeap::new(),
            arrays: ArrayHeap::new(),
            strings: StringTable::new(),
            statics: StaticFieldStore::new(),
        }
    }
}

impl Default for SharedJvmHeap {
    fn default() -> Self {
        Self::new()
    }
}

// ── Jvm ──────────────────────────────────────────────────────────────────────

/// A Java bytecode interpreter.
///
/// `Jvm` holds the set of loaded [`ClassFile`]s and executes bytecode against
/// a caller-supplied [`SharedJvmHeap`].  Create one `Jvm` per execution
/// context (e.g. per thread), load the required classes with
/// [`load_class`](Jvm::load_class), then drive execution with
/// [`invoke_static`](Jvm::invoke_static) or
/// [`invoke_instance`](Jvm::invoke_instance).
pub struct Jvm {
    classes: Vec<ClassFile>,
}

impl Jvm {
    /// Creates a new, empty interpreter with no classes loaded.
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
        }
    }
}

impl Default for Jvm {
    fn default() -> Self {
        Self::new()
    }
}

impl Jvm {
    /// Parses and registers a compiled `.class` file.
    ///
    /// `data` must be a `'static` byte slice (e.g. embedded via `include_bytes!`
    /// or a build-script generated constant) because the interpreter holds
    /// references into it for the lifetime of the `Jvm`.
    ///
    /// # Errors
    /// Returns [`JvmError::InvalidBytecode`] if `data` is not a valid `.class`
    /// file.
    pub fn load_class(&mut self, data: &'static [u8]) -> Result<(), JvmError> {
        let cf = ClassFile::parse(data).map_err(|_| JvmError::InvalidBytecode)?;
        self.classes.push(cf);
        Ok(())
    }

    /// Invokes a static method with no arguments.
    ///
    /// Locates the first method named `method_name` in the class named
    /// `class_name` (using JVM internal form, e.g. `"com/example/MyApp"`) and
    /// executes it.  The descriptor is not checked — load only one overload per
    /// name if disambiguation is needed.
    ///
    /// # Errors
    /// Returns [`JvmError::MethodNotFound`] if the class or method cannot be
    /// found, or any execution error propagated from the bytecode.
    pub fn invoke_static(
        &mut self,
        class_name: &str,
        method_name: &str,
        heap: &mut SharedJvmHeap,
        handler: &mut impl NativeMethodHandler,
    ) -> Result<(), JvmError> {
        let (ci, mi) = find_method_by_name(&self.classes, class_name, method_name)?;
        interpreter::execute(
            &self.classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            handler,
            ci,
            mi,
            &[],
        )?;
        Ok(())
    }

    /// Invokes an instance method on an object already in the heap.
    ///
    /// `obj_ref` is the [`ObjectHeap`] index of the receiver (`this`).  The
    /// method is looked up by name in `class_name`; use the runtime class of
    /// the object when virtual dispatch is desired.
    ///
    /// # Errors
    /// Returns [`JvmError::MethodNotFound`] if the class or method cannot be
    /// found, or any execution error propagated from the bytecode.
    pub fn invoke_instance(
        &mut self,
        class_name: &str,
        method_name: &str,
        obj_ref: u16,
        heap: &mut SharedJvmHeap,
        handler: &mut impl NativeMethodHandler,
    ) -> Result<(), JvmError> {
        let (ci, mi) = find_method_by_name(&self.classes, class_name, method_name)?;
        interpreter::execute(
            &self.classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            handler,
            ci,
            mi,
            &[Value::ObjectRef(obj_ref)],
        )?;
        Ok(())
    }
}

/// Find a class + method index by name (descriptor-agnostic).
fn find_method_by_name(
    classes: &[ClassFile],
    class_name: &str,
    method_name: &str,
) -> Result<(usize, usize), JvmError> {
    classes
        .iter()
        .enumerate()
        .find_map(|(ci, cf)| {
            let cn = cf.class_name()?;
            if cn != class_name.as_bytes() {
                return None;
            }
            cf.methods.iter().enumerate().find_map(|(mi, m)| {
                let mn = cf.cp_utf8(m.name_index)?;
                if mn == method_name.as_bytes() {
                    Some((ci, mi))
                } else {
                    None
                }
            })
        })
        .ok_or(JvmError::MethodNotFound)
}
