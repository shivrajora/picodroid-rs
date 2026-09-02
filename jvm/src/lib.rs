// SPDX-License-Identifier: GPL-3.0-only
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
//! pico-jvm = "0.2"
//! ```

#![no_std]

extern crate alloc;

pub mod array_heap;
pub mod atomic_section;
pub(crate) mod chunked_slots;
pub mod class_file;
pub mod class_objects;
pub mod frame;
pub mod gc;
pub mod heap;
pub mod interpreter;
#[cfg(feature = "mem-diag")]
pub mod mem_diag;
pub mod names;
pub mod native;
pub mod object_heap;
#[cfg(feature = "parity-metrics")]
pub mod parity;
pub mod sort;
pub mod static_fields;
pub mod tunables;
pub mod types;

use alloc::vec::Vec;
use array_heap::ArrayHeap;
use class_file::ClassFile;
use class_objects::ClassObjectCache;
use gc::GcState;
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
    /// Reusable GC buffers (persistent to avoid heap fragmentation).
    pub gc_state: GcState,
    /// Cached `java.lang.Class` objects, one per loaded class. See
    /// [`class_objects`] for why this lives on the shared heap.
    pub class_objects: ClassObjectCache,
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
            gc_state: GcState::new(),
            class_objects: ClassObjectCache::new(),
        }
    }
}

impl SharedJvmHeap {
    /// Clears all heap state — call before running a new app.
    /// Drops all objects, arrays, interned strings, and static fields.
    pub fn reset(&mut self) {
        *self = SharedJvmHeap::new();
    }

    /// Boot-time pre-reservation across the three heaps (PEM-3): claim
    /// steady-state slot chunks and arena capacity while the native heap is
    /// young and contiguous, so this permanent storage doesn't get
    /// allocated mid-heap during Activity churn and strand the free space
    /// around it. Values are board-tuned; zeros are no-ops. Best-effort —
    /// a refused reservation leaves on-demand growth in place. Call again
    /// after [`reset`], which drops the claim.
    pub fn prereserve(
        &mut self,
        obj_chunks: usize,
        fields_values: usize,
        arr_chunks: usize,
        arena_values: usize,
        arena8_bytes: usize,
        str_chunks: usize,
    ) {
        self.objects.prereserve(obj_chunks, fields_values);
        self.arrays
            .prereserve(arr_chunks, arena_values, arena8_bytes);
        self.strings.prereserve_dyn(str_chunks);
    }

    /// Runs a full GC cycle from *outside* the interpreter.
    ///
    /// Native code that allocates directly on the heap between bytecode
    /// executions (e.g. the sensor-event drain loop) has no safepoint where
    /// the interpreter's alloc-counter / `need_gc` emergency GC could run,
    /// so a failed allocation there would otherwise never be relieved. No
    /// bytecode frames exist at such a call site; the root set is static
    /// fields, cached `Class` objects, and the handler's native roots.
    ///
    /// Returns the number of heap entries freed.
    pub fn collect_now(&mut self, handler: &mut impl NativeMethodHandler) -> usize {
        let pre_gc_used =
            self.objects.live_bytes() + self.arrays.live_bytes() + self.strings.live_bytes();
        let t0 = handler.clock_nanos();
        let freed = gc::collect(
            &[],
            &mut self.objects,
            &mut self.arrays,
            &mut self.strings,
            &self.statics,
            &self.class_objects,
            &mut self.gc_state,
            |visit| handler.gc_visit_roots(visit),
        );
        let t1 = handler.clock_nanos();
        handler.report_gc(t1.wrapping_sub(t0), freed, pre_gc_used);
        interpreter::prune_monitors(handler, &self.objects, &self.arrays, &self.strings);
        self.gc_state.alloc_count = 0;
        self.gc_state.need_gc = false;
        #[cfg(feature = "mem-diag")]
        {
            let post_gc_live =
                self.objects.live_bytes() + self.arrays.live_bytes() + self.strings.live_bytes();
            self.gc_state
                .note_gc_cycle(freed, pre_gc_used, post_gc_live);
            if mem_diag::offensive() {
                if let Err(m) =
                    mem_diag::integrity_check(&self.objects, &self.arrays, &self.strings)
                {
                    panic!("mem-diag post-GC integrity violation: {m}");
                }
            }
        }
        freed
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
/// a caller-supplied [`SharedJvmHeap`].  Load the required classes with
/// [`load_class`](Jvm::load_class), then drive execution with
/// [`invoke_static`](Jvm::invoke_static) or
/// [`invoke_instance`](Jvm::invoke_instance).
///
/// The `invoke_*` family takes `&self` — the class set is read-only during
/// execution — so multiple execution contexts (threads) can share one loaded
/// `Jvm` rather than each paying for a private parsed-metadata copy
/// (measured ≈14 KB per child for a ~160-class app). The lazy
/// [`ClassFile`] parse is interior-mutable; sharing across cooperative
/// tasks is sound because `Parsed::parse` contains no yield point.
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

    /// Like [`new`](Self::new) but pre-sizes the class table.
    ///
    /// Avoids the Vec doubling cascade (and its transient double-allocation)
    /// when the caller already knows the final framework + app class count.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            classes: Vec::with_capacity(capacity),
        }
    }
}

impl Default for Jvm {
    fn default() -> Self {
        Self::new()
    }
}

