// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    class_file::ClassFile,
    class_objects::ClassObjectCache,
    heap::StringTable,
    object_heap::ObjectHeap,
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
    // JVMS §5.4.3.3: method resolution recurses into the superclass when the named
    // class doesn't declare a matching method. Used by invokestatic and invokespecial.
    let (ci, mi) = find_method_walking(classes, class_name, method_name, descriptor)?;
    cache.push((cn_ptr, mn_ptr, dn_ptr, ci, mi));
    Some((ci, mi))
}

pub(super) fn find_method_walking_cached(
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
    let (ci, mi) = find_method_walking(classes, runtime_class, method_name, descriptor)?;
    cache.push((cn_ptr, mn_ptr, dn_ptr, ci, mi));
    Some((ci, mi))
}

pub(super) fn resolve_ldc(
    cf: &ClassFile,
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    class_objects: &mut ClassObjectCache,
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
    if let Some(name_bytes) = cf.cp_class_name(cp_idx) {
        return resolve_class_literal(classes, strings, objects, class_objects, name_bytes);
    }
    Err(JvmError::InvalidBytecode)
}

/// Resolve a `CONSTANT_Class` reference to its cached `java.lang.Class`
/// instance, allocating one on the first sighting. Identity is guaranteed:
/// every `ldc` for the same class name returns the same `ObjectRef`,
/// regardless of which class file's CP the request came from.
fn resolve_class_literal(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    class_objects: &mut ClassObjectCache,
    name_bytes: &'static [u8],
) -> Result<Value, JvmError> {
    // The class must be loaded so getName() can read back a stable name and
    // bytecode that follows (e.g. checkcast, instanceof) can resolve it.
    classes
        .iter()
        .find(|c| c.class_name() == Some(name_bytes))
        .ok_or(JvmError::ClassNotFound)?;
    class_object_for_name(classes, strings, objects, class_objects, name_bytes)
}

/// Return the canonical `java.lang.Class` instance for `name_bytes`,
/// allocating and caching on first sighting. Shared by `ldc CONSTANT_Class`
/// (which additionally requires the class to be loaded) and
/// `Object.getClass()` (whose receiver may be a builtin like
/// `java/util/ArrayList` with no class file) — both must hand out the same
/// `ObjectRef` so `obj.getClass() == MyClass.class` holds.
pub(super) fn class_object_for_name(
    classes: &[ClassFile],
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    class_objects: &mut ClassObjectCache,
    name_bytes: &'static [u8],
) -> Result<Value, JvmError> {
    // Intern the name once — `StringTable::intern` deduplicates by content,
    // so the index is canonical across all class files and threads.
    let name_idx = strings.intern(name_bytes).ok_or(JvmError::StackOverflow)?;
    if let Some(obj) = class_objects.lookup(name_idx) {
        return Ok(Value::ObjectRef(obj));
    }
    let obj = objects
        .alloc_with_defaults("java/lang/Class", classes)
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, 0, Value::Reference(name_idx))
        .ok_or(JvmError::InvalidReference)?;
    class_objects.insert(name_idx, obj);
    Ok(Value::ObjectRef(obj))
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

