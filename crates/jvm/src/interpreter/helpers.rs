// SPDX-License-Identifier: GPL-3.0-only
use crate::array_heap::ArrayHeap;
use crate::names::{c, d, m};
use crate::resolve_cache::{flags, MethodHit, NameRef, ResolveCache, SiteKey, Target};
use crate::{
    class_file::{find_class, find_class_hashed, name_eq, name_hash, ClassFile, Classes},
    class_objects::ClassObjectCache,
    heap::StringTable,
    object_heap::ObjectHeap,
    types::{JvmError, Value},
};
use alloc::vec::Vec;

/// The receiver half of a [`SiteKey`] for `v`: the object's heap class id,
/// or the string / array tag. `RECV_NONE` for anything else (a malformed
/// receiver; the resolve then keys on the site alone and fails like the
/// invoke will).
#[inline]
pub(super) fn recv_of(objects: &ObjectHeap, arrays: &ArrayHeap, v: Value) -> u32 {
    match v {
        Value::ObjectRef(idx) => objects
            .class_id(idx)
            .map_or(crate::resolve_cache::RECV_NONE, SiteKey::recv_object),
        Value::Reference(_) => crate::resolve_cache::RECV_STRING,
        Value::ArrayRef(idx) => {
            SiteKey::recv_array(arrays.atype(idx).unwrap_or(crate::array_heap::ATYPE_REF))
        }
        _ => crate::resolve_cache::RECV_NONE,
    }
}

/// Does method `mi` of class `ci` spell `name` and `desc`? The check a
/// hashed site's hit needs before it is trusted (see [`SiteKey::hashed`]).
pub(super) fn method_matches(
    classes: Classes<'_>,
    ci: usize,
    mi: usize,
    name: &str,
    desc: &str,
) -> bool {
    let Some(cf) = classes.get(ci) else {
        return false;
    };
    let Some(m) = cf.methods().get(mi) else {
        return false;
    };
    cf.method_name(m)
        .is_some_and(|n| name_eq(n, name.as_bytes()))
        && cf
            .method_descriptor(m)
            .is_some_and(|d| name_eq(d, desc.as_bytes()))
}

/// `field_slot_declared` through the persistent field table, keyed by the
/// `Fieldref` site and the receiver's class (see [`crate::resolve_cache`]).
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn field_slot_cached(
    cache: &mut ResolveCache,
    classes: Classes<'_>,
    key: SiteKey,
    class_name: &'static str,
    declared_class: &[u8],
    field_name: &[u8],
) -> Option<usize> {
    if let Some(slot) = cache.field(key) {
        return Some(slot);
    }
    let slot = field_slot_declared(
        classes,
        class_name,
        core::str::from_utf8(declared_class).ok()?,
        core::str::from_utf8(field_name).ok()?,
    )?;
    cache.insert_field(key, slot);
    Some(slot)
}

/// Resolve from the CP-declared class (invokestatic / invokespecial),
/// through the persistent method table.
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn find_method_cached(
    cache: &mut ResolveCache,
    classes: Classes<'_>,
    key: SiteKey,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    site_flags: u8,
) -> MethodHit {
    if let Some(hit) = cache.method(key) {
        return hit;
    }
    // JVMS §5.4.3.3: method resolution recurses into the superclass when the named
    // class doesn't declare a matching method. Used by invokestatic and invokespecial.
    let walked = find_method_walking(classes, class_name, method_name, descriptor);
    let mut hit = resolved_hit(classes, walked, class_name, method_name, descriptor);
    hit.flags |= site_flags;
    cache.insert_target(key, hit.target, hit.flags);
    hit
}

/// The per-site bits a resolution stores besides its target and
/// [`flags::PRECHECK`]: the stringification the interpreter runs before
/// `StringBuilder.append(Object | CharSequence)` / `String.valueOf(Object)`
/// and before `String.format`'s varargs reach the native arm. Decided once,
/// from the `Methodref`'s own class, name and descriptor — exactly what
/// `Executor::stringify_object_arg` and `stringify_format_args` gate on.
pub(super) fn site_flags(class: &str, name: &str, desc: &str) -> u8 {
    let stringify = match (class, name) {
        (c::java_lang_StringBuilder, m::append) => {
            desc == d::Object__StringBuilder || desc == d::CharSequence__StringBuilder
        }
        (c::java_lang_String, m::valueOf) => desc == d::Object__String,
        _ => false,
    };
    let format =
        class == c::java_lang_String && name == m::format && desc == d::String_aObject__String;
    (if stringify { flags::STRINGIFY } else { 0 }) | (if format { flags::FORMAT } else { 0 })
}

/// What a walk's answer means for the site: bytecode to push, or — no
/// method at all, or one without a `Code` attribute — a native target,
/// with the [`flags::PRECHECK`] bit worked out once here rather than by
/// string compares on every call. `dispatch_class` is the class the
/// native will be dispatched under (the receiver's, for a virtual site).
fn resolved_hit(
    classes: Classes<'_>,
    walked: Option<(usize, usize)>,
    dispatch_class: &str,
    method_name: &str,
    descriptor: &str,
) -> MethodHit {
    match walked {
        Some((ci, mi)) if classes[ci].methods()[mi].code_offset != 0 => MethodHit {
            target: Target::Java { ci, mi },
            flags: 0,
            init: false,
        },
        _ => MethodHit {
            target: Target::Native { hint: None },
            flags: precheck_flag(dispatch_class, method_name, descriptor),
            init: false,
        },
    }
}

