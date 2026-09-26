// SPDX-License-Identifier: GPL-3.0-only
/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use alloc::boxed::Box;
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
// `walk_cp` still skips one by raw tag value, like every other tag it does
// not name.
const TAG_INVOKE_DYNAMIC: u8 = 18;

/// One entry in the BootstrapMethods class attribute, located in flash:
/// the record keeps the attribute's offset and decodes an entry on demand
/// ([`ClassFile::bootstrap_method`]), so a class's bootstrap table costs no
/// RAM.
#[derive(Debug, Clone, Copy)]
pub struct BootstrapMethod {
    /// CP index of CONSTANT_MethodHandle for the bootstrap method.
    pub method_ref: u16,
    /// Byte offset of the first argument's CP index inside the class data.
    pub(crate) args_off: u16,
    /// Number of bootstrap arguments.
    pub num_args: u16,
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

/// A method's parsed header. All `u16`, `#[repr(C)]`: it is stored inline
/// in the class's [`Parsed`] record and is the same 14 bytes (16 with line
/// numbers) on every target. The exception table is not here — it sits in
/// flash right after the bytecode and [`ClassFile::exception_table`] reads
/// it from there.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MethodInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    /// Byte offset of the Code attribute's bytecode array inside `data`.
    /// 0 means the method is native (no Code attribute).
    pub code_offset: u16,
    pub code_len: u16,
    pub max_stack: u16,
    pub max_locals: u16,
    pub access_flags: u16,
    /// Byte offset of the LineNumberTable body (entry_count u16 + entries)
    /// inside the Flash-backed class data; 0 = not present. `line-numbers`
    /// feature only.
    #[cfg(feature = "line-numbers")]
    pub lnt_offset: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
}

/// A record type stored inline in a [`Parsed`] blob.
///
/// # Safety
/// Implementors are `#[repr(C)]` structs made only of `u16` fields: size a
/// multiple of two, alignment two, no padding, every bit pattern valid.
/// That is what lets a `&[u16]` region be viewed as `&[T]`.
unsafe trait WordRecord: Copy {
    const WORDS: usize;
}

unsafe impl WordRecord for MethodInfo {
    const WORDS: usize = core::mem::size_of::<MethodInfo>() / 2;
}
unsafe impl WordRecord for FieldInfo {
    const WORDS: usize = core::mem::size_of::<FieldInfo>() / 2;
}

const _: () = assert!(core::mem::align_of::<MethodInfo>() == 2);
const _: () = assert!(core::mem::align_of::<FieldInfo>() == 2);
const _: () = assert!(core::mem::size_of::<FieldInfo>() == 4);
// Loosen only with a parity-audit update: the same bytes on every target.
const _: () = assert!(
    core::mem::size_of::<MethodInfo>()
        == if cfg!(feature = "line-numbers") {
            16
        } else {
            14
        }
);

/// View a word region as records. The region's length must be a multiple
/// of the record size (the parser sizes it so; a wrong header panics on
/// the slice bounds, never reads out of them).
fn words_as<T: WordRecord>(w: &[u16]) -> &[T] {
    debug_assert!(w.len() % T::WORDS == 0);
    // SAFETY: `WordRecord`'s contract — `T` is a `#[repr(C)]` record of
    // `u16`s, so alignment 2 (that of `w`), no padding and no invalid bit
    // patterns; the length is the region's, in whole records.
    unsafe { core::slice::from_raw_parts(w.as_ptr().cast::<T>(), w.len() / T::WORDS) }
}

fn words_as_mut<T: WordRecord>(w: &mut [u16]) -> &mut [T] {
    debug_assert!(w.len() % T::WORDS == 0);
    // SAFETY: as `words_as`, through a unique borrow.
    unsafe { core::slice::from_raw_parts_mut(w.as_mut_ptr().cast::<T>(), w.len() / T::WORDS) }
}