/// Superclass edges for classfile-less builtin classes — the `java.lang`
/// throwable hierarchy for catch-matching, and the builtin value/collection
/// classes so `checkcast`/`instanceof` against `Object`, `Number`, … hold.
/// Without the throwable rows, `catch (Throwable)` / `catch (Exception)`
/// never matched a thrown `RuntimeException` (or any user exception whose
/// super chain bottoms out in a builtin), which silently disabled javac's
/// synthetic try-with-resources cleanup and every catch-all handler.
///
/// Every class named here (key or value) must also be in
/// [`crate::native::BUILTIN_CLASS_NAMES`] so a `new` of it canonicalises
/// instead of producing an `"unknown"` object that no catch clause matches
/// — the `builtin_hierarchy_names_are_registered` test enforces it.
pub(crate) const BUILTIN_SUPER: &[(&str, &str)] = &[
    ("java/lang/Throwable", "java/lang/Object"),
    ("java/lang/Exception", "java/lang/Throwable"),
    ("java/lang/Error", "java/lang/Throwable"),
    ("java/lang/RuntimeException", "java/lang/Exception"),
    (
        "java/lang/IllegalArgumentException",
        "java/lang/RuntimeException",
    ),
    (
        "java/lang/NullPointerException",
        "java/lang/RuntimeException",
    ),
    (
        "java/lang/IllegalStateException",
        "java/lang/RuntimeException",
    ),
    (
        "java/lang/ArithmeticException",
        "java/lang/RuntimeException",
    ),
    ("java/lang/ClassCastException", "java/lang/RuntimeException"),
    (
        "java/lang/UnsupportedOperationException",
        "java/lang/RuntimeException",
    ),
    (
        "java/lang/IndexOutOfBoundsException",
        "java/lang/RuntimeException",
    ),
    (
        "java/util/NoSuchElementException",
        "java/lang/RuntimeException",
    ),
    (
        "java/lang/NumberFormatException",
        "java/lang/IllegalArgumentException",
    ),
    (
        "java/util/IllegalFormatException",
        "java/lang/IllegalArgumentException",
    ),
    // Checked exceptions thrown alloc-by-name from natives (net stack).
    // Mirrors the real java.net hierarchy so superclass catches behave
    // exactly as on Android — note SocketTimeoutException descends from
    // InterruptedIOException, NOT SocketException (real-Java quirk).
    ("java/io/IOException", "java/lang/Exception"),
    ("java/io/InterruptedIOException", "java/io/IOException"),
    (
        "java/net/SocketTimeoutException",
        "java/io/InterruptedIOException",
    ),
    ("java/net/SocketException", "java/io/IOException"),
    ("java/net/ConnectException", "java/net/SocketException"),
    (
        "java/net/NoRouteToHostException",
        "java/net/SocketException",
    ),
    ("java/net/BindException", "java/net/SocketException"),
    ("java/net/UnknownHostException", "java/io/IOException"),
    ("java/net/ProtocolException", "java/io/IOException"),
    (
        "java/lang/ArrayIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
    ),
    (
        "java/lang/StringIndexOutOfBoundsException",
        "java/lang/IndexOutOfBoundsException",
    ),
    ("java/lang/ExceptionInInitializerError", "java/lang/Error"),
    ("java/lang/StackOverflowError", "java/lang/Error"),
    // Boxed numerics descend from Number, as Kotlin's `checkcast
    // java/lang/Number` before every `intValue()` unboxing of a generic
    // element requires. No `X → java/lang/Object` rows: `is_instance_of`
    // answers `Object` up front and `dispatch_native` falls through to
    // Object for any class without a row.
    ("java/lang/Integer", "java/lang/Number"),
    ("java/lang/Long", "java/lang/Number"),
    ("java/lang/Float", "java/lang/Number"),
    ("java/lang/Double", "java/lang/Number"),
    ("java/lang/Short", "java/lang/Number"),
    ("java/lang/Byte", "java/lang/Number"),
];

/// Interfaces implemented by classfile-less builtin classes, flattened to
/// the transitive closure, plus superinterface edges for the JDK interfaces
/// that have no class file of their own (a user class implementing
/// `java/util/List` is a `Collection` and an `Iterable`). Consulted by
/// [`is_instance_of`] at every level of the superclass chain and of the
/// interface walk. Same registration rule as [`BUILTIN_SUPER`].
pub(crate) const BUILTIN_INTERFACES: &[(&str, &[&str])] = &[
    (
        "java/util/ArrayList",
        &[
            "java/util/List",
            "java/util/Collection",
            "java/lang/Iterable",
        ],
    ),
    ("java/util/HashMap", &["java/util/Map"]),
    (
        "java/util/HashSet",
        &[
            "java/util/Set",
            "java/util/Collection",
            "java/lang/Iterable",
        ],
    ),
    (
        "java/util/HashMap$KeySet",
        &[
            "java/util/Set",
            "java/util/Collection",
            "java/lang/Iterable",
        ],
    ),
    (
        "java/util/HashMap$Values",
        &["java/util/Collection", "java/lang/Iterable"],
    ),
    (
        "java/lang/String",
        &["java/lang/CharSequence", "java/lang/Comparable"],
    ),
    ("java/lang/StringBuilder", &["java/lang/CharSequence"]),
    ("java/lang/Integer", &["java/lang/Comparable"]),
    ("java/lang/Long", &["java/lang/Comparable"]),
    ("java/lang/Float", &["java/lang/Comparable"]),
    ("java/lang/Double", &["java/lang/Comparable"]),
    ("java/lang/Short", &["java/lang/Comparable"]),
    ("java/lang/Byte", &["java/lang/Comparable"]),
    ("java/lang/Boolean", &["java/lang/Comparable"]),
    ("java/lang/Character", &["java/lang/Comparable"]),
    ("java/lang/Enum", &["java/lang/Comparable"]),
    (
        "java/util/List",
        &["java/util/Collection", "java/lang/Iterable"],
    ),
    (
        "java/util/Set",
        &["java/util/Collection", "java/lang/Iterable"],
    ),
    ("java/util/Collection", &["java/lang/Iterable"]),
];