/// [`flags::PRECHECK`] when `dispatch_native` must run the interpreter's
/// own checks before the handlers see the call — the operations
/// `Executor::dispatch_native_inner` serves itself.
fn precheck_flag(class: &str, name: &str, desc: &str) -> u8 {
    let map = class == c::java_util_HashMap || class == c::java_util_LinkedHashMap;
    let set = class == c::java_util_HashSet || class == c::java_util_LinkedHashSet;
    let precheck = (map
        && matches!(
            name,
            m::get | m::getOrDefault | m::containsKey | m::remove | m::put
        ))
        || (set && matches!(name, m::add | m::contains | m::remove))
        || (class == c::java_util_ArrayList && matches!(name, m::contains | m::remove | m::sort))
        || (class == c::java_lang_Enum && name == m::valueOf)
        || (name == m::getClass && desc == d::__Class)
        || (class == c::java_lang_Class && matches!(name, m::forName | m::newInstance))
        || (class == c::picodroid_view_LayoutInflater && name == m::nativeNewView);
    if precheck {
        flags::PRECHECK
    } else {
        0
    }
}

/// Resolve from the receiver's runtime class (invokevirtual /
/// invokeinterface), through the persistent method table. A hashed key
/// (a native upcall's) trusts a hit only after [`method_matches`].
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn find_method_walking_cached(
    cache: &mut ResolveCache,
    classes: Classes<'_>,
    key: SiteKey,
    runtime_class: &str,
    method_name: &str,
    descriptor: &str,
    site_flags: u8,
) -> MethodHit {
    if let Some(hit) = cache.method(key) {
        match hit.java() {
            Some((ci, mi)) if key.is_hashed() => {
                if method_matches(classes, ci, mi, method_name, descriptor) {
                    return hit;
                }
            }
            _ => return hit,
        }
    }
    let walked = find_method_walking(classes, runtime_class, method_name, descriptor);
    let mut hit = resolved_hit(classes, walked, runtime_class, method_name, descriptor);
    hit.flags |= site_flags;
    // A hashed key's hit is trusted only after `method_matches`, which a
    // native target has no `(ci, mi)` for: never remember one under a hash.
    if !key.is_hashed() || hit.java().is_some() {
        cache.insert_target(key, hit.target, hit.flags);
    }
    hit
}

pub(super) fn resolve_ldc(
    cf: &ClassFile,
    classes: Classes<'_>,
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    class_objects: &mut ClassObjectCache,
    cp_idx: u16,
) -> Result<Value, JvmError> {
    if let Some(lit) = cf.cp_string_literal(cp_idx) {
        #[cfg(feature = "parity-metrics")]
        crate::parity::count_ldc_string();
        // A constant of a packed set is a row of its section's literal
        // pool: the reference is the pool's base plus the row.
        if let Some(r) = strings.literal_ref(cf.data(), lit) {
            return Ok(Value::Reference(r));
        }
        // A class linked on its own (tests, `link-at-load`) has no pool.
        let utf8 = cf.cp_string_utf8(cp_idx).ok_or(JvmError::InvalidBytecode)?;
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
    classes: Classes<'_>,
    strings: &mut StringTable,
    objects: &mut ObjectHeap,
    class_objects: &mut ClassObjectCache,
    name_bytes: &'static [u8],
) -> Result<Value, JvmError> {
    // The name must be one the JVM can hand back out: a loaded class, or a
    // builtin the interpreter already canonicalises (`String.class`,
    // `Runnable.class` — classfile-less by design, see `BUILTIN_CLASS_NAMES`).
    // Anything else would give getName() an unstable name and leave a
    // following checkcast/instanceof unresolvable.
    let loaded = find_class(classes, name_bytes).is_some();
    if !loaded {
        let is_builtin = core::str::from_utf8(name_bytes)
            .is_ok_and(|n| crate::native::BUILTIN_CLASS_NAMES.contains(&n));
        if !is_builtin {
            return Err(JvmError::ClassNotFound);
        }
    }
    class_object_for_name(classes, strings, objects, class_objects, name_bytes)
}

/// Return the canonical `java.lang.Class` instance for `name_bytes`,
/// allocating and caching on first sighting. Shared by `ldc CONSTANT_Class`
/// (which additionally requires the class to be loaded) and
/// `Object.getClass()` (whose receiver may be a builtin like
/// `java/util/ArrayList` with no class file) — both must hand out the same
/// `ObjectRef` so `obj.getClass() == MyClass.class` holds.
pub(super) fn class_object_for_name(
    classes: Classes<'_>,
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
        .alloc_with_defaults(c::java_lang_Class, classes)
        .ok_or(JvmError::StackOverflow)?;
    objects
        .set_field(obj, 0, Value::Reference(name_idx))
        .ok_or(JvmError::InvalidReference)?;
    class_objects.insert(name_idx, obj);
    Ok(Value::ObjectRef(obj))
}

#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn find_method(
    classes: Classes<'_>,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let ci = find_class(classes, class_name.as_bytes())?;
    find_method_in(classes, ci, method_name, descriptor).map(|mi| (ci, mi))
}

