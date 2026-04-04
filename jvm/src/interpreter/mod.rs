use crate::{
    array_heap::ArrayHeap,
    class_file::ClassFile,
    frame::Frame,
    gc::GcState,
    heap::StringTable,
    native::NativeMethodHandler,
    object_heap::ObjectHeap,
    static_fields::StaticFieldStore,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

mod helpers;
mod ops_arrays;
mod ops_constants;
mod ops_control;
mod ops_convert;
mod ops_exceptions;
mod ops_fields;
mod ops_invoke;
mod ops_locals;
mod ops_math;
mod ops_monitor;
mod ops_stack;

#[cfg(test)]
mod tests;

/// Number of allocations between automatic GC cycles.
const GC_THRESHOLD: u16 = 256;

pub(crate) struct Executor<'a, H: NativeMethodHandler> {
    pub classes: &'a [ClassFile],
    pub strings: &'a mut StringTable,
    pub objects: &'a mut ObjectHeap,
    pub arrays: &'a mut ArrayHeap,
    pub statics: &'a mut StaticFieldStore,
    pub gc_state: &'a mut GcState,
    pub handler: &'a mut H,
    /// Cache: (class_name ptr, field_name ptr) → field slot index.
    pub field_cache: Vec<(*const u8, *const u8, usize)>,
    /// Cache: (class_name ptr, method_name ptr, desc ptr) → (class_idx, method_idx).
    pub method_cache: Vec<helpers::MethodCacheEntry>,
    /// Set by `op_invoke` when a Java method should be called; the main loop
    /// pushes this frame onto the frame stack on the next iteration.
    pub pending_frame: Option<Frame>,
    /// Allocation counter for GC triggering.
    pub alloc_count: u16,
}

/// Search `method`'s exception table for a handler covering `inst_pc` that
/// catches the class of `obj_idx`.  Returns the handler bytecode offset on
/// a match, or `None` if no handler applies.
fn find_exception_handler(
    cf: &ClassFile,
    method: &crate::class_file::MethodInfo,
    inst_pc: usize,
    obj_idx: u16,
    objects: &ObjectHeap,
    classes: &[ClassFile],
) -> Option<usize> {
    let exception_class = objects.class_name(obj_idx)?;
    for entry in &method.exception_table {
        let start = entry.start_pc as usize;
        let end = entry.end_pc as usize;
        if inst_pc >= start && inst_pc < end {
            if entry.catch_type_index == 0 {
                // catch-all (finally)
                return Some(entry.handler_pc as usize);
            }
            if let Some(class_bytes) = cf.cp_class_name(entry.catch_type_index) {
                if let Ok(catch_class) = core::str::from_utf8(class_bytes) {
                    if helpers::is_instance_of(classes, exception_class, catch_class) {
                        return Some(entry.handler_pc as usize);
                    }
                }
            }
        }
    }
    None
}

