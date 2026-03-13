use crate::framework::{
    array_heap::ArrayHeap,
    class_file::ClassFile,
    frame::Frame,
    heap::StringTable,
    native::NativeMethodHandler,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

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
mod ops_stack;

#[cfg(test)]
mod tests;

pub(crate) struct Executor<'a, H: NativeMethodHandler> {
    pub classes: &'a [ClassFile],
    pub strings: &'a mut StringTable,
    pub objects: &'a mut ObjectHeap,
    pub arrays: &'a mut ArrayHeap,
    pub handler: &'a mut H,
}

/// Search `method`'s exception table for a handler covering `inst_pc` that
/// catches the class of `obj_idx`.  Returns the handler bytecode offset on
/// a match, or `None` if no handler applies.
fn find_exception_handler(
    cf: &ClassFile,
    method: &crate::framework::class_file::MethodInfo,
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

#[allow(clippy::too_many_arguments)]
pub fn execute<H: NativeMethodHandler>(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    handler: &mut H,
    class_idx: usize,
    method_idx: usize,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut frame = Frame::new(class_idx, method_idx, args)?;
    let mut ex = Executor {
        classes,
        strings,
        objects,
        arrays,
        handler,
    };

    loop {
        let cf = &ex.classes[frame.class_idx];
        let method = &cf.methods[frame.method_idx];
        let code = cf.method_code(method);

        if frame.pc >= code.len() {
            return Ok(None);
        }

        // Save instruction start PC for exception table lookup.
        let inst_pc = frame.pc;
        let opcode = code[frame.pc];
        frame.pc += 1;

        // Return opcodes are handled inline — they cannot throw Java exceptions.
        match opcode {
            0xac | 0xae | 0xb0 => {
                let v = frame.pop()?;
                return Ok(Some(v));
            }
            0xb1 => return Ok(None),
            _ => {}
        }

        let r: Result<(), JvmError> = match opcode {
            0x00..=0x13 => ex.op_constants(opcode, code, &mut frame),
            0x15 | 0x17 | 0x19 | 0x1a..=0x1d | 0x22..=0x25 | 0x2a..=0x2d => {
                ex.op_locals_load(opcode, code, &mut frame)
            }
            0x2e | 0x32..=0x35 => ex.op_array_load(opcode, &mut frame),
            0x36 | 0x38 | 0x3a | 0x3b..=0x3e | 0x43..=0x46 | 0x4b..=0x4e => {
                ex.op_locals_store(opcode, code, &mut frame)
            }
            0x4f | 0x53..=0x56 => ex.op_array_store(opcode, &mut frame),
            0x57..=0x59 => ex.op_stack(opcode, &mut frame),
            0x60..=0x84 => ex.op_math(opcode, code, &mut frame),
            0x86 | 0x8b | 0x91..=0x93 | 0x95..=0x96 => ex.op_convert(opcode, &mut frame),
            0x99..=0xa7 | 0xc0 | 0xc1 => ex.op_control(opcode, code, &mut frame),
            0xb2 | 0xb4 | 0xb5 => ex.op_fields(opcode, code, &mut frame),
            0xb6..=0xb9 => ex.op_invoke(opcode, code, &mut frame),
            0xbb => ex.op_new(code, &mut frame),
            0xbc..=0xbe => ex.op_array_alloc(opcode, code, &mut frame),
            0xbf => ex.op_athrow(&mut frame),
            op => Err(JvmError::UnsupportedOpcode(op)),
        };

        match r {
            Ok(()) => {}
            Err(JvmError::Exception(obj_idx)) => {
                // Look for a matching catch handler in this frame.
                let cf = &ex.classes[frame.class_idx];
                let method = &cf.methods[frame.method_idx];
                if let Some(handler_pc) =
                    find_exception_handler(cf, method, inst_pc, obj_idx, ex.objects, ex.classes)
                {
                    frame.stack.clear();
                    frame.push(Value::ObjectRef(obj_idx))?;
                    frame.pc = handler_pc;
                } else {
                    // No handler — propagate to the caller.
                    return Err(JvmError::Exception(obj_idx));
                }
            }
            Err(e) => return Err(e),
        }
    }
}