impl Jvm {
    /// Read-only access to the loaded class table. Callers that need
    /// `ClassFile` metadata (e.g. `ObjectHeap::alloc_with_defaults`) take
    /// `&[ClassFile]` — this accessor avoids exposing the inner `Vec`.
    pub fn classes(&self) -> &[ClassFile] {
        &self.classes
    }

    /// Returns (parsed, total) counts for currently loaded classes.
    ///
    /// `ClassFile::register` (called by `load_class`) produces a lazy entry:
    /// only the constant pool is scanned for the class name, and the full
    /// method/field tables stay unparsed until first access. This accessor
    /// exposes how many classes have been forced past that lazy state — a
    /// direct measure of the lazy-load win on any given run.
    pub fn count_parsed(&self) -> (usize, usize) {
        let parsed = self.classes.iter().filter(|c| c.is_parsed()).count();
        (parsed, self.classes.len())
    }

    /// Total RAM held by parsed class metadata across the loaded set, as
    /// `(host_bytes, device_bytes)` — see
    /// [`ClassFile::parsed_metadata_bytes`]. The census uses this to price
    /// each executor's metadata (a `Thread.start` child that loads its own
    /// class set duplicates all of it — handover §6).
    #[cfg(feature = "mem-diag")]
    pub fn parsed_metadata_bytes(&self) -> (usize, usize) {
        let mut host = 0;
        let mut dev = 0;
        for c in &self.classes {
            if let Some((h, d)) = c.parsed_metadata_bytes() {
                host += h;
                dev += d;
            }
        }
        (host, dev)
    }

    /// RAM held by the class *registration* table itself — the per-entry
    /// `ClassFile` structs, paid for every registered class whether or not
    /// it ever parses. `(host_bytes, device_bytes)`; device: 20 B per entry
    /// (two 8 B Flash slices at 4-byte pointers + a 4 B `OnceCell<Box>`)
    /// plus the Vec header and its heap_4 block header.
    #[cfg(feature = "mem-diag")]
    pub fn class_table_bytes(&self) -> (usize, usize) {
        let host = core::mem::size_of::<Vec<ClassFile>>()
            + self.classes.capacity() * core::mem::size_of::<ClassFile>();
        let dev = if self.classes.capacity() > 0 {
            12 + 8 + self.classes.capacity() * 20
        } else {
            12
        };
        (host, dev)
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
        let cf = ClassFile::register(data).map_err(|_| JvmError::InvalidBytecode)?;
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
        &self,
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
            &mut heap.gc_state,
            &mut heap.class_objects,
            handler,
            ci,
            mi,
            &[],
        )?;
        Ok(())
    }

    /// Invokes a static method with explicit arguments.
    ///
    /// Like [`invoke_static`] but accepts a `Value` slice for the method
    /// parameters — e.g. used by the Executor drain path to pass a queued
    /// `Runnable` reference into a one-line static bridge that then invokes
    /// `run()` via bytecode (so lambda proxies are resolved by the
    /// interpreter's invokeinterface path).
    ///
    /// # Errors
    /// Returns [`JvmError::MethodNotFound`] if the class or method cannot be
    /// found, or any execution error propagated from the bytecode.
    pub fn invoke_static_with_args(
        &self,
        class_name: &str,
        method_name: &str,
        args: &[Value],
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
            &mut heap.gc_state,
            &mut heap.class_objects,
            handler,
            ci,
            mi,
            args,
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
        &self,
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
            &mut heap.gc_state,
            &mut heap.class_objects,
            handler,
            ci,
            mi,
            &[Value::ObjectRef(obj_ref)],
        )?;
        Ok(())
    }

    /// Invoke an instance method with explicit arguments (beyond `this`).
    pub fn invoke_instance_with_args(
        &self,
        class_name: &str,
        method_name: &str,
        obj_ref: u16,
        extra_args: &[Value],
        heap: &mut SharedJvmHeap,
        handler: &mut impl NativeMethodHandler,
    ) -> Result<(), JvmError> {
        let (ci, mi) = find_method_by_name(&self.classes, class_name, method_name)?;
        let mut args = alloc::vec![Value::ObjectRef(obj_ref)];
        args.extend_from_slice(extra_args);
        interpreter::execute(
            &self.classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            &mut heap.gc_state,
            &mut heap.class_objects,
            handler,
            ci,
            mi,
            &args,
        )?;
        Ok(())
    }

    /// Same as [`invoke_instance_with_args`], but surfaces the method's
    /// return value. Used by the framework event loop to read e.g.
    /// `View.fireKey`'s `boolean` result so that BACK can fall through to
    /// `Activity.onBackPressed()` only when no listener consumed it.
    ///
    /// Kept as a separate function so existing `let _ = invoke_*` call
    /// sites don't need to change signature.
    pub fn invoke_instance_with_args_returning(
        &self,
        class_name: &str,
        method_name: &str,
        obj_ref: u16,
        extra_args: &[Value],
        heap: &mut SharedJvmHeap,
        handler: &mut impl NativeMethodHandler,
    ) -> Result<Option<Value>, JvmError> {
        let (ci, mi) = find_method_by_name(&self.classes, class_name, method_name)?;
        let mut args = alloc::vec![Value::ObjectRef(obj_ref)];
        args.extend_from_slice(extra_args);
        interpreter::execute(
            &self.classes,
            &mut heap.strings,
            &mut heap.objects,
            &mut heap.arrays,
            &mut heap.statics,
            &mut heap.gc_state,
            &mut heap.class_objects,
            handler,
            ci,
            mi,
            &args,
        )
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
            cf.methods().iter().enumerate().find_map(|(mi, m)| {
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
