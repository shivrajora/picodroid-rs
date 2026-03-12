use crate::framework::{
    class_file::ClassFile,
    frame::Frame,
    heap::StringTable,
    native::NativeMethodHandler,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

pub fn execute(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    handler: &mut impl NativeMethodHandler,
    class_idx: usize,
    method_idx: usize,
    args: &[Value],
) -> Result<Option<Value>, JvmError> {
    let mut frame = Frame::new(class_idx, method_idx, args)?;

    loop {
        let cf = &classes[frame.class_idx];
        let method = &cf.methods[frame.method_idx];
        let code = cf.method_code(method);

        if frame.pc >= code.len() {
            return Ok(None);
        }

        let opcode = code[frame.pc];
        frame.pc += 1;

        match opcode {
            // nop
            0x00 => {}

            // aconst_null
            0x01 => frame.push(Value::Null)?,

            // iconst_<n>: -1..5
            0x02 => frame.push(Value::Int(-1))?,
            0x03 => frame.push(Value::Int(0))?,
            0x04 => frame.push(Value::Int(1))?,
            0x05 => frame.push(Value::Int(2))?,
            0x06 => frame.push(Value::Int(3))?,
            0x07 => frame.push(Value::Int(4))?,
            0x08 => frame.push(Value::Int(5))?,

            // bipush
            0x10 => {
                let b = code[frame.pc] as i8;
                frame.pc += 1;
                frame.push(Value::Int(b as i32))?;
            }

            // sipush
            0x11 => {
                let hi = code[frame.pc] as i16;
                let lo = code[frame.pc + 1] as i16;
                frame.pc += 2;
                frame.push(Value::Int(((hi << 8) | lo) as i32))?;
            }

            // ldc
            0x12 => {
                let cp_idx = code[frame.pc] as u16;
                frame.pc += 1;
                let v = resolve_ldc(cf, strings, cp_idx)?;
                frame.push(v)?;
            }

            // ldc_w
            0x13 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = resolve_ldc(cf, strings, cp_idx)?;
                frame.push(v)?;
            }

            // iload (index u8)
            0x15 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }

            // aload (index u8)
            0x19 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }

            // iload_<n>
            0x1a => {
                let v = frame.load_local(0)?;
                frame.push(v)?;
            }
            0x1b => {
                let v = frame.load_local(1)?;
                frame.push(v)?;
            }
            0x1c => {
                let v = frame.load_local(2)?;
                frame.push(v)?;
            }
            0x1d => {
                let v = frame.load_local(3)?;
                frame.push(v)?;
            }

            // aload_<n>
            0x2a => {
                let v = frame.load_local(0)?;
                frame.push(v)?;
            }
            0x2b => {
                let v = frame.load_local(1)?;
                frame.push(v)?;
            }
            0x2c => {
                let v = frame.load_local(2)?;
                frame.push(v)?;
            }
            0x2d => {
                let v = frame.load_local(3)?;
                frame.push(v)?;
            }

            // istore (index u8)
            0x36 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }

            // astore (index u8)
            0x3a => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.pop()?;
                frame.store_local(idx, v)?;
            }

            // istore_<n>
            0x3b => {
                let v = frame.pop()?;
                frame.store_local(0, v)?;
            }
            0x3c => {
                let v = frame.pop()?;
                frame.store_local(1, v)?;
            }
            0x3d => {
                let v = frame.pop()?;
                frame.store_local(2, v)?;
            }
            0x3e => {
                let v = frame.pop()?;
                frame.store_local(3, v)?;
            }

            // astore_<n>
            0x4b => {
                let v = frame.pop()?;
                frame.store_local(0, v)?;
            }
            0x4c => {
                let v = frame.pop()?;
                frame.store_local(1, v)?;
            }
            0x4d => {
                let v = frame.pop()?;
                frame.store_local(2, v)?;
            }
            0x4e => {
                let v = frame.pop()?;
                frame.store_local(3, v)?;
            }

            // pop
            0x57 => {
                frame.pop()?;
            }

            // pop2
            0x58 => {
                frame.pop()?;
                frame.pop()?;
            }

            // dup
            0x59 => {
                let v = frame.pop()?;
                frame.push(v)?;
                frame.push(v)?;
            }

            // iadd
            0x60 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_add(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // isub
            0x64 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => frame.push(Value::Int(a.wrapping_sub(b)))?,
                    _ => return Err(JvmError::InvalidBytecode),
                }
            }

            // iinc: local[index] += const
            0x84 => {
                let idx = code[frame.pc];
                let inc = code[frame.pc + 1] as i8;
                frame.pc += 2;
                if let Value::Int(n) = frame.load_local(idx)? {
                    frame.store_local(idx, Value::Int(n.wrapping_add(inc as i32)))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
            }

            // ifeq: branch if TOS == 0
            0x99 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = frame.pop()?;
                if matches!(v, Value::Int(0) | Value::Null) {
                    frame.pc = branch_target(frame.pc, offset);
                }
            }

            // ifne: branch if TOS != 0
            0x9a => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = frame.pop()?;
                if !matches!(v, Value::Int(0) | Value::Null) {
                    frame.pc = branch_target(frame.pc, offset);
                }
            }

            // iflt
            0x9b => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n < 0 {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // ifge
            0x9c => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n >= 0 {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // ifgt
            0x9d => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n > 0 {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // ifle
            0x9e => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let Value::Int(n) = frame.pop()? {
                    if n <= 0 {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpeq
            0x9f => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let b = frame.pop()?;
                let a = frame.pop()?;
                if a == b {
                    frame.pc = branch_target(frame.pc, offset);
                }
            }

            // if_icmpne
            0xa0 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let b = frame.pop()?;
                let a = frame.pop()?;
                if a != b {
                    frame.pc = branch_target(frame.pc, offset);
                }
            }

            // if_icmplt: branch if a < b (a=below TOS, b=TOS)
            0xa1 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a < b {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpge
            0xa2 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a >= b {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmpgt
            0xa3 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a > b {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // if_icmple
            0xa4 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                if let (Value::Int(b), Value::Int(a)) = (frame.pop()?, frame.pop()?) {
                    if a <= b {
                        frame.pc = branch_target(frame.pc, offset);
                    }
                }
            }

            // goto — signed 16-bit offset from opcode start (opcode is at pc-1)
            0xa7 => {
                let offset = i16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                // goto instruction starts at frame.pc - 1; offset is from that start
                frame.pc = ((frame.pc as i32) - 1 + offset as i32) as usize;
            }

            // ireturn
            0xac => {
                let v = frame.pop()?;
                return Ok(Some(v));
            }

            // areturn
            0xb0 => {
                let v = frame.pop()?;
                return Ok(Some(v));
            }

            // return (void)
            0xb1 => return Ok(None),

            // getstatic — for M2 we only see this for PeripheralManager; push Null placeholder
            0xb2 => {
                frame.pc += 2;
                frame.push(Value::Null)?;
            }

            // getfield — objectref → value
            0xb4 => {
                let _cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        let v = objects.get_field(idx, 0).unwrap_or(Value::Null);
                        frame.push(v)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            // putfield — objectref, value →
            0xb5 => {
                let _cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let value = frame.pop()?;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        objects
                            .set_field(idx, 0, value)
                            .ok_or(JvmError::InvalidReference)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            // invokevirtual
            0xb6 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let (class_bytes, name_bytes, desc_bytes) =
                    cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                let class_str =
                    core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let name_str =
                    core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let desc_str =
                    core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let arg_count = 1 + count_args(desc_str); // +1 for `this`
                let result = invoke_method(
                    classes,
                    strings,
                    objects,
                    handler,
                    class_str,
                    name_str,
                    desc_str,
                    &mut frame.stack,
                    arg_count,
                )?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
            }

            // invokespecial (constructor / private)
            0xb7 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let (class_bytes, name_bytes, desc_bytes) =
                    cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                let class_str =
                    core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let name_str =
                    core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let desc_str =
                    core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let arg_count = 1 + count_args(desc_str); // +1 for `this`
                let result = invoke_method(
                    classes,
                    strings,
                    objects,
                    handler,
                    class_str,
                    name_str,
                    desc_str,
                    &mut frame.stack,
                    arg_count,
                )?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
            }

            // invokestatic
            0xb8 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let (class_bytes, name_bytes, desc_bytes) =
                    cf.cp_methodref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                let class_str =
                    core::str::from_utf8(class_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let name_str =
                    core::str::from_utf8(name_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let desc_str =
                    core::str::from_utf8(desc_bytes).map_err(|_| JvmError::InvalidBytecode)?;
                let arg_count = count_args(desc_str);
                let result = invoke_method(
                    classes,
                    strings,
                    objects,
                    handler,
                    class_str,
                    name_str,
                    desc_str,
                    &mut frame.stack,
                    arg_count,
                )?;
                if let Some(v) = result {
                    frame.push(v)?;
                }
            }

            // new — allocate object, push objectref
            0xbb => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let class_name = cf
                    .cp_class_name(cp_idx)
                    .and_then(|b| core::str::from_utf8(b).ok())
                    .ok_or(JvmError::InvalidBytecode)?;
                let static_name = class_name_to_static(class_name);
                let obj_idx = objects.alloc(static_name).ok_or(JvmError::StackOverflow)?;
                frame.push(Value::ObjectRef(obj_idx))?;
            }

            // checkcast — no-op for M2
            0xc0 => {
                frame.pc += 2;
            }

            op => return Err(JvmError::UnsupportedOpcode(op)),
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn invoke_method(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    handler: &mut impl NativeMethodHandler,
    class_str: &str,
    name_str: &str,
    desc_str: &str,
    stack: &mut heapless::Vec<Value, 16>,
    arg_count: usize,
) -> Result<Option<Value>, JvmError> {
    let stack_len = stack.len();
    if stack_len < arg_count {
        return Err(JvmError::StackUnderflow);
    }
    let mut args_buf = [Value::Null; 8];
    let start = stack_len - arg_count;
    args_buf[..arg_count].copy_from_slice(&stack[start..]);
    for _ in 0..arg_count {
        stack.pop();
    }
    let args_ref = &args_buf[..arg_count];

    if let Some((ci, mi)) = find_method(classes, class_str, name_str, desc_str) {
        let is_native = classes[ci].methods[mi].code_offset == 0;
        if is_native {
            handler.dispatch(class_str, name_str, desc_str, args_ref, strings, objects)
        } else {
            execute(classes, strings, objects, handler, ci, mi, args_ref)
        }
    } else {
        handler.dispatch(class_str, name_str, desc_str, args_ref, strings, objects)
    }
}

fn resolve_ldc(cf: &ClassFile, strings: &mut StringTable, cp_idx: u16) -> Result<Value, JvmError> {
    if let Some(utf8) = cf.cp_string_utf8(cp_idx) {
        let ref_idx = strings.intern(utf8).ok_or(JvmError::StackOverflow)?;
        return Ok(Value::Reference(ref_idx));
    }
    if let Some(n) = cf.cp_integer(cp_idx) {
        return Ok(Value::Int(n));
    }
    Err(JvmError::InvalidBytecode)
}

fn find_method(
    classes: &[ClassFile],
    class_name: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    for (ci, cf) in classes.iter().enumerate() {
        let cn = cf.class_name()?;
        if cn != class_name.as_bytes() {
            continue;
        }
        for (mi, m) in cf.methods.iter().enumerate() {
            let mn = cf.cp_utf8(m.name_index)?;
            let md = cf.cp_utf8(m.descriptor_index)?;
            if mn == method_name.as_bytes() && md == descriptor.as_bytes() {
                return Some((ci, mi));
            }
        }
    }
    None
}

fn count_args(descriptor: &str) -> usize {
    let inner = descriptor
        .strip_prefix('(')
        .and_then(|s| s.find(')').map(|i| &s[..i]))
        .unwrap_or("");
    let mut count = 0;
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            'L' => {
                for c2 in chars.by_ref() {
                    if c2 == ';' {
                        break;
                    }
                }
                count += 1;
            }
            '[' => {}
            'J' | 'D' => count += 2,
            _ => count += 1,
        }
    }
    count
}

/// Branch target: offset is relative to the start of the branch instruction.
/// By the time we use this, frame.pc points 2 bytes past the offset field,
/// i.e. 3 bytes past the opcode. So instruction_start = frame.pc - 3.
#[inline]
fn branch_target(pc_after_offset: usize, offset: i16) -> usize {
    ((pc_after_offset as i32) - 3 + offset as i32) as usize
}

fn class_name_to_static(name: &str) -> &'static str {
    match name {
        "picodroid/pio/Gpio" => "picodroid/pio/Gpio",
        "picodroid/pio/PeripheralManager" => "picodroid/pio/PeripheralManager",
        "picodroid/os/SystemClock" => "picodroid/os/SystemClock",
        "picodroid/util/Log" => "picodroid/util/Log",
        _ => "unknown",
    }
}
