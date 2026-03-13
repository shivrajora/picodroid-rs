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

        let opcode = code[frame.pc];
        frame.pc += 1;

        match opcode {
            0x00..=0x13 => ex.op_constants(opcode, code, &mut frame)?,
            0x15 | 0x17 | 0x19 | 0x1a..=0x1d | 0x22..=0x25 | 0x2a..=0x2d => {
                ex.op_locals_load(opcode, code, &mut frame)?
            }
            0x2e | 0x32..=0x35 => ex.op_array_load(opcode, &mut frame)?,
            0x36 | 0x38 | 0x3a | 0x3b..=0x3e | 0x43..=0x46 | 0x4b..=0x4e => {
                ex.op_locals_store(opcode, code, &mut frame)?
            }
            0x4f | 0x53..=0x56 => ex.op_array_store(opcode, &mut frame)?,
            0x57..=0x59 => ex.op_stack(opcode, &mut frame)?,
            0x60..=0x84 => ex.op_math(opcode, code, &mut frame)?,
            0x86 | 0x8b | 0x91..=0x93 | 0x95..=0x96 => ex.op_convert(opcode, &mut frame)?,
            0x99..=0xa7 | 0xc0 | 0xc1 => ex.op_control(opcode, code, &mut frame)?,
            // Returns handled inline — these need to return from execute()
            0xac | 0xae | 0xb0 => {
                let v = frame.pop()?;
                return Ok(Some(v));
            }
            0xb1 => return Ok(None),
            0xb2 | 0xb4 | 0xb5 => ex.op_fields(opcode, code, &mut frame)?,
            0xb6..=0xb9 => ex.op_invoke(opcode, code, &mut frame)?,
            0xbb => ex.op_new(code, &mut frame)?,
            0xbc..=0xbe => ex.op_array_alloc(opcode, code, &mut frame)?,
            op => return Err(JvmError::UnsupportedOpcode(op)),
        }
    }
}
