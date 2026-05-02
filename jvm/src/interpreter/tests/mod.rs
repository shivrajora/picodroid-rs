// SPDX-License-Identifier: GPL-3.0-only
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
        &mut crate::class_objects::ClassObjectCache::new(),
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
        &mut crate::class_objects::ClassObjectCache::new(),
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

/// Build a minimal class file for a single public method `m()I` with the
/// given Code attribute parameters. The constant pool is fixed (see the
/// hand-written test classes in this directory for the layout). Methods
/// needing a longer CP must keep using the hand-written form.
///
/// The returned Vec must be `Box::leak`'d or kept alive for the duration
/// of the test — the JVM keeps string/class-name references into the data.
#[allow(dead_code)]
fn build_class(max_stack: u16, max_locals: u16, code: &[u8]) -> alloc::vec::Vec<u8> {
    use alloc::vec::Vec;
    let mut out: Vec<u8> = Vec::new();
    // Shared 84-byte header (cp + access + this/super + ifaces/fields +
    // method header up to "Code" attr name and length placeholders).
    // Same layout as the hand-written CLASS_* constants in this module.
    out.extend_from_slice(&[
        0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x08, 0x07, 0x00, 0x02, 0x01, 0x00,
        0x01, 0x54, 0x07, 0x00, 0x04, 0x01, 0x00, 0x10, 0x6A, 0x61, 0x76, 0x61, 0x2F, 0x6C, 0x61,
        0x6E, 0x67, 0x2F, 0x4F, 0x62, 0x6A, 0x65, 0x63, 0x74, 0x01, 0x00, 0x01, 0x6D, 0x01, 0x00,
        0x03, 0x28, 0x29, 0x49, 0x01, 0x00, 0x04, 0x43, 0x6F, 0x64, 0x65, 0x00, 0x01, 0x00, 0x01,
        0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x06, 0x00,
        0x01, 0x00, 0x07,
    ]);
    // Code attribute: u32 attr_len | u16 max_stack | u16 max_locals |
    // u32 code_len | [code] | u16 ex_table_len | u16 code_attrs_count
    let code_len = code.len() as u32;
    let attr_len = 2 + 2 + 4 + code_len + 2 + 2;
    out.extend_from_slice(&attr_len.to_be_bytes());
    out.extend_from_slice(&max_stack.to_be_bytes());
    out.extend_from_slice(&max_locals.to_be_bytes());
    out.extend_from_slice(&code_len.to_be_bytes());
    out.extend_from_slice(code);
    // exception_table_length (0) + attributes_count (0) for this Code
    out.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    // class-level attributes_count (0)
    out.extend_from_slice(&[0x00, 0x00]);
    out
}

/// Run a method built from a raw bytecode slice via `build_class`.
/// Leaks the class bytes for the duration of the test — fine because these
/// are #[test] functions with no lifetime concerns.
#[allow(dead_code)]
fn run_code(max_stack: u16, max_locals: u16, code: &[u8]) -> Result<Option<Value>, JvmError> {
    let bytes = build_class(max_stack, max_locals, code);
    let leaked: &'static [u8] = alloc::boxed::Box::leak(bytes.into_boxed_slice());
    run(leaked)
}

mod arrays;
mod class_literal;
mod clinit;
mod constants;
mod control;
mod convert;
mod exceptions;
mod fields;
mod invoke;
mod locals;
mod long_arrays;
mod math;
mod multianewarray;
mod stack;
mod stack_manip;
mod wide_goto;
