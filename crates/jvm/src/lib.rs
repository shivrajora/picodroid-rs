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
#[cfg(test)]
extern crate std;

extern crate alloc;

pub mod array_heap;
pub mod atomic_section;
pub(crate) mod chunked_slots;
pub mod class_file;
pub mod class_objects;
pub mod fconv;
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
pub mod resolve_cache;
pub mod sort;
pub mod static_fields;
#[cfg(test)]
mod test_alloc;
pub mod tunables;
pub mod types;

use alloc::vec::Vec;
use array_heap::ArrayHeap;
use class_file::{ClassFile, ClassIndex, Classes};
use class_objects::ClassObjectCache;
use gc::GcState;
use heap::StringTable;
pub use native::{BuiltinHandler, NativeContext, NativeMethodHandler};
use object_heap::ObjectHeap;
pub use resolve_cache::SiteKey;
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
        // Load-bearing, not belt and braces: the resolution tables key
        // virtual sites by heap class ids, which a fresh heap renumbers, and
        // a relaunch of the same app can put its `Vec<ClassFile>` at the
        // same address and length, so `ResolveCache::sync` alone would keep
        // stale entries.
        self.class_objects.resolve.clear();
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
        self.gc_state.prereserve_compact_buf();
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
/// execution — so multiple execution contexts (threads) share one loaded
/// `Jvm`. A [`ClassFile`] is two pointers into flash (the class bytes and
/// the link table built for them when the set was packed or embedded); the
/// table costs no RAM and nothing is parsed at load
/// (docs/designs/class-link-2026-09.md).
pub struct Jvm {
    classes: Vec<ClassFile>,
    /// Lookup by name over the framework and app sections' sorted indices
    /// (`class_file::index`); classes appended past them are scanned.
    index: ClassIndex,
    fw: Option<class_link::ClassSection<'static>>,
    app: Option<class_link::ClassSection<'static>>,
}

impl Jvm {
    /// Creates a new, empty interpreter with no classes loaded.
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
            index: ClassIndex::LINEAR,
            fw: None,
            app: None,
        }
    }

    /// Like [`new`](Self::new) but pre-sizes the class table.
    ///
    /// Avoids the Vec doubling cascade (and its transient double-allocation)
    /// when the caller already knows the final framework + app class count.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            classes: Vec::with_capacity(capacity),
            index: ClassIndex::LINEAR,
            fw: None,
            app: None,
        }
    }
}

impl Default for Jvm {
    fn default() -> Self {
        Self::new()
    }
}

impl Jvm {
    /// The loaded class table with its index — what every lookup by name
    /// takes (`ObjectHeap::alloc_with_defaults`, the natives' `ctx.classes`);
    /// dereferences to the table itself.
    pub fn classes(&self) -> Classes<'_> {
        Classes {
            files: &self.classes,
            index: &self.index,
        }
    }

    /// Number of loaded classes.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// RAM held by the class table — the per-entry `ClassFile` structs (the
    /// only per-class RAM there is: the tables live in flash) plus the
    /// `Vec` header. `(host_bytes, device_bytes)`: the device pays
    /// `size_of::<ClassFile>()` less [`class_file::CLASS_FILE_DELTA`] per
    /// entry and a 12 B header.
    pub fn class_table_bytes(&self) -> (usize, usize) {
        let cap = self.classes.capacity();
        let host = core::mem::size_of::<Vec<ClassFile>>() + cap * core::mem::size_of::<ClassFile>();
        let dev = 12 + cap * (core::mem::size_of::<ClassFile>() - class_file::CLASS_FILE_DELTA);
        (host, dev)
    }
}

impl Jvm {
    /// Register the framework's class section — the corpus embedded in
    /// firmware — which must come first: its index is searched first and
    /// its classes take indices `0..len`.
    pub fn load_framework(
        &mut self,
        section: class_link::ClassSection<'static>,
    ) -> Result<(), JvmError> {
        debug_assert!(self.classes.is_empty(), "the framework loads first");
        self.load_section(section)?;
        self.fw = Some(section);
        self.index = ClassIndex::new(self.fw.as_ref(), self.app.as_ref());
        Ok(())
    }

