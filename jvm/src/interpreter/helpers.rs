use crate::{
    class_file::ClassFile,
    heap::StringTable,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

/// Cache entry: (class_name ptr, method_name ptr, descriptor ptr) → (class_idx, method_idx).
pub(super) type MethodCacheEntry = (*const u8, *const u8, *const u8, usize, usize);

/// Cached field_slot: uses pointer identity on the Flash-backed class/field name slices.
pub(super) fn field_slot_cached(
    cache: &mut Vec<(*const u8, *const u8, usize)>,
    classes: &[ClassFile],
    class_name: &'static str,
    field_name: &[u8],
) -> Option<usize> {
    let cn_ptr = class_name.as_ptr();
    let fn_ptr = field_name.as_ptr();
    for &(cp, fp, slot) in cache.iter() {
        if cp == cn_ptr && fp == fn_ptr {
            return Some(slot);
        }
    }
    let slot = field_slot(classes, class_name, core::str::from_utf8(field_name).ok()?)?;
    cache.push((cn_ptr, fn_ptr, slot));
    Some(slot)
}

pub(super) fn find_method_cached(
    cache: &mut Vec<MethodCacheEntry>,
    classes: &[ClassFile],
    class_name: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let cn_ptr = class_name.as_ptr();
    let mn_ptr = method_name.as_ptr();
    let dn_ptr = descriptor.as_ptr();
    for &(cp, mp, dp, ci, mi) in cache.iter() {
        if cp == cn_ptr && mp == mn_ptr && dp == dn_ptr {
            return Some((ci, mi));
        }
    }
    let (ci, mi) = find_method(classes, class_name, method_name, descriptor)?;
    cache.push((cn_ptr, mn_ptr, dn_ptr, ci, mi));
    Some((ci, mi))
}

pub(super) fn find_method_virtual_cached(
    cache: &mut Vec<MethodCacheEntry>,
    classes: &[ClassFile],
    runtime_class: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let cn_ptr = runtime_class.as_ptr();
    let mn_ptr = method_name.as_ptr();
    let dn_ptr = descriptor.as_ptr();
    for &(cp, mp, dp, ci, mi) in cache.iter() {
        if cp == cn_ptr && mp == mn_ptr && dp == dn_ptr {
            return Some((ci, mi));
        }
    }
    let (ci, mi) = find_method_virtual(classes, runtime_class, method_name, descriptor)?;
    cache.push((cn_ptr, mn_ptr, dn_ptr, ci, mi));
    Some((ci, mi))
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
        for (mi, m) in cf.methods().iter().enumerate() {
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
            'J' | 'D' => count += 1,
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

/// Number of implicit fields in `java/lang/Enum` (name + ordinal).
const ENUM_IMPLICIT_FIELDS: usize = 2;

/// Computes the runtime field slot for a named field, walking from the root of the hierarchy down.
/// Super-class fields come first (slot 0), then subclass fields.
/// Handles `java/lang/Enum` as a native superclass with 2 implicit fields (name, ordinal).
pub(super) fn field_slot(
    classes: &[ClassFile],
    class_name: &str,
    field_name: &str,
) -> Option<usize> {
    // Build a chain of class indices from root to leaf (root first).
    // Track whether the chain bottoms out at java/lang/Enum (a native class
    // not in the loaded class set) so we can account for its implicit fields.
    let mut chain: Vec<usize> = Vec::new();
    let mut enum_base = false;
    let mut current: &str = class_name;
    loop {
        let ci = match classes
            .iter()
            .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()))
        {
            Some(i) => i,
            None => {
                // Not in loaded classes — check if it's java/lang/Enum
                if current == "java/lang/Enum" {
                    enum_base = true;
                }
                break;
            }
        };
        chain.push(ci);
        match classes[ci].super_class_name() {
            None => break, // reached java/lang/Object
            Some(super_bytes) => {
                let super_str: &'static str = core::str::from_utf8(super_bytes).ok()?;
                current = super_str;
            }
        }
    }
    chain.reverse(); // root first

    // Start slot count after Enum's implicit fields if applicable.
    let mut slot = if enum_base { ENUM_IMPLICIT_FIELDS } else { 0 };
    for ci in chain.iter() {
        let cf = &classes[*ci];
        for fi in 0..cf.fields().len() {
            if cf.field_name(fi)? == field_name.as_bytes() {
                return Some(slot);
            }
            slot += 1;
        }
    }
    None
}

/// Returns true if `runtime_class` is the same as, a subclass of, or
/// implements `target_class` (checked at each level of the superclass chain).
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
        // Check implemented interfaces at this level
        let cf = &classes[ci];
        for iface_idx in cf.interfaces() {
            if let Some(iface_name) = cf.cp_utf8(*iface_idx) {
                if iface_name == target_class.as_bytes() {
                    return true;
                }
            }
        }
        match cf.super_class_name() {
            None => return false,
            Some(super_bytes) => match core::str::from_utf8(super_bytes) {
                Ok(s) => current = s,
                Err(_) => return false,
            },
        }
    }
}

/// Find the `<clinit>` method in the given class (by raw class name bytes).
pub(super) fn find_clinit(classes: &[ClassFile], class_name: &[u8]) -> Option<(usize, usize)> {
    for (ci, cf) in classes.iter().enumerate() {
        if cf.class_name() != Some(class_name) {
            continue;
        }
        for (mi, m) in cf.methods().iter().enumerate() {
            if let Some(mn) = cf.cp_utf8(m.name_index) {
                if mn == b"<clinit>" {
                    return Some((ci, mi));
                }
            }
        }
    }
    None
}

