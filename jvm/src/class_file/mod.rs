// SPDX-License-Identifier: GPL-3.0-only
/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::OnceCell;

mod accessors;
mod parse;
#[cfg(test)]
mod tests;

// Constant pool tag constants
const TAG_UTF8: u8 = 1;
const TAG_CLASS: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_FIELDREF: u8 = 9;
const TAG_METHODREF: u8 = 10;
const TAG_NAME_AND_TYPE: u8 = 12;
const TAG_METHOD_HANDLE: u8 = 15;
// No TAG_METHOD_TYPE (16): nothing resolves a CONSTANT_MethodType entry.
// `parse_cp` still skips one by raw tag value, like every other tag it does
// not name.
const TAG_INVOKE_DYNAMIC: u8 = 18;

/// One entry in the BootstrapMethods class attribute.
#[derive(Debug, Clone)]
pub struct BootstrapMethod {
    /// CP index of CONSTANT_MethodHandle for the bootstrap method.
    pub method_ref: u16,
    /// CP indices of the bootstrap arguments.
    pub arguments: Vec<u16>,
}

/// One entry in a method's exception table (try/catch region).
#[derive(Debug, Clone, Copy)]
pub struct ExceptionEntry {
    /// Start of the guarded region (inclusive), as a bytecode offset.
    pub start_pc: u16,
    /// End of the guarded region (exclusive), as a bytecode offset.
    pub end_pc: u16,
    /// Bytecode offset of the catch handler.
    pub handler_pc: u16,
    /// CP index of the caught class (CONSTANT_Class), or 0 to catch any (finally).
    pub catch_type_index: u16,
}

#[derive(Debug)]
pub struct MethodInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    /// Byte offset of the Code attribute's bytecode array inside `data`.
    /// 0 means the method is native (no Code attribute).
    pub code_offset: usize,
    pub code_len: usize,
    pub max_stack: u16,
    pub max_locals: u16,
    pub access_flags: u16,
    /// Exception table parsed from the Code attribute.
    pub exception_table: Vec<ExceptionEntry>,
    /// Byte offset of the LineNumberTable body (entry_count u16 + entries) inside
    /// the Flash-backed class data. 0 = not present. Debug builds only.
    #[cfg(debug_assertions)]
    pub lnt_offset: usize,
    /// Byte length of the LineNumberTable body (= 2 + entry_count*4). Debug builds only.
    #[cfg(debug_assertions)]
    pub lnt_len: usize,
}

#[derive(Debug)]
pub struct FieldInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
}

/// Fully-parsed internals of a class file.  Populated lazily on first access.
#[derive(Debug)]
pub(crate) struct Parsed {
    /// Byte offset of each CP entry's *data* (after the tag byte) within `data`.
    /// Index 0 is unused (CP is 1-based); index N corresponds to CP entry N.
    pub cp_offsets: Vec<usize>,
    /// Tag of each CP entry (same indexing as cp_offsets).
    pub cp_tags: Vec<u8>,
    pub methods: Vec<MethodInfo>,
    pub class_name_index: u16,
    pub super_class_name_index: u16,
    pub fields: Vec<FieldInfo>,
    pub static_fields: Vec<FieldInfo>,
    pub access_flags: u16,
    pub interfaces: Vec<u16>,
    pub bootstrap_methods: Vec<BootstrapMethod>,
}

/// A class file backed by a `&'static [u8]` slice in Flash.
///
/// The class name is scanned eagerly at registration so name-based lookups
/// (e.g. `find_method`, `class_name_to_static_in`) can iterate all registered
/// classes without forcing a full parse.  All other accessors route through
/// [`Parsed`] which is populated on first access.
#[derive(Debug)]
pub struct ClassFile {
    data: &'static [u8],
    /// Pre-scanned class name (Flash-backed UTF8 bytes from the constant
    /// pool), exactly as the class file spells it.
    name: &'static [u8],
    /// [`name_hash`] of `name`, so a lookup by name compares one `u32` per
    /// registered class before touching bytes — see [`find_class`].
    name_hash: u32,
    /// Fully-parsed internals; filled on first access via `parsed()`.
    /// Boxed so an unparsed ClassFile is one null pointer (8 B) instead of an
    /// inlined ~176 B of empty Vec headers — saves ~21 KB on 128 framework
    /// classes when most are never accessed.
    parsed: OnceCell<Box<Parsed>>,
}

impl ClassFile {
    /// Returns the raw bytecode slice backing this class file.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// Returns a reference to the parsed internals, parsing on first call.
    ///
    /// Panics only if the class data is malformed — registration (`register`)
    /// already validated the constant pool enough to extract the class name,
    /// so in practice a subsequent full parse should not fail.
    pub(crate) fn parsed(&self) -> &Parsed {
        self.parsed.get_or_init(|| {
            Box::new(
                Parsed::parse(self.data).expect("class file became unparseable after registration"),
            )
        })
    }

    /// Returns `true` if the full parse has already been performed.
    pub fn is_parsed(&self) -> bool {
        self.parsed.get().is_some()
    }

    pub(crate) fn new_lazy(data: &'static [u8], name: &'static [u8]) -> Self {
        Self {
            data,
            name,
            name_hash: name_hash(name),
            parsed: OnceCell::new(),
        }
    }

    pub(crate) fn new_eager(data: &'static [u8], name: &'static [u8], parsed: Parsed) -> Self {
        let cell = OnceCell::new();
        let _ = cell.set(Box::new(parsed));
        Self {
            data,
            name,
            name_hash: name_hash(name),
            parsed: cell,
        }
    }

