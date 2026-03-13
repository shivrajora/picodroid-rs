use crate::framework::{
    array_heap::ArrayHeap,
    class_file::ClassFile,
    heap::StringTable,
    native::NativeMethodHandler,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke_method<H: NativeMethodHandler>(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    handler: &mut H,
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
            handler.dispatch(
                class_str, name_str, desc_str, args_ref, strings, objects, arrays,
            )
        } else {
            super::execute(classes, strings, objects, arrays, handler, ci, mi, args_ref)
        }
    } else {
        handler.dispatch(
            class_str, name_str, desc_str, args_ref, strings, objects, arrays,
        )
    }
}

pub(super) fn resolve_ldc(
    cf: &ClassFile,
    strings: &mut StringTable,
    cp_idx: u16,
) -> Result<Value, JvmError> {
    if let Some(utf8) = cf.cp_string_utf8(cp_idx) {
        let ref_idx = strings.intern(utf8).ok_or(JvmError::StackOverflow)?;
        return Ok(Value::Reference(ref_idx));
    }
    if let Some(n) = cf.cp_integer(cp_idx) {
        return Ok(Value::Int(n));
    }
    if let Some(f) = cf.cp_float(cp_idx) {
        return Ok(Value::Float(f));
    }
    Err(JvmError::InvalidBytecode)
}

pub(super) fn find_method(
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

pub(super) fn count_args(descriptor: &str) -> usize {
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
pub(super) fn branch_target(pc_after_offset: usize, offset: i16) -> usize {
    ((pc_after_offset as i32) - 3 + offset as i32) as usize
}

/// Computes the runtime field slot for a named field, walking from the root of the hierarchy down.
/// Super-class fields come first (slot 0), then subclass fields.
/// Returns None if the field is not found or the hierarchy is too deep (>8 levels).
pub(super) fn field_slot(
    classes: &[ClassFile],
    class_name: &str,
    field_name: &str,
) -> Option<usize> {
    // Build a chain of class indices from root to leaf (root first)
    let mut chain: heapless::Vec<usize, 8> = heapless::Vec::new();
    let mut current: &str = class_name;
    loop {
        let ci = classes
            .iter()
            .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()))?;
        chain.push(ci).ok()?; // returns None if depth > 8
        match classes[ci].super_class_name() {
            None => break, // reached java/lang/Object
            Some(super_bytes) => {
                let super_str: &'static str = core::str::from_utf8(super_bytes).ok()?;
                current = super_str;
            }
        }
    }
    chain.reverse(); // root first

    let mut slot = 0usize;
    for ci in chain.iter() {
        let cf = &classes[*ci];
        for fi in 0..cf.fields.len() {
            if cf.field_name(fi)? == field_name.as_bytes() {
                return Some(slot);
            }
            slot += 1;
        }
    }
    None
}

/// Returns true if `runtime_class` is the same as or a subclass of `target_class`.
pub(super) fn is_instance_of(
    classes: &[ClassFile],
    runtime_class: &str,
    target_class: &str,
) -> bool {
    let mut current: &str = runtime_class;
    loop {
        if current == target_class {
            return true;
        }
        let ci = match classes
            .iter()
            .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()))
        {
            Some(i) => i,
            None => return false,
        };
        match classes[ci].super_class_name() {
            None => return false,
            Some(super_bytes) => match core::str::from_utf8(super_bytes) {
                Ok(s) => current = s,
                Err(_) => return false,
            },
        }
    }
}

/// Virtual dispatch: find a method starting from `runtime_class`, walking up the hierarchy.
pub(super) fn find_method_virtual(
    classes: &[ClassFile],
    runtime_class: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let mut current: &str = runtime_class;
    loop {
        if let Some(result) = find_method(classes, current, method_name, descriptor) {
            return Some(result);
        }
        let ci = classes
            .iter()
            .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()))?;
        let super_bytes = classes[ci].super_class_name()?;
        let super_str: &'static str = core::str::from_utf8(super_bytes).ok()?;
        current = super_str;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn invoke_method_virtual<H: NativeMethodHandler>(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    arrays: &mut ArrayHeap,
    handler: &mut H,
    runtime_class: &str,
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

    if let Some((ci, mi)) = find_method_virtual(classes, runtime_class, name_str, desc_str) {
        let is_native = classes[ci].methods[mi].code_offset == 0;
        if is_native {
            handler.dispatch(
                runtime_class,
                name_str,
                desc_str,
                args_ref,
                strings,
                objects,
                arrays,
            )
        } else {
            super::execute(classes, strings, objects, arrays, handler, ci, mi, args_ref)
        }
    } else {
        handler.dispatch(
            runtime_class,
            name_str,
            desc_str,
            args_ref,
            strings,
            objects,
            arrays,
        )
    }
}

/// Returns a &'static str for a class name. For user-defined classes (loaded into `classes`),
/// returns the Flash-backed name directly. Falls back to a hardcoded list for native classes.
pub(super) fn class_name_to_static_in(classes: &[ClassFile], name: &str) -> &'static str {
    // Check loaded user classes first — their names are Flash-backed (&'static [u8])
    for cf in classes.iter() {
        if let Some(cn) = cf.class_name() {
            if cn == name.as_bytes() {
                if let Ok(s) = core::str::from_utf8(cn) {
                    return s;
                }
            }
        }
    }
    // Fallback for native/framework classes
    match name {
        "picodroid/pio/Gpio" => "picodroid/pio/Gpio",
        "picodroid/pio/PeripheralManager" => "picodroid/pio/PeripheralManager",
        "picodroid/os/SystemClock" => "picodroid/os/SystemClock",
        "picodroid/util/Log" => "picodroid/util/Log",
        "java/lang/StringBuilder" => "java/lang/StringBuilder",
        _ => "unknown",
    }
}