/// Linear scan of a name-keyed table. Opaque to the optimiser: with the
/// `const` table visible LLVM unrolls the scan into one constant-length
/// `memcmp` per row (~600 B for the interface table on thumbv6m).
#[inline(never)]
fn table_lookup<V: Copy>(table: &[(&str, V)], name: &str) -> Option<V> {
    // black_box: each monomorphisation has one caller, so without it LLVM
    // propagates the constant table into the body and unrolls anyway.
    let table = core::hint::black_box(table);
    table.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

/// Superclass of a classfile-less builtin class, from [`BUILTIN_SUPER`].
pub(super) fn builtin_super(name: &str) -> Option<&'static str> {
    table_lookup(BUILTIN_SUPER, name)
}

/// Interfaces of a classfile-less builtin class (or superinterfaces of a
/// classfile-less JDK interface), from [`BUILTIN_INTERFACES`].
fn builtin_interfaces(name: &str) -> &'static [&'static str] {
    table_lookup(BUILTIN_INTERFACES, name).unwrap_or(&[])
}

/// Bound on the superinterface recursion in [`iface_reaches`]. Valid class
/// files cannot cycle, but a hand-assembled one can; real hierarchies are
/// two or three deep.
const MAX_IFACE_DEPTH: u8 = 8;

/// Returns true if interface `iface` is `target` or extends it, walking
/// superinterfaces transitively through loaded interface class files and
/// [`BUILTIN_INTERFACES`]. Interfaces with no class file and no table row
/// (`kotlin/jvm/internal/markers/KMappedMarker`) simply end the walk.
fn iface_reaches(classes: &[ClassFile], iface: &[u8], target: &[u8], depth: u8) -> bool {
    if iface == target {
        return true;
    }
    if depth == 0 {
        return false;
    }
    if let Ok(name) = core::str::from_utf8(iface) {
        if builtin_interfaces(name)
            .iter()
            .any(|i| i.as_bytes() == target)
        {
            return true;
        }
    }
    let Some(cf) = classes.iter().find(|cf| cf.class_name() == Some(iface)) else {
        return false;
    };
    cf.interfaces().iter().any(|&idx| {
        cf.cp_utf8(idx)
            .is_some_and(|sup| iface_reaches(classes, sup, target, depth - 1))
    })
}