/// The method of class `ci` spelling `method_name`/`descriptor`: one `u32`
/// compare per method against the signature hash its link table stores,
/// and the name and descriptor bytes read only on a hash match. A miss
/// walks every method of every class on the chain (a `View` has ~150),
/// which used to be a flash read and a byte compare each.
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn find_method_in(
    classes: Classes<'_>,
    ci: usize,
    method_name: &str,
    descriptor: &str,
) -> Option<usize> {
    let cf = &classes[ci];
    let sig = class_link::sig_hash(method_name.as_bytes(), descriptor.as_bytes());
    for (mi, m) in cf.methods().iter().enumerate() {
        if m.sig_hash() != sig {
            continue;
        }
        if cf
            .method_name(m)
            .is_some_and(|n| name_eq(n, method_name.as_bytes()))
            && cf
                .method_descriptor(m)
                .is_some_and(|d| name_eq(d, descriptor.as_bytes()))
        {
            return Some(mi);
        }
    }
    None
}

/// The class `ci`'s superclass, by its table's hash: `None` for a class
/// with no superclass in the set (Object, a builtin parent, or a parent
/// this board's framework excludes).
#[inline]
pub(super) fn super_index(classes: Classes<'_>, ci: usize) -> Option<usize> {
    classes.super_of(ci)
}

/// Number of parameters in `descriptor` — one per value, whatever its
/// width: the operand stack holds one `Value` per parameter (a `long` is
/// one 16 B entry, not two slots), so this is what an invoke pops.
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
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
            '[' => {
                // An array is one reference whatever its element type: skip
                // the dimensions and the element descriptor.
                let mut elem = chars.next();
                while elem == Some('[') {
                    elem = chars.next();
                }
                if elem == Some('L') {
                    for c2 in chars.by_ref() {
                        if c2 == ';' {
                            break;
                        }
                    }
                }
                count += 1;
            }
            _ => count += 1,
        }
    }
    count
}

/// One byte per parameter of a method descriptor: the primitive letter
/// (`I`, `J`, `F`, `D`, `Z`, `B`, `S`, `C`), or `b'L'` for any reference
/// (object or array). Stops at `)`.
pub(super) struct ParamKinds<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> ParamKinds<'a> {
    pub(super) fn new(desc: &'a [u8]) -> Self {
        Self { bytes: desc, i: 0 }
    }
}

impl Iterator for ParamKinds<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        loop {
            let b = *self.bytes.get(self.i)?;
            self.i += 1;
            match b {
                b'(' => continue,
                b')' => return None,
                b'[' | b'L' => {
                    // Skip the rest of the type: further `[`s, then either a
                    // single primitive letter or `L…;`.
                    let mut c = b;
                    while c == b'[' {
                        c = *self.bytes.get(self.i)?;
                        self.i += 1;
                    }
                    if c == b'L' {
                        while *self.bytes.get(self.i)? != b';' {
                            self.i += 1;
                        }
                        self.i += 1;
                    }
                    return Some(b'L');
                }
                p => return Some(p),
            }
        }
    }
}

/// Return kind of a method descriptor: the primitive letter, `b'V'`, or
/// `b'L'` for any reference.
pub(super) fn return_kind(desc: &[u8]) -> u8 {
    let i = desc.iter().position(|&c| c == b')').map_or(0, |i| i + 1);
    match desc.get(i).copied().unwrap_or(b'V') {
        b'[' | b'L' => b'L',
        k => k,
    }
}