/// Fully-parsed internals of a class file, populated lazily on first
/// access: one exact allocation (`blob`) and a header of `u16` offsets
/// into it. The host and the device differ by the blob's fat pointer and
/// nothing else (M8, docs/parity-audit.md).
///
/// Blob layout, in `u16` words:
/// - `[0, cp_count)`: byte offset of each CP entry's *data* (after the tag
///   byte) within the class data; index 0 and the pad slot after a
///   Long/Double are 0.
/// - `cp_count` words of packed `u8` tags, one per CP entry (padded to a
///   word).
/// - `fields_len` [`FieldInfo`] records, then `statics_len` of them.
/// - the interface list: the file's `interfaces_count` words reserved,
///   `ifaces_len` of them filled with Utf8 CP indices.
/// - `methods_len` [`MethodInfo`] records.
#[derive(Debug)]
pub struct Parsed {
    blob: Box<[u16]>,
    cp_count: u16,
    /// Word offset of the instance field records (`cp_count` plus the tag
    /// words; cached so `fields()` is one add).
    fields_off: u16,
    fields_len: u16,
    statics_len: u16,
    ifaces_len: u16,
    /// Word offset of the method records, the hot region.
    methods_off: u16,
    methods_len: u16,
    /// Byte offset of the BootstrapMethods attribute body in the class
    /// data (`num_bootstrap_methods` first); 0 = none.
    bsm_offset: u16,
    pub(crate) class_name_index: u16,
    pub(crate) super_class_name_index: u16,
    pub(crate) access_flags: u16,
    /// CP index of the `SourceFile` attribute's Utf8 (0 = none).
    #[cfg(feature = "line-numbers")]
    pub(crate) source_file_index: u16,
}

// Loosen only with a parity-audit update: 40 B on a 64-bit host, 32 B on
// the device — the blob's fat pointer is the whole difference.
const _: () = assert!(
    core::mem::size_of::<Parsed>()
        == if cfg!(target_pointer_width = "64") {
            40
        } else {
            32
        }
);

impl Parsed {
    /// Words the packed tag bytes take.
    #[inline]
    const fn tag_words(cp_count: usize) -> usize {
        (cp_count + 1) / 2
    }

    /// The tag region as bytes, `cp_count` of them. Written and read
    /// through the same view, so endianness never enters.
    #[inline]
    fn tag_bytes(blob: &[u16], cp_count: usize) -> &[u8] {
        let words = &blob[cp_count..cp_count + Self::tag_words(cp_count)];
        // SAFETY: a `u8` view of `u16` storage, `2 * len` bytes, same
        // lifetime; clamped to the `cp_count` tags that are meaningful.
        let bytes =
            unsafe { core::slice::from_raw_parts(words.as_ptr().cast::<u8>(), words.len() * 2) };
        &bytes[..cp_count]
    }

    #[inline]
    fn tag_bytes_mut(blob: &mut [u16], cp_count: usize) -> &mut [u8] {
        let words = &mut blob[cp_count..cp_count + Self::tag_words(cp_count)];
        // SAFETY: as `tag_bytes`, through a unique borrow.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 2)
        };
        &mut bytes[..cp_count]
    }

    /// Byte offset of CP entry `i`'s data. `i` must have passed
    /// [`Self::cp_tag`].
    #[inline]
    pub(crate) fn cp_offset(&self, i: usize) -> usize {
        self.blob[i] as usize
    }

    /// Tag of CP entry `i`, `None` past the pool.
    #[inline]
    pub(crate) fn cp_tag(&self, i: usize) -> Option<u8> {
        let cp_count = self.cp_count as usize;
        (i < cp_count).then(|| Self::tag_bytes(&self.blob, cp_count)[i])
    }

    #[inline]
    pub(crate) fn fields(&self) -> &[FieldInfo] {
        let off = self.fields_off as usize;
        words_as(&self.blob[off..off + 2 * self.fields_len as usize])
    }

    #[inline]
    pub(crate) fn static_fields(&self) -> &[FieldInfo] {
        let off = self.fields_off as usize + 2 * self.fields_len as usize;
        words_as(&self.blob[off..off + 2 * self.statics_len as usize])
    }

    #[inline]
    fn ifaces_off(&self) -> usize {
        self.fields_off as usize + 2 * (self.fields_len as usize + self.statics_len as usize)
    }

    #[inline]
    pub(crate) fn interfaces(&self) -> &[u16] {
        let off = self.ifaces_off();
        &self.blob[off..off + self.ifaces_len as usize]
    }

    #[inline]
    pub(crate) fn methods(&self) -> &[MethodInfo] {
        let off = self.methods_off as usize;
        words_as(&self.blob[off..off + self.methods_len as usize * MethodInfo::WORDS])
    }

    #[inline]
    pub(crate) fn bsm_offset(&self) -> usize {
        self.bsm_offset as usize
    }
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
    /// Where the pre-scanned class name lies inside `data` (the Utf8 bytes
    /// of the constant pool's `this_class` entry, exactly as the class
    /// file spells them). Offsets rather than a second slice: the table
    /// then costs the host one fat pointer more than the device per entry,
    /// not two (M8, docs/parity-audit.md).
    name_off: u16,
    name_len: u16,
    /// [`name_hash`] of the name, so a lookup by name compares one `u32`
    /// per registered class before touching bytes — see [`find_class`].
    name_hash: u32,
    /// Fully-parsed internals; filled on first access via `parsed()`.
    /// Boxed so an unparsed ClassFile is one null pointer instead of an
    /// inlined record header, on the classes the app never touches.
    parsed: OnceCell<Box<Parsed>>,
}

