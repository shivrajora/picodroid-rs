#![no_std]

extern crate alloc;

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
//
// Bundles all JVM runtime state (objects, arrays, strings, statics) into a
// single struct.  The app crate owns the global instance; callers pass
// `&mut SharedJvmHeap` into `invoke_static` / `invoke_instance` rather than
// relying on a module-level static.

pub struct SharedJvmHeap {
    pub objects: ObjectHeap,
    pub arrays: ArrayHeap,
    pub strings: StringTable,
    pub statics: StaticFieldStore,
}

impl SharedJvmHeap {
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

pub struct Jvm {
    classes: Vec<ClassFile>,
}

impl Jvm {
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
    pub fn load_class(&mut self, data: &'static [u8]) -> Result<(), JvmError> {
        let cf = ClassFile::parse(data).map_err(|_| JvmError::InvalidBytecode)?;
        self.classes.push(cf);
        Ok(())
    }

    /// Invoke a static (no-arg) method by class and method name.
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

    /// Invoke an instance method on an object already in the shared heap.
    /// `obj_ref` is the ObjectHeap index of `this`.
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