    /// Returns the pre-scanned class name (does not trigger a full parse).
    pub(crate) fn scanned_name(&self) -> &'static [u8] {
        self.name
    }

    /// Whether this class is named `name`, given `hash == name_hash(name)`.
    #[inline]
    pub fn is_named(&self, name: &[u8], hash: u32) -> bool {
        self.name_hash == hash && name_eq(self.name, name)
    }

    /// Approximate RAM held by this class's lazily-parsed metadata, as
    /// `(host_bytes, device_bytes)`. `None` when the parse has not run —
    /// an unparsed entry costs only its `ClassFile` struct in the class
    /// table.
    ///
    /// `host_bytes` is what this process actually pays (real `size_of` /
    /// capacities — the figure the sim's modeled arena sees). Pointer-width
    /// parts differ 2× on the 64-bit host, so `device_bytes` re-derives the
    /// 32-bit release layout (4-byte usize, 12-byte Vec headers) and adds
    /// one 8-byte heap_4 block header per real allocation — use the device
    /// figure for sizing decisions.
    #[cfg(feature = "mem-diag")]
    pub fn parsed_metadata_bytes(&self) -> Option<(usize, usize)> {
        // Device layout constants (32-bit release):
        /// heap_4 BlockLink_t header per allocation.
        const DEV_HDR: usize = 8;
        /// `Parsed`: 6 Vec headers (12 B) + 3 u16 scalars, padded.
        const DEV_PARSED: usize = 80;
        /// `MethodInfo` without the debug-only lnt fields: 7 scalars (18 B)
        /// + exception-table Vec header (12 B), padded to 4.
        const DEV_METHOD_INFO: usize = 32;
        /// `BootstrapMethod`: u16 + Vec header, padded.
        const DEV_BOOTSTRAP: usize = 16;
        /// Payload bytes -> device cost including the block header (empty
        /// Vecs don't allocate).
        fn dev_alloc(payload: usize) -> usize {
            if payload > 0 {
                payload + DEV_HDR
            } else {
                0
            }
        }

        let p = self.parsed.get()?;

        let mut host = core::mem::size_of::<Parsed>();
        host += p.cp_offsets.capacity() * core::mem::size_of::<usize>();
        host += p.cp_tags.capacity();
        host += p.methods.capacity() * core::mem::size_of::<MethodInfo>();
        host += p.fields.capacity() * core::mem::size_of::<FieldInfo>();
        host += p.static_fields.capacity() * core::mem::size_of::<FieldInfo>();
        host += p.interfaces.capacity() * core::mem::size_of::<u16>();
        host += p.bootstrap_methods.capacity() * core::mem::size_of::<BootstrapMethod>();

        let mut dev = DEV_PARSED + DEV_HDR; // the Box allocation
        dev += dev_alloc(p.cp_offsets.capacity() * 4);
        dev += dev_alloc(p.cp_tags.capacity());
        dev += dev_alloc(p.methods.capacity() * DEV_METHOD_INFO);
        dev += dev_alloc(p.fields.capacity() * 4);
        dev += dev_alloc(p.static_fields.capacity() * 4);
        dev += dev_alloc(p.interfaces.capacity() * 2);
        dev += dev_alloc(p.bootstrap_methods.capacity() * DEV_BOOTSTRAP);

        for m in &p.methods {
            let payload = m.exception_table.capacity() * core::mem::size_of::<ExceptionEntry>();
            host += payload;
            dev += dev_alloc(payload); // ExceptionEntry is 8 B on all targets
        }
        for b in &p.bootstrap_methods {
            let payload = b.arguments.capacity() * core::mem::size_of::<u16>();
            host += payload;
            dev += dev_alloc(payload);
        }
        Some((host, dev))
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        let hi = self.u8()? as u16;
        let lo = self.u8()? as u16;
        Some((hi << 8) | lo)
    }

    fn u32(&mut self) -> Option<u32> {
        let hi = self.u16()? as u32;
        let lo = self.u16()? as u32;
        Some((hi << 16) | lo)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.pos = self.pos.checked_add(n)?;
        if self.pos > self.data.len() {
            return None;
        }
        Some(())
    }

    fn pos(&self) -> usize {
        self.pos
    }
}

/// FNV-1a over a class name. Every registered class carries its hash, so a
/// lookup by name is one integer compare per class before any bytes are
/// read. That matters under `--shrink`: every framework and `java/**` class
/// is then a four-byte `a/XX` / `b/XX` name, so the length check that used
/// to reject almost every candidate passes for all of them, and a plain
/// byte compare became a `bcmp` call per class per lookup (17× the compare
/// calls of a no-shrink run, 73 % slower string benchmarks).
///
/// Out of line, like [`name_eq`] and [`find_class`]: inlined at every scan
/// site these cost 8 KB of rp2040 flash, and a local `bl` is no dearer than
/// the `bcmp` call they replace.
#[inline(never)]
pub const fn name_hash(name: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < name.len() {
        h ^= name[i] as u32;
        h = h.wrapping_mul(0x0100_0193);
        i += 1;
    }
    h
}

/// Equality for the short byte strings the JVM matches on — class, method
/// and field names, descriptors. Up to 16 bytes it is an inlined loop
/// rather than a `bcmp` call, which is what a two- or four-byte shrunk name
/// wants; longer slices take the library path.
#[inline(never)]
pub fn name_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.len() <= 16 {
        for i in 0..a.len() {
            if a[i] != b[i] {
                return false;
            }
        }
        true
    } else {
        a == b
    }
}

/// Index of the registered class named `name`, if any. The one lookup
/// every resolution path funnels through; see [`name_hash`].
#[inline(never)]
pub fn find_class(classes: &[ClassFile], name: &[u8]) -> Option<usize> {
    let hash = name_hash(name);
    classes.iter().position(|cf| cf.is_named(name, hash))
}
