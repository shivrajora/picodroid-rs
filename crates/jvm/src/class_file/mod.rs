// SPDX-License-Identifier: GPL-3.0-only
//! Class files and their link tables.
//!
//! A class arrives as two `&'static` slices in flash: the standard `.class`
//! bytes, and the link table `class-link` built for them at pack time (an
//! app) or firmware-build time (the framework) — every constant-pool
//! offset, the method and field tables, a hash per method signature and per
//! superclass / interface name, and a descriptor per `Methodref`. Nothing is
//! parsed at load and nothing is kept in RAM per class beyond
//! [`ClassFile`]'s two pointers: [`ClassFile::linked`] checks the table's
//! header and that is registration. The layout is documented in
//! `crates/class-link/src/layout.rs`; the tables were validated against
//! their class bytes when the set was packed, embedded or installed, so the
//! runtime trusts them.
//!
//! Until 2026-09 the same record (`Parsed`) was built lazily on first touch
//! and boxed on the heap, ~360 B a class — the largest heap consumer on the
//! device (docs/designs/class-link-2026-09.md).

use core::ptr::NonNull;

pub use class_link::{FieldInfo, IfaceInfo, Link, LinkError, Linked, MethodInfo, MethodrefDesc};

mod accessors;
pub mod index;
#[cfg(test)]
mod tests;

pub use index::{find_class, find_class_hashed, ClassIndex, Classes};

/// Test-only tally of [`ClassFile::method_name`] reads: the hash-based
/// resolution walk confirms a hit with exactly one and reads none on a miss
/// (`interpreter::tests::invoke::a_hash_walk_reads_one_name`).
#[cfg(test)]
pub static METHOD_NAME_READS: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// One entry in the BootstrapMethods class attribute, located in flash:
/// the table keeps the attribute's offset and an entry is decoded on demand
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

/// A class file backed by a `&'static [u8]` slice in flash, with its link
/// table beside it.
#[derive(Debug)]
pub struct ClassFile {
    data: &'static [u8],
    /// The link table's first word; the table carries its own length
    /// ([`Link::from_raw`]). A thin pointer rather than a slice.
    link: NonNull<u16>,
    /// [`name_hash`] of the class name — a copy of the table's, kept in RAM
    /// because a lookup by name compares it for every class it scans (the
    /// tail past the indexed sections; every class, in a test), and three
    /// XIP reads a class made `find_class` ~7× slower than the RAM compare
    /// it replaced (`object_allocation` 6.4 → 12.2 s on the RP2350).
    name_hash: u32,
}

// SAFETY: the pointer is to an immutable table in flash (or a leaked buffer
// in tests), shared freely; `ClassFile` hands out only shared views.
unsafe impl Send for ClassFile {}
unsafe impl Sync for ClassFile {}

// Loosen only with a parity-audit update: 32 B on a 64-bit host, 16 B on
// the device — the data slice's fat pointer, one thin pointer, the hash.
const _: () = assert!(
    core::mem::size_of::<ClassFile>()
        == if cfg!(target_pointer_width = "64") {
            32
        } else {
            16
        }
);

/// What a `ClassFile` costs on this target over the device's: one fat
/// pointer (8 B) and one thin pointer (4 B) on a 64-bit host (plus their
/// padding), 0 on the device.
pub const CLASS_FILE_DELTA: usize = core::mem::size_of::<ClassFile>() - 16;

impl ClassFile {
    /// Register a class from its bytes and link table. O(1): the table's
    /// header must name these bytes' length and resolve the class name;
    /// the deep check ([`Link::validate`]) ran when the set was built.
    pub fn linked(l: Linked<'static>) -> Result<Self, LinkError> {
        if l.link.class_len() != l.class.len() {
            return Err(LinkError::WordMismatch { word: 2 });
        }
        if l.name().is_none() {
            return Err(LinkError::BadOffset { word: 13 });
        }
        Ok(Self {
            data: l.class,
            link: NonNull::from(&l.link.words()[0]),
            name_hash: l.link.name_hash(),
        })
    }

    /// Link `data` now and register the result — the host's and the tests'
    /// way in, for class bytes that no packer has seen. The table is
    /// leaked: a test's classes live for the test, and a host tool's for
    /// the process.
    #[cfg(any(test, feature = "link-at-load"))]
    pub fn parse(data: &'static [u8]) -> Result<Self, &'static str> {
        let words = class_link::build::link_class(data).map_err(|e| e.as_str())?;
        let words: &'static [u16] = alloc::boxed::Box::leak(words.into_boxed_slice());
        let link = Link::new(words).map_err(|e| e.as_str())?;
        Self::linked(Linked { class: data, link }).map_err(|e| e.as_str())
    }

    /// [`Self::parse`] under the name the lazy loader had; registration is
    /// no longer lazy, so the two are one.
    #[cfg(any(test, feature = "link-at-load"))]
    pub fn register(data: &'static [u8]) -> Result<Self, &'static str> {
        Self::parse(data)
    }

    /// The link table.
    #[inline]
    pub fn link(&self) -> Link<'static> {
        // SAFETY: `linked` took the pointer from a `Link` over a `'static`
        // table, which is immutable for the program's life.
        unsafe { Link::from_raw(self.link.as_ptr()) }
    }

    /// The class bytes with their table: where every read that needs both
    /// lives.
    #[inline]
    pub fn view(&self) -> Linked<'static> {
        Linked {
            class: self.data,
            link: self.link(),
        }
    }

    /// Returns the raw bytecode slice backing this class file.
    #[inline]
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// The class name, exactly as the class file spells it. The slice is
    /// the class file's own bytes, so its address is stable for as long as
    /// the class is loaded.
    #[inline]
    pub(crate) fn scanned_name(&self) -> &'static [u8] {
        // `linked` checked the name resolves.
        self.view().name().unwrap_or(&[])
    }

    /// [`name_hash`] of the class name.
    #[inline]
    pub fn name_hash(&self) -> u32 {
        self.name_hash
    }

    /// Whether this class is named `name`, given `hash == name_hash(name)`.
    #[inline]
    pub fn is_named(&self, name: &[u8], hash: u32) -> bool {
        self.name_hash() == hash && name_eq(self.scanned_name(), name)
    }
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

/// FNV-1a over a class name — [`class_link::name_hash`], the hash every
/// table stores. Every registered class carries its hash, so a lookup by
/// name is one integer compare per class before any bytes are read. That
/// matters under `--shrink`: every framework and `java/**` class is then a
/// four-byte `a/XX` / `b/XX` name, so the length check that used to reject
/// almost every candidate passes for all of them, and a plain byte compare
/// became a `bcmp` call per class per lookup (17× the compare calls of a
/// no-shrink run, 73 % slower string benchmarks).
///
/// Out of line, like [`name_eq`] and [`find_class`]: inlined at every scan
/// site these cost 8 KB of rp2040 flash, and a local `bl` is no dearer than
/// the `bcmp` call they replace.
#[inline(never)]
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
pub const fn name_hash(name: &[u8]) -> u32 {
    class_link::name_hash(name)
}

/// Equality for the short byte strings the JVM matches on — class, method
/// and field names, descriptors. Up to 16 bytes it is an inlined loop
/// rather than a `bcmp` call, which is what a two- or four-byte shrunk name
/// wants; longer slices take the library path.
#[inline(never)]
#[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
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