/// Box a primitive `Value` as the wrapper for descriptor letter `kind`
/// (`I` → `java/lang/Integer`, …): the box's field 0 holds the raw value, as
/// `Integer.valueOf` and `op_new` + `<init>` lay it out. `None` on OOM.
pub(super) fn box_primitive(objects: &mut ObjectHeap, kind: u8, v: Value) -> Option<Value> {
    let class = match kind {
        b'I' => c::java_lang_Integer,
        b'J' => c::java_lang_Long,
        b'F' => c::java_lang_Float,
        b'D' => c::java_lang_Double,
        b'Z' => c::java_lang_Boolean,
        b'C' => c::java_lang_Character,
        b'B' => c::java_lang_Byte,
        b'S' => c::java_lang_Short,
        _ => return Some(v),
    };
    // Same identity contract as `Integer.valueOf`: the JLS-cached range is
    // one shared box per value.
    if let Some(idx) = objects.cached_box(class, v) {
        return Some(Value::ObjectRef(idx));
    }
    let idx = objects.alloc(class)?;
    objects.set_field(idx, 0, v);
    objects.cache_box(class, v, idx);
    Some(Value::ObjectRef(idx))
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
/// Super-class fields come first (slot 0), then subclass fields; a `long` or
/// `double` field takes two slots, so the field after it starts one higher.
/// Handles `java/lang/Enum` as a native superclass with 2 implicit fields (name, ordinal).
/// Name-only resolution (declared class = runtime class); the interpreter
/// itself resolves through [`field_slot_declared`] with the Fieldref's class.
/// Public so the hand-numbered native field tables in `picodroid-core` can
/// be checked against the class files they mirror.
pub fn field_slot(classes: Classes<'_>, class_name: &str, field_name: &str) -> Option<usize> {
    field_slot_declared(classes, class_name, class_name, field_name)
}

/// Slots an instance of `class_name` occupies: the sum of its own and its
/// superclasses' instance field widths — what `alloc_with_defaults` sizes
/// and what a native `alloc_with_field_count` must pass. `None` for a class
/// that is not loaded.
pub fn instance_slot_count(classes: Classes<'_>, class_name: &str) -> Option<usize> {
    let mut total = 0;
    let mut ci = find_class(classes, class_name.as_bytes())?;
    loop {
        let cf = &classes[ci];
        total += cf
            .fields()
            .iter()
            .map(|fi| {
                cf.field_descriptor(fi)
                    .map_or(1, Value::descriptor_slot_width)
            })
            .sum::<usize>();
        let Some(sup) = cf.super_class_name() else {
            return Some(total);
        };
        match find_class_hashed(classes, cf.super_hash(), sup) {
            Some(next) => ci = next,
            None => {
                // A parent with no class file: `java/lang/Enum` carries two
                // implicit fields (name, ordinal); any other ends the walk.
                if sup == c::java_lang_Enum.as_bytes() {
                    total += ENUM_IMPLICIT_FIELDS;
                }
                return Some(total);
            }
        }
    }
}

/// [`field_slot`], honouring the `Fieldref`'s declaring class: JVMS §5.4.3.2
/// resolves a field starting at the CP-named class and walking *up*, so when
/// a subclass shadows a super's field (`class A { int x; } class B extends A
/// { int x; }`) the two Fieldrefs address two distinct slots. The old
/// name-only walk returned the root-most match for both — reads and writes
/// through either declaring class aliased A's storage and B's own field was
/// unreachable (bugbash J12).
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
#[cfg_attr(feature = "hot-in-ram", inline(never))]
pub(super) fn field_slot_declared(
    classes: Classes<'_>,
    runtime_class: &str,
    declared_class: &str,
    field_name: &str,
) -> Option<usize> {
    // Resolve which class actually declares the field, from the CP-named
    // class upward. A declaring class outside the loaded set (builtin
    // natives lay their fields out by convention) falls back to the
    // name-only walk below.
    let mut declaring: Option<&str> = None;
    let mut current = declared_class;
    while let Some(ci) = find_class(classes, current.as_bytes()) {
        let cf = &classes[ci];
        let declares = (0..cf.fields().len()).any(|fi| {
            cf.field_name(fi)
                .is_some_and(|n| n == field_name.as_bytes())
        });
        if declares {
            declaring = Some(current);
            break;
        }
        match cf.super_class_name() {
            Some(sup) => current = core::str::from_utf8(sup).ok()?,
            None => break,
        }
    }
    field_slot_in(classes, runtime_class, declaring, field_name)
}

fn field_slot_in(
    classes: Classes<'_>,
    class_name: &str,
    declaring: Option<&str>,
    field_name: &str,
) -> Option<usize> {
    // Build a chain of class indices from root to leaf (root first).
    // Track whether the chain bottoms out at java/lang/Enum (a native class
    // not in the loaded class set) so we can account for its implicit fields.
    let mut chain: Vec<usize> = Vec::new();
    let mut enum_base = false;
    let mut current: &str = class_name;
    loop {
        let ci = match find_class(classes, current.as_bytes()) {
            Some(i) => i,
            None => {
                // Not in loaded classes — check if it's java/lang/Enum
                if current == c::java_lang_Enum {
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
        // With a known declaring class, only its own field table may match;
        // shadowing classes above/below it keep their own slots.
        let this_declares = match declaring {
            Some(d) => cf.class_name().is_some_and(|n| name_eq(n, d.as_bytes())),
            None => true,
        };
        for fi in 0..cf.fields().len() {
            if this_declares && cf.field_name(fi)? == field_name.as_bytes() {
                return Some(slot);
            }
            slot += cf
                .field_descriptor(&cf.fields()[fi])
                .map_or(1, Value::descriptor_slot_width);
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
pub const BUILTIN_SUPER: &[(&str, &str)] = &[
    (c::java_lang_Throwable, c::java_lang_Object),
    (c::java_lang_Exception, c::java_lang_Throwable),
    (c::java_lang_Error, c::java_lang_Throwable),
    (c::java_lang_RuntimeException, c::java_lang_Exception),
    // Thread primitives (picodroid.concurrent.Thread, Object.wait/notify).
    (c::java_lang_InterruptedException, c::java_lang_Exception),
    (
        c::java_lang_IllegalThreadStateException,
        c::java_lang_IllegalArgumentException,
    ),
    (
        c::java_lang_IllegalMonitorStateException,
        c::java_lang_RuntimeException,
    ),
    // Reflection-lite (`Class.forName` / `newInstance`, ops_reflect.rs):
    // the JDK's checked family, so a `catch (ClassNotFoundException e)`
    // written for Android matches here.
    (
        c::java_lang_ReflectiveOperationException,
        c::java_lang_Exception,
    ),
    (
        c::java_lang_ClassNotFoundException,
        c::java_lang_ReflectiveOperationException,
    ),
    (
        c::java_lang_InstantiationException,
        c::java_lang_ReflectiveOperationException,
    ),
    (
        c::java_lang_IllegalAccessException,
        c::java_lang_ReflectiveOperationException,
    ),
    // picodroid.concurrent's ExecutorService/Future (pure Java) throw these
    // by their JDK names, alloc-by-name like the java.net family.
    (
        c::java_util_concurrent_ExecutionException,
        c::java_lang_Exception,
    ),
    (
        c::java_util_concurrent_CancellationException,
        c::java_lang_IllegalStateException,
    ),
    (
        c::java_util_concurrent_TimeoutException,
        c::java_lang_Exception,
    ),
    (
        c::java_util_concurrent_RejectedExecutionException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_IllegalArgumentException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_NullPointerException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_IllegalStateException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_ArithmeticException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_ClassCastException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_UnsupportedOperationException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_IndexOutOfBoundsException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_util_NoSuchElementException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_NumberFormatException,
        c::java_lang_IllegalArgumentException,
    ),
    (
        c::java_util_IllegalFormatException,
        c::java_lang_IllegalArgumentException,
    ),
    // The formatter's family, as in java.util: each names its case (QA
    // 2026-09-13 §7 — `%.2d` threw the base class).
    (
        c::java_util_IllegalFormatConversionException,
        c::java_util_IllegalFormatException,
    ),
    (
        c::java_util_IllegalFormatPrecisionException,
        c::java_util_IllegalFormatException,
    ),
    (
        c::java_util_MissingFormatArgumentException,
        c::java_util_IllegalFormatException,
    ),
    (
        c::java_util_UnknownFormatConversionException,
        c::java_util_IllegalFormatException,
    ),
    // Checked exceptions thrown alloc-by-name from natives (net stack).
    // Mirrors the real java.net hierarchy so superclass catches behave
    // exactly as on Android — note SocketTimeoutException descends from
    // InterruptedIOException, NOT SocketException (real-Java quirk).
    (c::java_io_IOException, c::java_lang_Exception),
    (c::java_io_InterruptedIOException, c::java_io_IOException),
    (
        c::java_net_SocketTimeoutException,
        c::java_io_InterruptedIOException,
    ),
    (c::java_net_SocketException, c::java_io_IOException),
    (c::java_net_ConnectException, c::java_net_SocketException),
    (
        c::java_net_NoRouteToHostException,
        c::java_net_SocketException,
    ),
    (c::java_net_BindException, c::java_net_SocketException),
    (c::java_net_UnknownHostException, c::java_io_IOException),
    (c::java_net_ProtocolException, c::java_io_IOException),
    // javax.net.ssl, thrown by HttpURLConnection's TLS handshake.
    (c::javax_net_ssl_SSLException, c::java_io_IOException),
    (
        c::javax_net_ssl_SSLHandshakeException,
        c::javax_net_ssl_SSLException,
    ),
    (
        c::javax_net_ssl_SSLPeerUnverifiedException,
        c::javax_net_ssl_SSLException,
    ),
    (
        c::java_lang_ArrayIndexOutOfBoundsException,
        c::java_lang_IndexOutOfBoundsException,
    ),
    (
        c::java_lang_ArrayStoreException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_lang_StringIndexOutOfBoundsException,
        c::java_lang_IndexOutOfBoundsException,
    ),
    (
        c::java_lang_NegativeArraySizeException,
        c::java_lang_RuntimeException,
    ),
    (
        c::java_util_ConcurrentModificationException,
        c::java_lang_RuntimeException,
    ),
    (c::java_lang_OutOfMemoryError, c::java_lang_Error),
    (c::java_lang_ExceptionInInitializerError, c::java_lang_Error),
    (c::java_lang_StackOverflowError, c::java_lang_Error),
    // Boxed numerics descend from Number, as Kotlin's `checkcast
    // java/lang/Number` before every `intValue()` unboxing of a generic
    // element requires. No `X → java/lang/Object` rows: `is_instance_of`
    // answers `Object` up front and `dispatch_native` falls through to
    // Object for any class without a row.
    (c::java_lang_Integer, c::java_lang_Number),
    (c::java_lang_Long, c::java_lang_Number),
    (c::java_lang_Float, c::java_lang_Number),
    (c::java_lang_Double, c::java_lang_Number),
    (c::java_lang_Short, c::java_lang_Number),
    (c::java_lang_Byte, c::java_lang_Number),
    // Insertion-ordered collections are aliases of the hash-ordered ones
    // (documented divergence): `mutableMapOf()` / `mutableSetOf()` are
    // inline and emit `new java/util/LinkedHashMap` at the call site.
    (c::java_util_LinkedHashMap, c::java_util_HashMap),
    (c::java_util_LinkedHashSet, c::java_util_HashSet),
];

/// Interfaces implemented by classfile-less builtin classes, flattened to
/// the transitive closure, plus superinterface edges for the JDK interfaces
/// that have no class file of their own (a user class implementing
/// `java/util/List` is a `Collection` and an `Iterable`). Consulted by
/// [`is_instance_of`] at every level of the superclass chain and of the
/// interface walk. Same registration rule as [`BUILTIN_SUPER`].
///
/// Every `java/**` interface lives here rather than in `sdk/java/`: apps
/// compile against the JDK's `ct.sym` (`javac --release 8`, no bootclasspath
/// override), which shadows any SDK file of the same name, and dispatch goes
/// by the receiver's runtime class — so a `.java` file would document
/// nothing, serve nothing, and cost its `.class` size on every board.
pub const BUILTIN_INTERFACES: &[(&str, &[&str])] = &[
    (
        c::java_util_ArrayList,
        &[
            c::java_util_List,
            c::java_util_Collection,
            c::java_lang_Iterable,
        ],
    ),
    (c::java_util_HashMap, &[c::java_util_Map]),
    (
        c::java_util_HashSet,
        &[
            c::java_util_Set,
            c::java_util_Collection,
            c::java_lang_Iterable,
        ],
    ),
    (
        c::java_util_HashMap_KeySet,
        &[
            c::java_util_Set,
            c::java_util_Collection,
            c::java_lang_Iterable,
        ],
    ),
    (
        c::java_util_HashMap_Values,
        &[c::java_util_Collection, c::java_lang_Iterable],
    ),
    (
        c::java_util_HashMap_EntrySet,
        &[
            c::java_util_Set,
            c::java_util_Collection,
            c::java_lang_Iterable,
        ],
    ),
    (
        c::java_lang_String,
        &[c::java_lang_CharSequence, c::java_lang_Comparable],
    ),
    (
        c::java_lang_StringBuilder,
        &[c::java_lang_CharSequence, c::java_lang_Appendable],
    ),
    (c::java_lang_Integer, &[c::java_lang_Comparable]),
    (c::java_lang_Long, &[c::java_lang_Comparable]),
    (c::java_lang_Float, &[c::java_lang_Comparable]),
    (c::java_lang_Double, &[c::java_lang_Comparable]),
    (c::java_lang_Short, &[c::java_lang_Comparable]),
    (c::java_lang_Byte, &[c::java_lang_Comparable]),
    (c::java_lang_Boolean, &[c::java_lang_Comparable]),
    (c::java_lang_Character, &[c::java_lang_Comparable]),
    (c::java_lang_Enum, &[c::java_lang_Comparable]),
    (
        c::java_util_List,
        &[c::java_util_Collection, c::java_lang_Iterable],
    ),
    (
        c::java_util_Set,
        &[c::java_util_Collection, c::java_lang_Iterable],
    ),
    (c::java_util_Collection, &[c::java_lang_Iterable]),
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
fn iface_reaches(classes: Classes<'_>, iface: &[u8], target: &[u8], depth: u8) -> bool {
    iface_reaches_hashed(
        classes,
        name_hash(iface),
        iface,
        name_hash(target),
        target,
        depth,
    )
}

/// [`iface_reaches`] with both names' hashes in hand: every comparison on
/// the way is a `u32` first, bytes only on a match.
fn iface_reaches_hashed(
    classes: Classes<'_>,
    iface_hash: u32,
    iface: &[u8],
    target_hash: u32,
    target: &[u8],
    depth: u8,
) -> bool {
    if iface_hash == target_hash && iface == target {
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
    let Some(cf) = find_class_hashed(classes, iface_hash, iface).map(|i| &classes[i]) else {
        return false;
    };
    cf.interfaces().iter().any(|f| {
        cf.iface_name(f).is_some_and(|sup| {
            iface_reaches_hashed(classes, f.hash(), sup, target_hash, target, depth - 1)
        })
    })
}

/// Returns true if `runtime_class` is the same as, a subclass of, or
/// implements `target_class` (checked at each level of the superclass chain,
/// with superinterfaces walked transitively at each level).
pub(super) fn is_instance_of(
    classes: Classes<'_>,
    runtime_class: &str,
    target_class: &str,
) -> bool {
    // Every reference is an Object — including lambda proxies and
    // handler-allocated objects whose class has neither a class file nor a
    // table row.
    if target_class == c::java_lang_Object {
        return true;
    }
    let target = target_class.as_bytes();
    let target_hash = name_hash(target);
    let mut current: &str = runtime_class;
    let mut current_hash = name_hash(current.as_bytes());
    loop {
        if current_hash == target_hash && current == target_class {
            return true;
        }
        if builtin_interfaces(current).contains(&target_class) {
            return true;
        }
        let ci = match find_class_hashed(classes, current_hash, current.as_bytes()) {
            Some(i) => i,
            None => {
                // No classfile — follow the builtin hierarchy.
                match builtin_super(current) {
                    Some(s) => {
                        current = s;
                        current_hash = name_hash(s.as_bytes());
                        continue;
                    }
                    None => return false,
                }
            }
        };
        // Check implemented interfaces at this level, transitively.
        let cf = &classes[ci];
        for f in cf.interfaces() {
            if let Some(iface_name) = cf.iface_name(f) {
                if iface_reaches_hashed(
                    classes,
                    f.hash(),
                    iface_name,
                    target_hash,
                    target,
                    MAX_IFACE_DEPTH,
                ) {
                    return true;
                }
            }
        }
        match cf.super_class_name() {
            None => return false,
            Some(super_bytes) => match core::str::from_utf8(super_bytes) {
                Ok(s) => {
                    current = s;
                    current_hash = cf.super_hash();
                }
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
        _ => d::t_aObject,
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
    classes: Classes<'_>,
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
        Value::Reference(_) => is_instance_of(classes, c::java_lang_String, target),
        Value::ArrayRef(idx) => match target.as_bytes().first() {
            Some(b'[') => {
                let atype = arrays.atype(idx).unwrap_or(crate::array_heap::ATYPE_REF);
                if atype == crate::array_heap::ATYPE_REF {
                    matches!(target.as_bytes().get(1), Some(b'L') | Some(b'['))
                } else {
                    array_class_name(atype) == target
                }
            }
            _ => matches!(target, c::java_lang_Object | c::java_lang_Cloneable),
        },
        _ => false,
    }
}

/// Index of the `<clinit>` method in `cf`'s own method table.
pub(super) fn find_clinit_in(cf: &ClassFile) -> Option<usize> {
    cf.methods()
        .iter()
        .position(|m| cf.method_name(m) == Some(b"<clinit>"))
}

/// The superclass chain of class `ci`, root-first, as class indices. Only
/// classes present in the loaded set are included; the chain ends where
/// a superclass has no class file.
pub(super) fn superclass_chain_indices(classes: Classes<'_>, ci: usize) -> Vec<usize> {
    let mut chain: Vec<usize> = Vec::new();
    let mut current = Some(ci);
    while let Some(i) = current {
        chain.push(i);
        current = super_index(classes, i);
    }
    chain.reverse(); // root-first
    chain
}

/// JVMS §5.4.3.2 field resolution for a static: the class named by the
/// `Fieldref`, then its superinterfaces (breadth-first, transitively), then
/// its superclass, repeated up the chain. Returns the *declaring* class
/// index and the field's position in that class's own static-field table,
/// which is the store's key. `None` when the named class has no class
/// file, or nothing on the chain declares the field.
pub(super) fn resolve_static_field(
    classes: Classes<'_>,
    class_name: &[u8],
    field_name: &[u8],
) -> Option<(usize, usize)> {
    fn declared_in(cf: &ClassFile, field_name: &[u8]) -> Option<usize> {
        cf.static_fields().iter().position(|f| {
            cf.cp_utf8(f.name_index)
                .is_some_and(|n| name_eq(n, field_name))
        })
    }
    let mut current = find_class(classes, class_name)?;
    let mut queue: Vec<(u32, &'static [u8])> = Vec::new();
    loop {
        let cf = &classes[current];
        if let Some(fi) = declared_in(cf, field_name) {
            return Some((current, fi));
        }
        // Superinterfaces of this class, then theirs (bounded, deduplicated).
        push_interfaces(&mut queue, cf);
        let mut i = 0;
        while i < queue.len() {
            let (hash, name) = queue[i];
            i += 1;
            let Some(ici) = find_class_hashed(classes, hash, name) else {
                continue;
            };
            let icf = &classes[ici];
            if let Some(fi) = declared_in(icf, field_name) {
                return Some((ici, fi));
            }
            push_interfaces(&mut queue, icf);
        }
        current = super_index(classes, current)?;
    }
}

/// JVMS §5.4.3.3 method resolution: find a method starting from `start_class`, walking up the
/// superclass chain. Used by invokevirtual / invokeinterface (starting from the receiver's runtime
/// class) AND by invokestatic / invokespecial (starting from the CP-declared class) — both forms
/// of dispatch recurse to the superclass when the named class doesn't declare the method.
///
/// When the chain misses — it reaches `java/lang/Object`, or leaves the
/// loaded class set (a user class extending a builtin such as
/// `RuntimeException`) — resolution continues into the superinterfaces of
/// every class on the chain ([`find_default_method`]): interface default
/// methods, including the bodies kotlinc emits under `-Xjvm-default=all`.
pub(super) fn find_method_walking(
    classes: Classes<'_>,
    start_class: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let mut current = find_class(classes, start_class.as_bytes());
    while let Some(ci) = current {
        if let Some(mi) = find_method_in(classes, ci, method_name, descriptor) {
            return Some((ci, mi));
        }
        current = super_index(classes, ci);
    }
    find_default_method(classes, start_class.as_bytes(), method_name, descriptor)
}

/// Bound on the interfaces visited per resolution. Real hierarchies have a
/// handful; a hand-assembled cycle must not spin.
const MAX_IFACES: usize = 16;

/// JVMS §5.4.3.3 step 3: the maximally-specific superinterface method with a
/// body. Breadth-first over the interfaces of every loaded class on
/// `start_class`'s superclass chain, then their superinterfaces; a candidate
/// declared in a subinterface of the one held so far replaces it (a
/// sub-interface's override beats the inherited default whatever the
/// `implements` order). Abstract declarations are skipped, and interfaces
/// with no class file (`kotlin/jvm/internal/markers/KMappedMarker`) simply
/// end their branch. Only reached on a miss, so an interface is parsed the
/// first time a default has to be found through it, never eagerly.
#[inline(never)]
fn find_default_method(
    classes: Classes<'_>,
    start_class: &[u8],
    method_name: &str,
    descriptor: &str,
) -> Option<(usize, usize)> {
    let mut queue: Vec<(u32, &'static [u8])> = Vec::new();
    let mut current = find_class(classes, start_class);
    while let Some(ci) = current {
        push_interfaces(&mut queue, &classes[ci]);
        current = super_index(classes, ci);
    }
    let mut best: Option<(&'static [u8], usize, usize)> = None;
    let mut i = 0;
    while i < queue.len() {
        let (hash, name) = queue[i];
        i += 1;
        let Some(ci) = find_class_hashed(classes, hash, name) else {
            continue;
        };
        let cf = &classes[ci];
        push_interfaces(&mut queue, cf);
        if let Some(mi) = find_method_in(classes, ci, method_name, descriptor) {
            if classes[ci].methods()[mi].code_offset == 0 {
                continue;
            }
            match best {
                Some((held, _, _)) if !iface_reaches(classes, name, held, MAX_IFACE_DEPTH) => {}
                _ => best = Some((name, ci, mi)),
            }
        }
    }
    best.map(|(_, ci, mi)| (ci, mi))
}

/// Append `cf`'s direct superinterfaces to `queue` (deduplicated, bounded).
fn push_interfaces(queue: &mut Vec<(u32, &'static [u8])>, cf: &ClassFile) {
    for f in cf.interfaces() {
        if let Some(n) = cf.iface_name(f) {
            if queue.len() < MAX_IFACES && !queue.iter().any(|&(_, q)| q == n) {
                queue.push((f.hash(), n));
            }
        }
    }
}

/// Extract the class name from the return type of a method descriptor:
/// `"()Ljava/lang/Runnable;"` gives `Some("java/lang/Runnable")`.
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
    classes: Classes<'_>,
    extra_native_classes: &[&'static str],
    name: &str,
) -> &'static str {
    name_of(
        classes,
        extra_native_classes,
        class_name_ref(classes, extra_native_classes, name),
    )
}

/// Where `name`'s canonical `&'static str` comes from — the lookup half of
/// [`class_name_to_static_in`], as an index a `new` site's cache entry can
/// hold without a pointer ([`NameRef`]).
pub(super) fn class_name_ref(
    classes: Classes<'_>,
    extra_native_classes: &[&'static str],
    name: &str,
) -> NameRef {
    // 1. Loaded user classes (Flash-backed)
    if let Some(ci) = find_class(classes, name.as_bytes()) {
        if classes[ci]
            .class_name()
            .is_some_and(|cn| core::str::from_utf8(cn).is_ok())
        {
            if let Ok(i) = u16::try_from(ci) {
                return NameRef::Loaded(i);
            }
        }
    }
    // 2. JVM builtins
    if let Some(i) = crate::native::BUILTIN_CLASS_NAMES
        .iter()
        .position(|b| name_eq(b.as_bytes(), name.as_bytes()))
    {
        if let Ok(i) = u16::try_from(i) {
            return NameRef::Builtin(i);
        }
    }
    // 3. Host-supplied native classes
    if let Some(i) = extra_native_classes
        .iter()
        .position(|e| name_eq(e.as_bytes(), name.as_bytes()))
    {
        if let Ok(i) = u16::try_from(i) {
            return NameRef::Native(i);
        }
    }
    NameRef::Unknown
}

/// The `&'static str` a [`NameRef`] stands for.
pub(super) fn name_of(
    classes: Classes<'_>,
    extra_native_classes: &[&'static str],
    r: NameRef,
) -> &'static str {
    match r {
        NameRef::Loaded(i) => classes
            .get(i as usize)
            .and_then(|cf| cf.class_name())
            .and_then(|cn| core::str::from_utf8(cn).ok())
            .unwrap_or("unknown"),
        NameRef::Builtin(i) => crate::native::BUILTIN_CLASS_NAMES
            .get(i as usize)
            .copied()
            .unwrap_or("unknown"),
        NameRef::Native(i) => extra_native_classes
            .get(i as usize)
            .copied()
            .unwrap_or("unknown"),
        NameRef::Unknown => "unknown",
    }
}
