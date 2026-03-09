use crate::framework::{
    class_file::ClassFile,
    frame::Frame,
    heap::StringTable,
    native,
    types::{JvmError, Value},
};

/// Execute a method by class index and method index within the provided class list.
/// `classes` is the full loaded class table (needed for cross-class method resolution).
pub fn execute(
    classes: &[ClassFile],
    strings: &mut StringTable,
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
            // Implicit return at end of bytecode
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

            // bipush: push signed byte as int
            0x10 => {
                let b = code[frame.pc] as i8;
                frame.pc += 1;
                frame.push(Value::Int(b as i32))?;
            }

            // sipush: push signed short as int
            0x11 => {
                let hi = code[frame.pc] as i16;
                let lo = code[frame.pc + 1] as i16;
                frame.pc += 2;
                frame.push(Value::Int(((hi << 8) | lo) as i32))?;
            }

            // ldc: load constant from CP (index u8)
            0x12 => {
                let cp_idx = code[frame.pc] as u16;
                frame.pc += 1;
                let v = resolve_ldc(cf, strings, cp_idx)?;
                frame.push(v)?;
            }

            // ldc_w: ldc with u16 index
            0x13 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let v = resolve_ldc(cf, strings, cp_idx)?;
                frame.push(v)?;
            }

            // iload: load int local (index u8)
            0x15 => {
                let idx = code[frame.pc];
                frame.pc += 1;
                let v = frame.load_local(idx)?;
                frame.push(v)?;
            }

            // aload: load reference local (index u8)
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

            // iadd
            0x60 => {
                let b = frame.pop()?;
                let a = frame.pop()?;
                if let (Value::Int(a), Value::Int(b)) = (a, b) {
                    frame.push(Value::Int(a.wrapping_add(b)))?;
                } else {
                    return Err(JvmError::InvalidBytecode);
                }
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

                // Collect arguments from operand stack based on descriptor
                let arg_count = count_args(desc_str);
                let stack_len = frame.stack.len();
                if stack_len < arg_count {
                    return Err(JvmError::StackUnderflow);
                }
                let args_slice = &frame.stack[stack_len - arg_count..];
                let mut args_buf = [Value::Null; 8];
                args_buf[..arg_count].copy_from_slice(args_slice);
                let args_ref = &args_buf[..arg_count];
                // Pop args off the stack
                for _ in 0..arg_count {
                    frame.stack.pop();
                }

                // Try to find the method in the loaded classes first
                if let Some((ci, mi)) = find_method(classes, class_str, name_str, desc_str) {
                    let target_method = &classes[ci].methods[mi];
                    if target_method.code_offset == 0 {
                        // Native method
                        let result = native::dispatch_native(
                            class_str, name_str, desc_str, args_ref, strings,
                        )?;
                        if let Some(v) = result {
                            frame.push(v)?;
                        }
                    } else {
                        // JVM method — recursive call
                        let result = execute(classes, strings, ci, mi, args_ref)?;
                        if let Some(v) = result {
                            frame.push(v)?;
                        }
                    }
                } else {
                    // Not found in loaded classes — try native dispatch
                    let result =
                        native::dispatch_native(class_str, name_str, desc_str, args_ref, strings)?;
                    if let Some(v) = result {
                        frame.push(v)?;
                    }
                }
            }

            op => return Err(JvmError::UnsupportedOpcode(op)),
        }
    }
}

fn resolve_ldc(cf: &ClassFile, strings: &mut StringTable, cp_idx: u16) -> Result<Value, JvmError> {
    // Try String constant
    if let Some(utf8) = cf.cp_string_utf8(cp_idx) {
        let ref_idx = strings.intern(utf8).ok_or(JvmError::StackOverflow)?;
        return Ok(Value::Reference(ref_idx));
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

/// Count the number of method arguments from a JVM descriptor like "(Ljava/lang/String;II)V".
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
                // Object type — skip until ';'
                for c2 in chars.by_ref() {
                    if c2 == ';' {
                        break;
                    }
                }
                count += 1;
            }
            '[' => {}                // array modifier — don't count separately
            'J' | 'D' => count += 2, // long/double take 2 slots
            _ => count += 1,
        }
    }
    count
}