// Loosen only with a parity-audit update: 32 B on a 64-bit host, 20 B on
// the device — the data slice's fat pointer and the `Box` pointer.
const _: () = assert!(
    core::mem::size_of::<ClassFile>()
        == if cfg!(target_pointer_width = "64") {
            32
        } else {
            20
        }
);

/// What a `ClassFile` costs on this target over the device's: one fat
/// pointer (8 B) and one thin pointer (4 B) on a 64-bit host, 0 on the
/// device.
pub const CLASS_FILE_DELTA: usize = FAT_PTR_DELTA + (core::mem::size_of::<usize>() - 4);

impl ClassFile {
    /// Returns the raw bytecode slice backing this class file.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// Returns a reference to the parsed internals, parsing on first call.
    ///
    /// Panics only if the class data is malformed or the heap refuses the
    /// one allocation the record takes — registration (`register`) already
    /// validated the constant pool enough to extract the class name, so in
    /// practice a subsequent full parse should not fail.
    pub(crate) fn parsed(&self) -> &Parsed {
        self.parsed.get_or_init(|| {
            Box::new(
                Parsed::parse(self.data)
                    .expect("class file unparseable after registration, or no heap for its record"),
            )
        })
    }

    /// Returns `true` if the full parse has already been performed.
    pub fn is_parsed(&self) -> bool {
        self.parsed.get().is_some()
    }

    /// `name` is `(byte offset, length)` of the class name inside `data`;
    /// both fit a `u16` because the parser refuses class files past 64 KB.
    pub(crate) fn new_lazy(data: &'static [u8], name: (usize, usize)) -> Self {
        Self {
            data,
            name_off: name.0 as u16,
            name_len: name.1 as u16,
            name_hash: name_hash(&data[name.0..name.0 + name.1]),
            parsed: OnceCell::new(),
        }
    }

    pub(crate) fn new_eager(data: &'static [u8], name: (usize, usize), parsed: Parsed) -> Self {
        let cell = OnceCell::new();
        let _ = cell.set(Box::new(parsed));
        Self {
            data,
            name_off: name.0 as u16,
            name_len: name.1 as u16,
            name_hash: name_hash(&data[name.0..name.0 + name.1]),
            parsed: cell,
        }
    }

    /// Returns the pre-scanned class name (does not trigger a full parse).
    /// The slice is the class file's own bytes, so its address is stable
    /// for as long as the class is loaded.
    #[inline]
    pub(crate) fn scanned_name(&self) -> &'static [u8] {
        let off = self.name_off as usize;
        &self.data[off..off + self.name_len as usize]
    }

    /// Whether this class is named `name`, given `hash == name_hash(name)`.
    #[inline]
    pub fn is_named(&self, name: &[u8], hash: u32) -> bool {
        self.name_hash == hash && name_eq(self.scanned_name(), name)
    }

    /// RAM held by this class's lazily-parsed metadata, as
    /// `(host_bytes, device_bytes)`. `None` when the parse has not run —
    /// an unparsed entry costs only its `ClassFile` struct in the class
    /// table. The totals of [`Self::parsed_metadata_census`].
    pub fn parsed_metadata_bytes(&self) -> Option<(usize, usize)> {
        let c = self.parsed_metadata_census()?;
        Some((c.host.total(), c.dev.total()))
    }

    /// The same figure broken down by the part of [`Parsed`] that holds it,
    /// with the counts that drive each part.
    ///
    /// `host` is what this process pays (`size_of` and the blob's length);
    /// `dev` is the device's figure for the same class: the blob is
    /// byte-identical, and the header differs by [`FAT_PTR_DELTA`]. Two
    /// allocations per class on both targets, so no allocator header term
    /// is modelled here — the ledger (`PICODROID_MEMDIAG_SITES`) sees the
    /// real blocks.
    pub fn parsed_metadata_census(&self) -> Option<MetaCensus> {
        let p = self.parsed.get()?;
        let cp_count = p.cp_count as usize;
        let host = MetaParts {
            boxed: core::mem::size_of::<Parsed>(),
            cp_offsets: cp_count * 2,
            cp_tags: Parsed::tag_words(cp_count) * 2,
            methods: p.methods_len as usize * core::mem::size_of::<MethodInfo>(),
            fields: p.fields_len as usize * core::mem::size_of::<FieldInfo>(),
            statics: p.statics_len as usize * core::mem::size_of::<FieldInfo>(),
            interfaces: (p.methods_off as usize - p.ifaces_off()) * 2,
        };
        debug_assert_eq!(host.total() - host.boxed, p.blob.len() * 2);
        let mut dev = host;
        dev.boxed -= FAT_PTR_DELTA;
        let exc_entries = p
            .methods()
            .iter()
            .map(|m| self.exception_table(m).len())
            .sum();
        Some(MetaCensus {
            host,
            dev,
            cp_entries: cp_count,
            methods: p.methods_len as usize,
            fields: p.fields_len as usize + p.statics_len as usize,
            exc_entries,
            class_bytes: self.data.len(),
        })
    }
}

