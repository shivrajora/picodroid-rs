use super::*;
use crate::{
    class_file::ClassFile,
    gc::GcState,
    heap::StringTable,
    native::{NativeContext, NativeMethodHandler},
    object_heap::ObjectHeap,
    static_fields::StaticFieldStore,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

struct NoopHandler;
impl NativeMethodHandler for NoopHandler {
    fn dispatch(
        &mut self,
        _class_name: &str,
        _method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        None
    }
}

// ── Helper to run a single-class, single-method test ──────────────────────

fn run(class_bytes: &'static [u8]) -> Result<Option<Value>, JvmError> {
    let cf = ClassFile::parse(class_bytes).expect("parse failed");
    let mut classes: Vec<ClassFile> = Vec::new();
    classes.push(cf);
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NoopHandler;
    execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut handler,
        0,
        0,
        &[],
    )
}

/// Run method 0 of class at `exec_class_idx` with the given args,
/// with all `classes_data` slices pre-loaded.
fn run_multi(
    classes_data: &[&'static [u8]],
    exec_class_idx: usize,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut classes: Vec<ClassFile> = Vec::new();
    for &data in classes_data {
        let cf = ClassFile::parse(data).expect("parse failed");
        classes.push(cf);
    }
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut handler = NoopHandler;
    execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut handler,
        exec_class_idx,
        0,
        args,
    )
}

/// Allocate an object with class_name `name` and return its ObjectRef.
fn alloc_object(objects: &mut ObjectHeap, name: &'static str) -> Value {
    Value::ObjectRef(objects.alloc(name).expect("alloc failed"))
}

mod arrays;
mod constants;
mod control;
mod convert;
mod fields;
mod invoke;
mod locals;
mod math;
mod stack;