/// Handle a Java exception by walking the frame stack for a matching handler.
/// Returns `Ok(())` if a handler was found and the frame stack is set up to
/// continue execution, or `Err` if the exception should propagate to the caller.
fn handle_exception<H: NativeMethodHandler>(
    ex: &Executor<'_, H>,
    frames: &mut Vec<Frame>,
    obj_idx: u16,
) -> Result<(), JvmError> {
    loop {
        if let Some(f) = frames.last_mut() {
            let cf = &ex.classes[f.class_idx];
            let method = &cf.methods[f.method_idx];
            if let Some(handler_pc) =
                find_exception_handler(cf, method, f.inst_pc, obj_idx, ex.objects, ex.classes)
            {
                f.stack.clear();
                f.push(Value::ObjectRef(obj_idx))?;
                f.pc = handler_pc;
                return Ok(());
            }
            frames.pop();
        } else {
            return Err(JvmError::Exception(obj_idx));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute<H: NativeMethodHandler>(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    statics: &mut StaticFieldStore,
    gc_state: &mut GcState,
    handler: &mut H,
    class_idx: usize,
    method_idx: usize,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let initial_frame = Frame::new(class_idx, method_idx, args)?;
    let mut frames: Vec<Frame> = Vec::new();
    frames.push(initial_frame);

    let mut ex = Executor {
        classes,
        strings,
        objects,
        arrays,
        statics,
        gc_state,
        handler,
        field_cache: Vec::new(),
        method_cache: Vec::new(),
        pending_frame: None,
        alloc_count: 0,
    };

    loop {
        // Cooperative stop-point: checked once per bytecode instruction.
        if ex.handler.interrupted() {
            return Err(JvmError::Interrupted);
        }

        let frame = match frames.last_mut() {
            Some(f) => f,
            None => return Ok(None),
        };

        let cf = &ex.classes[frame.class_idx];
        let method = &cf.methods[frame.method_idx];
        let code = cf.method_code(method);

        if frame.pc >= code.len() {
            frames.pop();
            if frames.is_empty() {
                return Ok(None);
            }
            continue;
        }

        // Save instruction start PC for exception table lookup.
        frame.inst_pc = frame.pc;
        let opcode = code[frame.pc];
        frame.pc += 1;

        // Return opcodes are handled inline — they pop the frame stack.
        match opcode {
            0xac..=0xb0 => {
                let v = frame.pop()?;
                frames.pop();
                if frames.is_empty() {
                    return Ok(Some(v));
                }
                frames.last_mut().unwrap().push(v)?;
                continue;
            }
            0xb1 => {
                frames.pop();
                if frames.is_empty() {
                    return Ok(None);
                }
                continue;
            }
            _ => {}
        }

        let r: Result<(), JvmError> = match opcode {
            0x00..=0x14 => ex.op_constants(opcode, code, frame),
            0x15..=0x2d => ex.op_locals_load(opcode, code, frame),
            0x2e | 0x32..=0x35 => ex.op_array_load(opcode, frame),
            0x36..=0x4e => ex.op_locals_store(opcode, code, frame),
            0x4f | 0x53..=0x56 => ex.op_array_store(opcode, frame),
            0x57..=0x59 => ex.op_stack(opcode, frame),
            0x60..=0x84 => ex.op_math(opcode, code, frame),
            0x85..=0x98 => ex.op_convert(opcode, frame),
            0x99..=0xa7 | 0xaa | 0xab | 0xc0 | 0xc1 | 0xc6 | 0xc7 => {
                ex.op_control(opcode, code, frame)
            }
            0xb2..=0xb5 => ex.op_fields(opcode, code, frame),
            0xb6..=0xb9 => ex.op_invoke(opcode, code, frame),
            0xbb => ex.op_new(code, frame),
            0xbc..=0xbe => ex.op_array_alloc(opcode, code, frame),
            0xbf => ex.op_athrow(frame),
            0xc2 => ex.op_monitorenter(frame),
            0xc3 => ex.op_monitorexit(frame),
            op => Err(JvmError::UnsupportedOpcode(op)),
        };

        // If op_invoke resolved a Java method, push the new frame.
        if let Some(new_frame) = ex.pending_frame.take() {
            if r.is_ok() {
                frames.push(new_frame);
                continue;
            }
            // If the opcode errored, drop the pending frame and fall through
            // to exception handling below.
        }

        // Trigger GC when allocation counter crosses the threshold.
        if r.is_ok() && ex.alloc_count >= GC_THRESHOLD {
            let t0 = ex.handler.clock_nanos();
            let freed = crate::gc::collect(
                &frames,
                ex.objects,
                ex.arrays,
                ex.strings,
                ex.statics,
                ex.gc_state,
            );
            let t1 = ex.handler.clock_nanos();
            ex.handler.report_gc(t1.wrapping_sub(t0), freed);
            ex.alloc_count = 0;
        }

        match r {
            Ok(()) => {}
            Err(JvmError::Exception(obj_idx)) => {
                handle_exception(&ex, &mut frames, obj_idx)?;
            }
            Err(e) => return Err(e),
        }
    }
}