    /// Register the app's class section (its PAPK's CLASSES section), after
    /// the framework's.
    pub fn load_app(&mut self, section: class_link::ClassSection<'static>) -> Result<(), JvmError> {
        debug_assert!(self.app.is_none() && self.classes.len() == self.fw.map_or(0, |s| s.len()));
        self.load_section(section)?;
        self.app = Some(section);
        self.index = ClassIndex::new(self.fw.as_ref(), self.app.as_ref());
        Ok(())
    }

    /// Register every class of a class section, in directory order, past
    /// whatever is loaded; these classes are found by scanning (the tests'
    /// path — [`Self::load_framework`] and [`Self::load_app`] index theirs).
    /// O(1) per class: the section was validated when it was packed,
    /// embedded or installed, and each entry is two pointers into it.
    ///
    /// # Errors
    /// [`JvmError::InvalidBytecode`] if a record does not resolve (a
    /// section that never went through `validate_structure`).
    pub fn load_section(
        &mut self,
        section: class_link::ClassSection<'static>,
    ) -> Result<(), JvmError> {
        if self.classes.try_reserve(section.len()).is_err() {
            return Err(JvmError::StackOverflow);
        }
        for i in 0..section.len() {
            let linked = section.class(i).ok_or(JvmError::InvalidBytecode)?;
            self.load_linked(linked)?;
        }
        Ok(())
    }

    /// Register one class from its bytes and link table, past the indexed
    /// sections.
    pub fn load_linked(&mut self, linked: class_link::Linked<'static>) -> Result<(), JvmError> {
        let cf = ClassFile::linked(linked).map_err(|_| JvmError::InvalidBytecode)?;
        self.classes.push(cf);
        Ok(())
    }

    /// Link and register a compiled `.class` file — the host's and the
    /// tests' way in for class bytes no packer has linked (feature
    /// `link-at-load`; the table is built now and leaked).
    ///
    /// `data` must be a `'static` byte slice because the interpreter holds
    /// references into it for the lifetime of the `Jvm`.
    ///
    /// # Errors
    /// Returns [`JvmError::InvalidBytecode`] if `data` is not a valid `.class`
    /// file.
    #[cfg(any(test, feature = "link-at-load"))]
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
        &self,
        class_name: &str,
        method_name: &str,
        heap: &mut SharedJvmHeap,
        handler: &mut impl NativeMethodHandler,
    ) -> Result<(), JvmError> {
        let (ci, mi) = find_method_by_name(self.classes(), class_name, method_name)?;
        interpreter::execute_indexed(
            self.classes(),
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
        let (ci, mi) = find_method_by_name(self.classes(), class_name, method_name)?;
        interpreter::execute_indexed(
            self.classes(),
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
        let (ci, mi) = find_method_by_name(self.classes(), class_name, method_name)?;
        interpreter::execute_indexed(
            self.classes(),
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
        let (ci, mi) = find_method_by_name(self.classes(), class_name, method_name)?;
        let mut args = alloc::vec![Value::ObjectRef(obj_ref)];
        args.extend_from_slice(extra_args);
        interpreter::execute_indexed(
            self.classes(),
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
        let (ci, mi) = find_method_by_name(self.classes(), class_name, method_name)?;
        let mut args = alloc::vec![Value::ObjectRef(obj_ref)];
        args.extend_from_slice(extra_args);
        interpreter::execute_indexed(
            self.classes(),
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
    classes: Classes<'_>,
    class_name: &str,
    method_name: &str,
) -> Result<(usize, usize), JvmError> {
    class_file::find_class(classes, class_name.as_bytes())
        .map(|ci| (ci, &classes[ci]))
        .and_then(|(ci, cf)| {
            cf.methods().iter().enumerate().find_map(|(mi, m)| {
                let mn = cf.method_name(m)?;
                if mn == method_name.as_bytes() {
                    Some((ci, mi))
                } else {
                    None
                }
            })
        })
        .ok_or(JvmError::MethodNotFound)
}