/// Returns true if `runtime_class` is the same as, a subclass of, or
/// implements `target_class` (checked at each level of the superclass chain,
/// with superinterfaces walked transitively at each level).
pub(super) fn is_instance_of(
    classes: &[ClassFile],
    runtime_class: &str,
    target_class: &str,
) -> bool {
    // Every reference is an Object — including lambda proxies and
    // handler-allocated objects whose class has neither a class file nor a
    // table row.
    if target_class == "java/lang/Object" {
        return true;
    }
    let mut current: &str = runtime_class;
    loop {
        if current == target_class {
            return true;
        }
        if builtin_interfaces(current).contains(&target_class) {
            return true;
        }
        let ci = match classes
            .iter()
            .position(|cf| cf.class_name().is_some_and(|n| n == current.as_bytes()))
        {
            Some(i) => i,
            None => {
                // No classfile — follow the builtin hierarchy.
                match builtin_super(current) {
                    Some(s) => {
                        current = s;
                        continue;
                    }
                    None => return false,
                }
            }
        };
        // Check implemented interfaces at this level, transitively.
        let cf = &classes[ci];
        for iface_idx in cf.interfaces() {
            if let Some(iface_name) = cf.cp_utf8(*iface_idx) {
                if iface_reaches(
                    classes,
                    iface_name,
                    target_class.as_bytes(),
                    MAX_IFACE_DEPTH,
                ) {
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

/// JVM class name of an array by element type: `[I`, `[F`, … for primitive
/// arrays; `[Ljava/lang/Object;` for every reference array, because the
/// array heap records no element class (see [`value_is_instance`]).
pub(crate) fn array_class_name(atype: u8) -> &'static str {
    use crate::array_heap::*;
    match atype {
        ATYPE_BOOLEAN => "[Z",
        ATYPE_CHAR => "[C",
        ATYPE_FLOAT => "[F",
        ATYPE_DOUBLE => "[D",
        ATYPE_BYTE => "[B",
        ATYPE_SHORT => "[S",
        ATYPE_INT => "[I",
        ATYPE_LONG => "[J",
        _ => "[Ljava/lang/Object;",
    }
}

/// `instanceof` for any operand-stack value, as `checkcast`/`instanceof`
/// need it. `Null` is an instance of nothing here (checkcast handles null
/// itself). A string Reference is a `java/lang/String`; an array is an
/// `Object`/`Cloneable`, its exact primitive array class, or
/// — for reference arrays, whose element class is not recorded — any
/// reference-array target (`[L…;` / `[[…`): a documented divergence, the
/// cast succeeds where Java might throw.
pub(super) fn value_is_instance(
    classes: &[ClassFile],
    objects: &ObjectHeap,
    arrays: &crate::array_heap::ArrayHeap,
    value: Value,
    target: &str,
) -> bool {
    match value {
        Value::ObjectRef(idx) => {
            let runtime_class = objects.class_name(idx).unwrap_or("");
            is_instance_of(classes, runtime_class, target)
        }
        Value::Reference(_) => is_instance_of(classes, "java/lang/String", target),
        Value::ArrayRef(idx) => match target.as_bytes().first() {
            Some(b'[') => {
                let atype = arrays.atype(idx).unwrap_or(crate::array_heap::ATYPE_REF);
                if atype == crate::array_heap::ATYPE_REF {
                    matches!(target.as_bytes().get(1), Some(b'L') | Some(b'['))
                } else {
                    array_class_name(atype) == target
                }
            }
            _ => matches!(target, "java/lang/Object" | "java/lang/Cloneable"),
        },
        _ => false,
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

/// JVMS §5.4.3.3 method resolution: find a method starting from `start_class`, walking up the
/// superclass chain. Used by invokevirtual / invokeinterface (starting from the receiver's runtime
/// class) AND by invokestatic / invokespecial (starting from the CP-declared class) — both forms
/// of dispatch recurse to the superclass when the named class doesn't declare the method.
pub(super) fn find_method_walking(
    classes: &[ClassFile],
    start_class: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let mut current: &str = start_class;
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

/// Returns a `&'static str` for a class name.
///
/// Checks, in order:
/// 1. Loaded user classes — their names are Flash-backed (`&'static [u8]`)
/// 2. JVM builtins ([`crate::native::BUILTIN_CLASS_NAMES`])
/// 3. The host application's native classes (passed in via the
///    [`crate::native::NativeMethodHandler::native_class_names`] trait method)
///
/// Falls back to `"unknown"` if no match. A class missing from all three lists
/// will silently lose virtual dispatch through pointer-identity caching, so
/// every native class the JVM might encounter must appear in one of them.
pub(super) fn class_name_to_static_in(
    classes: &[ClassFile],
    extra_native_classes: &[&'static str],
    name: &str,
) -> &'static str {
    // 1. Loaded user classes (Flash-backed)
    for cf in classes.iter() {
        if let Some(cn) = cf.class_name() {
            if cn == name.as_bytes() {
                if let Ok(s) = core::str::from_utf8(cn) {
                    return s;
                }
            }
        }
    }
    // 2. JVM builtins
    for &builtin in crate::native::BUILTIN_CLASS_NAMES {
        if builtin == name {
            return builtin;
        }
    }
    // 3. Host-supplied native classes
    for &extra in extra_native_classes {
        if extra == name {
            return extra;
        }
    }
    "unknown"
}