/// Build the superclass chain for `class_name`, root-first.
/// Only includes classes present in the loaded `classes` set.
pub(super) fn superclass_chain(classes: &[ClassFile], class_name: &[u8]) -> Vec<&'static [u8]> {
    let mut chain: Vec<&'static [u8]> = Vec::new();
    // Find the Flash-backed &'static [u8] for the initial class name.
    let mut current: Option<&'static [u8]> = classes
        .iter()
        .find(|cf| cf.class_name() == Some(class_name))
        .and_then(|cf| cf.class_name());
    while let Some(name) = current {
        chain.push(name);
        let super_name = classes
            .iter()
            .find(|cf| cf.class_name() == Some(name))
            .and_then(|cf| cf.super_class_name());
        // Only follow superclasses that are in our loaded class set.
        current = super_name.and_then(|sn| {
            classes
                .iter()
                .find(|cf| cf.class_name() == Some(sn))
                .and_then(|cf| cf.class_name())
        });
    }
    chain.reverse(); // root-first
    chain
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

/// Extract the class name from the return type of a method descriptor.
/// e.g. `"()Ljava/lang/Runnable;"` → `Some("java/lang/Runnable")`.
pub(super) fn descriptor_return_class(desc: &str) -> Option<&str> {
    let ret_start = desc.find(')')? + 1;
    let rest = &desc[ret_start..];
    if rest.starts_with('L') && rest.ends_with(';') {
        Some(&rest[1..rest.len() - 1])
    } else {
        None
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
        "java/lang/System" => "java/lang/System",
        "java/lang/StringBuilder" => "java/lang/StringBuilder",
        "java/lang/Integer" => "java/lang/Integer",
        "java/lang/Boolean" => "java/lang/Boolean",
        "java/lang/Long" => "java/lang/Long",
        "java/lang/Float" => "java/lang/Float",
        "java/lang/Double" => "java/lang/Double",
        "java/lang/Enum" => "java/lang/Enum",
        "java/util/ArrayList" => "java/util/ArrayList",
        "java/util/HashMap" => "java/util/HashMap",
        "java/util/HashMap$KeySet" => "java/util/HashMap$KeySet",
        "java/util/HashMap$Values" => "java/util/HashMap$Values",
        "java/util/HashSet" => "java/util/HashSet",
        "java/util/Iterator" => "java/util/Iterator",
        "java/util/Random" => "java/util/Random",
        "java/util/Arrays" => "java/util/Arrays",
        "java/lang/Runnable" => "java/lang/Runnable",
        "picodroid/concurrent/Executor" => "picodroid/concurrent/Executor",
        "picodroid/concurrent/Executors" => "picodroid/concurrent/Executors",
        "picodroid/concurrent/MainExecutor" => "picodroid/concurrent/MainExecutor",
        "picodroid/concurrent/BackgroundExecutor" => "picodroid/concurrent/BackgroundExecutor",
        "picodroid/app/Application" => "picodroid/app/Application",
        "picodroid/view/View" => "picodroid/view/View",
        "picodroid/view/MotionEvent" => "picodroid/view/MotionEvent",
        "picodroid/view/KeyEvent" => "picodroid/view/KeyEvent",
        "picodroid/view/OnKeyListener" => "picodroid/view/OnKeyListener",
        "picodroid/graphics/Display" => "picodroid/graphics/Display",
        "picodroid/widget/TextView" => "picodroid/widget/TextView",
        "picodroid/widget/Button" => "picodroid/widget/Button",
        "picodroid/widget/LinearLayout" => "picodroid/widget/LinearLayout",
        "picodroid/widget/ProgressBar" => "picodroid/widget/ProgressBar",
        "picodroid/widget/Switch" => "picodroid/widget/Switch",
        "picodroid/widget/ListView" => "picodroid/widget/ListView",
        "picodroid/widget/ImageView" => "picodroid/widget/ImageView",
        "picodroid/widget/ToggleButton" => "picodroid/widget/ToggleButton",
        "picodroid/widget/SeekBar" => "picodroid/widget/SeekBar",
        "picodroid/widget/CheckBox" => "picodroid/widget/CheckBox",
        "picodroid/widget/ScrollView" => "picodroid/widget/ScrollView",
        "picodroid/widget/FrameLayout" => "picodroid/widget/FrameLayout",
        "picodroid/widget/Spinner" => "picodroid/widget/Spinner",
        "picodroid/widget/EditText" => "picodroid/widget/EditText",
        "picodroid/net/Socket" => "picodroid/net/Socket",
        "picodroid/net/ServerSocket" => "picodroid/net/ServerSocket",
        "picodroid/net/DatagramSocket" => "picodroid/net/DatagramSocket",
        "picodroid/net/DatagramPacket" => "picodroid/net/DatagramPacket",
        "picodroid/net/InetAddress" => "picodroid/net/InetAddress",
        "picodroid/net/NetworkInfo" => "picodroid/net/NetworkInfo",
        "picodroid/net/Url" => "picodroid/net/Url",
        "picodroid/net/HttpUrlConnection" => "picodroid/net/HttpUrlConnection",
        "picodroid/net/HttpInputStream" => "picodroid/net/HttpInputStream",
        "picodroid/net/HttpOutputStream" => "picodroid/net/HttpOutputStream",
        "picodroid/io/File" => "picodroid/io/File",
        "picodroid/io/FileInputStream" => "picodroid/io/FileInputStream",
        "picodroid/io/FileOutputStream" => "picodroid/io/FileOutputStream",
        _ => "unknown",
    }
}