/// What a `Box<[T]>` costs on this target over the device's 8-byte fat
/// pointer: 8 on a 64-bit host, 0 on the device. The one term by which a
/// class's parsed metadata differs between the simulator and the board.
pub const FAT_PTR_DELTA: usize = 2 * core::mem::size_of::<usize>() - 8;

/// Bytes of parsed metadata by the part of [`Parsed`] that holds them.
/// One instance per layout model (host or device); see
/// [`ClassFile::parsed_metadata_census`].
#[derive(Clone, Copy, Default, Debug)]
pub struct MetaParts {
    /// The `Box<Parsed>` itself: the record header.
    pub boxed: usize,
    /// `cp_offsets`: one `u16` per constant-pool entry.
    pub cp_offsets: usize,
    /// `cp_tags`: one byte per constant-pool entry, padded to a word.
    pub cp_tags: usize,
    /// `methods`: one `MethodInfo` per method.
    pub methods: usize,
    /// Instance field table.
    pub fields: usize,
    /// Static field table.
    pub statics: usize,
    /// Interface index list.
    pub interfaces: usize,
}

impl MetaParts {
    pub fn total(&self) -> usize {
        self.boxed
            + self.cp_offsets
            + self.cp_tags
            + self.methods
            + self.fields
            + self.statics
            + self.interfaces
    }

    pub fn add(&mut self, o: &MetaParts) {
        self.boxed += o.boxed;
        self.cp_offsets += o.cp_offsets;
        self.cp_tags += o.cp_tags;
        self.methods += o.methods;
        self.fields += o.fields;
        self.statics += o.statics;
        self.interfaces += o.interfaces;
    }
}

/// One parsed class's metadata cost with the counts behind it.
#[derive(Clone, Copy, Debug)]
pub struct MetaCensus {
    pub host: MetaParts,
    pub dev: MetaParts,
    pub cp_entries: usize,
    pub methods: usize,
    pub fields: usize,
    /// Exception-table entries across every method: in flash, not RAM.
    pub exc_entries: usize,
    /// Size of the class file in flash, for the RAM-per-flash-byte ratio.
    pub class_bytes: usize,
}

/// A method's exception table, decoded entry by entry from the class bytes
/// (JVMS §4.7.3: a `u16` count then 8-byte entries, right after the
/// bytecode). See [`ClassFile::exception_table`].
pub struct ExceptionTable {
    data: &'static [u8],
    pos: usize,
    remaining: usize,
}

impl Iterator for ExceptionTable {
    type Item = ExceptionEntry;

    #[inline]
    fn next(&mut self) -> Option<ExceptionEntry> {
        if self.remaining == 0 {
            return None;
        }
        let e = self.data.get(self.pos..self.pos + 8)?;
        self.pos += 8;
        self.remaining -= 1;
        Some(ExceptionEntry {
            start_pc: u16::from_be_bytes([e[0], e[1]]),
            end_pc: u16::from_be_bytes([e[2], e[3]]),
            handler_pc: u16::from_be_bytes([e[4], e[5]]),
            catch_type_index: u16::from_be_bytes([e[6], e[7]]),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for ExceptionTable {}

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
