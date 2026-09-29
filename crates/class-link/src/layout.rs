// SPDX-License-Identifier: GPL-3.0-only
//! One class's link table, and how to read it in place.
//!
//! The table is a sequence of little-endian `u16` words, 4-byte aligned,
//! immutable, stored right after its class bytes. Every byte offset in it
//! is into the class bytes; every word offset is into the table itself.
//!
//! ```text
//! header, 20 words:
//!  w0  LINK_MAGIC (0x4B01: 'K', layout version 1)
//!  w1  total_words       the whole table, header included (even)
//!  w2  class_len         == the class bytes' length
//!  w3  cp_count          constant_pool_count
//!  w4  fields_len        instance fields   w5 statics_len   w6 ifaces_len
//!  w7  methods_len       w8 mrefs_len     (Methodref + InterfaceMethodref)
//!  w9  methods_off       word offset of the method records
//!  w10 mrefs_off         word offset of the Methodref descriptors
//!  w11 bsm_off           byte offset of the BootstrapMethods body, 0 = none
//!  w12 access_flags
//!  w13 name_off          byte offset of this class's name Utf8 data ([u16 len][bytes])
//!  w14 super_off         the same for the superclass name; 0 = no superclass
//!                        (java/lang/Object itself — the runtime treats a
//!                        super whose hash is Object's the same way)
//!  w15 source_file_idx   CP index of the SourceFile Utf8, 0 = none
//!  w16-17 name_hash      u32, low word first
//!  w18-19 super_hash     u32, 0 when super_off == 0
//! regions, in this order:
//!  cp_words[cp_count]        entry i's data offset (after its tag byte); for
//!                            a Methodref, the WORD offset of its descriptor
//!  tags[(cp_count+1)/2]      the tags, two a word, entry i at byte i
//!  fields[fields_len]        FieldInfo, 2 words
//!  statics[statics_len]      FieldInfo, 2 words
//!  ifaces[ifaces_len]        IfaceInfo, 3 words
//!  methods[methods_len]      MethodInfo, 6 words
//!  mrefs[mrefs_len]          MethodrefDesc, 2 words
//!  [pad word]                to an even total
//! ```
//!
//! What is *not* here is read from the class bytes at a fixed distance from
//! something that is: a method's `max_stack`, `max_locals` and
//! `code_length` sit 8, 6 and 4 bytes before its `code_offset`; its name and
//! descriptor indices 2 and 4 bytes after its `info_offset`.

use crate::classfile::{be16, be32, is_methodref, utf8_at, TAG_CLASS, TAG_UTF8};
use crate::error::LinkError;

/// `'K'` in the low byte, layout version 1 in the high byte.
pub const LINK_MAGIC: u16 = 0x4B01;
/// Header length, in words.
pub const HEADER_WORDS: usize = 20;

/// [`MethodrefDesc`] flag: the entry is an `InterfaceMethodref`.
pub const MREF_INTERFACE: u8 = 1 << 0;

mod hdr {
    pub const MAGIC: usize = 0;
    pub const TOTAL_WORDS: usize = 1;
    pub const CLASS_LEN: usize = 2;
    pub const CP_COUNT: usize = 3;
    pub const FIELDS_LEN: usize = 4;
    pub const STATICS_LEN: usize = 5;
    pub const IFACES_LEN: usize = 6;
    pub const METHODS_LEN: usize = 7;
    pub const MREFS_LEN: usize = 8;
    pub const METHODS_OFF: usize = 9;
    pub const MREFS_OFF: usize = 10;
    pub const BSM_OFF: usize = 11;
    pub const ACCESS_FLAGS: usize = 12;
    pub const NAME_OFF: usize = 13;
    pub const SUPER_OFF: usize = 14;
    pub const SOURCE_FILE_IDX: usize = 15;
    pub const NAME_HASH: usize = 16;
    pub const SUPER_HASH: usize = 18;
}

/// A record stored inline in the table.
///
/// # Safety
/// Implementors are `#[repr(C)]` structs made only of `u16` fields: size a
/// multiple of two, alignment two, no padding, every bit pattern valid.
/// That is what lets a `&[u16]` region be viewed as `&[T]`.
pub unsafe trait WordRecord: Copy {
    const WORDS: usize;
}

/// A field this class declares. Indices into the constant pool.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
}

/// A directly implemented interface.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IfaceInfo {
    /// Byte offset of the interface name's Utf8 data in the class bytes.
    pub utf8_off: u16,
    hash_lo: u16,
    hash_hi: u16,
}

impl IfaceInfo {
    /// [`crate::name_hash`] of the interface name.
    #[inline]
    pub fn hash(&self) -> u32 {
        self.hash_lo as u32 | (self.hash_hi as u32) << 16
    }
}

/// A method this class declares.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodInfo {
    /// Byte offset of the `method_info` structure in the class bytes:
    /// `access_flags`, then the name and descriptor indices.
    pub info_offset: u16,
    /// Byte offset of the bytecode; 0 for a method without a `Code`
    /// attribute (native or abstract). `max_stack`, `max_locals` and
    /// `code_length` precede it at −8, −6 and −4.
    pub code_offset: u16,
    pub access_flags: u16,
    /// Byte offset of the `LineNumberTable` body (`u16` count then
    /// entries); 0 when the method has none.
    pub lnt_offset: u16,
    sig_lo: u16,
    sig_hi: u16,
}

impl MethodInfo {
    /// [`crate::sig_hash`] of the method's name and descriptor.
    #[inline]
    pub fn sig_hash(&self) -> u32 {
        self.sig_lo as u32 | (self.sig_hi as u32) << 16
    }

    /// Has bytecode (a `Code` attribute).
    #[inline]
    pub fn has_code(&self) -> bool {
        self.code_offset != 0
    }
}

/// What an invoke site needs before it resolves: the `Methodref`'s own
/// position and its argument count.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodrefDesc {
    /// Byte offset of the `Methodref` entry's data in the class bytes.
    pub cp_off: u16,
    argc_flags: u16,
}

impl MethodrefDesc {
    /// Parameters, excluding `this`.
    #[inline]
    pub fn argc(&self) -> u8 {
        self.argc_flags as u8
    }

    #[inline]
    pub fn flags(&self) -> u8 {
        (self.argc_flags >> 8) as u8
    }

    /// The entry is an `InterfaceMethodref`.
    #[inline]
    pub fn is_interface(&self) -> bool {
        self.flags() & MREF_INTERFACE != 0
    }
}

unsafe impl WordRecord for FieldInfo {
    const WORDS: usize = 2;
}
unsafe impl WordRecord for IfaceInfo {
    const WORDS: usize = 3;
}
unsafe impl WordRecord for MethodInfo {
    const WORDS: usize = 6;
}
unsafe impl WordRecord for MethodrefDesc {
    const WORDS: usize = 2;
}

const _: () = assert!(core::mem::size_of::<FieldInfo>() == 4);
const _: () = assert!(core::mem::size_of::<IfaceInfo>() == 6);
const _: () = assert!(core::mem::size_of::<MethodInfo>() == 12);
const _: () = assert!(core::mem::size_of::<MethodrefDesc>() == 4);
const _: () = assert!(core::mem::align_of::<MethodInfo>() == 2);

/// View a word region as records. The region's length must be a multiple
/// of the record size (the header's counts make it so).
#[inline]
fn words_as<T: WordRecord>(w: &[u16]) -> &[T] {
    debug_assert!(w.len().is_multiple_of(T::WORDS));
    // SAFETY: `WordRecord`'s contract — `T` is a `#[repr(C)]` record of
    // `u16`s, so alignment 2 (that of `w`), no padding and no invalid bit
    // patterns; the length is the region's, in whole records.
    unsafe { core::slice::from_raw_parts(w.as_ptr().cast::<T>(), w.len() / T::WORDS) }
}

/// One class's link table, read in place.
#[derive(Clone, Copy, Debug)]
pub struct Link<'a> {
    words: &'a [u16],
}

impl<'a> Link<'a> {
    /// A table from its words. Checks the header's magic and that
    /// `total_words` is the slice's length — nothing deeper; that is
    /// [`Self::validate`]'s job, done once when a set is built or installed.
    pub fn new(words: &'a [u16]) -> Result<Self, LinkError> {
        if words.len() < HEADER_WORDS
            || words[hdr::MAGIC] != LINK_MAGIC
            || words[hdr::TOTAL_WORDS] as usize != words.len()
        {
            return Err(LinkError::BadHeader);
        }
        // The regions must tile the table in order, so that every slice an
        // accessor takes is in bounds by construction.
        let l = Link { words };
        let cp_end = HEADER_WORDS + l.cp_count() + crate::classfile::tag_words(l.cp_count());
        let members_end = cp_end + 2 * l.fields_len() + 2 * l.statics_len() + 3 * l.ifaces_len();
        if members_end != l.methods_off()
            || l.methods_off() + 6 * l.methods_len() != l.mrefs_off()
            || l.mrefs_off() + 2 * l.mrefs_len() > words.len()
        {
            return Err(LinkError::BadHeader);
        }
        Ok(l)
    }

    /// A table from its bytes, which must be 2-byte aligned (the section
    /// writer 4-aligns them).
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, LinkError> {
        if !(bytes.as_ptr() as usize).is_multiple_of(2) {
            return Err(LinkError::Misaligned);
        }
        if !bytes.len().is_multiple_of(2) {
            return Err(LinkError::BadHeader);
        }
        // SAFETY: 2-aligned, even length; every bit pattern is a valid
        // `u16`; the lifetime is the bytes'. Little-endian target only
        // (asserted at the crate root), so the words read as written.
        let words =
            unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast::<u16>(), bytes.len() / 2) };
        Self::new(words)
    }

    /// A table from a pointer to its first word.
    ///
    /// # Safety
    /// `ptr` must point at a table that [`Self::new`] accepted, alive for
    /// `'a`; the table's `total_words` (word 1) is its length.
    pub unsafe fn from_raw(ptr: *const u16) -> Self {
        // SAFETY: the caller's contract; word 1 is within any table.
        let total = unsafe { *ptr.add(hdr::TOTAL_WORDS) } as usize;
        // SAFETY: as above, `total` words starting at `ptr` are the table.
        Link {
            words: unsafe { core::slice::from_raw_parts(ptr, total) },
        }
    }

    /// The table's words.
    #[inline]
    pub fn words(&self) -> &'a [u16] {
        self.words
    }

    /// Pointer to the first word — what a 12-byte class-table entry keeps.
    #[inline]
    pub fn as_ptr(&self) -> *const u16 {
        self.words.as_ptr()
    }

    #[inline]
    fn word32(&self, w: usize) -> u32 {
        self.words[w] as u32 | (self.words[w + 1] as u32) << 16
    }

    #[inline]
    pub fn total_words(&self) -> usize {
        self.words[hdr::TOTAL_WORDS] as usize
    }
    #[inline]
    pub fn class_len(&self) -> usize {
        self.words[hdr::CLASS_LEN] as usize
    }
    #[inline]
    pub fn cp_count(&self) -> usize {
        self.words[hdr::CP_COUNT] as usize
    }
    #[inline]
    pub fn fields_len(&self) -> usize {
        self.words[hdr::FIELDS_LEN] as usize
    }
    #[inline]
    pub fn statics_len(&self) -> usize {
        self.words[hdr::STATICS_LEN] as usize
    }
    #[inline]
    pub fn ifaces_len(&self) -> usize {
        self.words[hdr::IFACES_LEN] as usize
    }
    #[inline]
    pub fn methods_len(&self) -> usize {
        self.words[hdr::METHODS_LEN] as usize
    }
    #[inline]
    pub fn mrefs_len(&self) -> usize {
        self.words[hdr::MREFS_LEN] as usize
    }
    #[inline]
    pub fn methods_off(&self) -> usize {
        self.words[hdr::METHODS_OFF] as usize
    }
    #[inline]
    pub fn mrefs_off(&self) -> usize {
        self.words[hdr::MREFS_OFF] as usize
    }
    /// Byte offset of the `BootstrapMethods` body; 0 = none.
    #[inline]
    pub fn bsm_off(&self) -> usize {
        self.words[hdr::BSM_OFF] as usize
    }
    #[inline]
    pub fn access_flags(&self) -> u16 {
        self.words[hdr::ACCESS_FLAGS]
    }
    /// Byte offset of this class's name Utf8 data.
    #[inline]
    pub fn name_off(&self) -> usize {
        self.words[hdr::NAME_OFF] as usize
    }
    /// Byte offset of the superclass name Utf8 data; 0 = no superclass.
    #[inline]
    pub fn super_off(&self) -> usize {
        self.words[hdr::SUPER_OFF] as usize
    }
    /// CP index of the `SourceFile` Utf8; 0 = none.
    #[inline]
    pub fn source_file_idx(&self) -> u16 {
        self.words[hdr::SOURCE_FILE_IDX]
    }
    /// [`crate::name_hash`] of this class's name.
    #[inline]
    pub fn name_hash(&self) -> u32 {
        self.word32(hdr::NAME_HASH)
    }
    /// [`crate::name_hash`] of the superclass name; 0 when there is none.
    #[inline]
    pub fn super_hash(&self) -> u32 {
        self.word32(hdr::SUPER_HASH)
    }

    #[inline]
    fn fields_off(&self) -> usize {
        HEADER_WORDS + self.cp_count() + crate::classfile::tag_words(self.cp_count())
    }
    #[inline]
    fn statics_off(&self) -> usize {
        self.fields_off() + 2 * self.fields_len()
    }
    #[inline]
    fn ifaces_off(&self) -> usize {
        self.statics_off() + 2 * self.statics_len()
    }

    /// Tag of constant-pool entry `i`; `None` past the pool. 0 for index 0
    /// and the pad slot after a `Long`/`Double`.
    #[inline]
    pub fn cp_tag(&self, i: usize) -> Option<u8> {
        if i >= self.cp_count() {
            return None;
        }
        let w = self.words[HEADER_WORDS + self.cp_count() + i / 2];
        Some(if i.is_multiple_of(2) {
            w as u8
        } else {
            (w >> 8) as u8
        })
    }

    /// The raw table word for entry `i`.
    #[inline]
    pub fn cp_word(&self, i: usize) -> Option<u16> {
        if i >= self.cp_count() {
            return None;
        }
        Some(self.words[HEADER_WORDS + i])
    }

    /// Byte offset of entry `i`'s data in the class bytes (0 for a tag-0
    /// slot). A `Methodref`'s is read through its descriptor.
    #[inline]
    pub fn cp_offset(&self, i: usize) -> Option<usize> {
        let tag = self.cp_tag(i)?;
        let w = self.words[HEADER_WORDS + i];
        if is_methodref(tag) {
            Some(self.desc_at_word(w as usize)?.cp_off as usize)
        } else {
            Some(w as usize)
        }
    }

    #[inline]
    fn desc_at_word(&self, w: usize) -> Option<&'a MethodrefDesc> {
        let base = self.mrefs_off();
        if w < base || !(w - base).is_multiple_of(2) {
            return None;
        }
        self.methodrefs().get((w - base) / 2)
    }

    /// The descriptor of `Methodref` / `InterfaceMethodref` entry `i`.
    #[inline]
    pub fn methodref_desc(&self, i: usize) -> Option<&'a MethodrefDesc> {
        if !is_methodref(self.cp_tag(i)?) {
            return None;
        }
        self.desc_at_word(self.words[HEADER_WORDS + i] as usize)
    }

    #[inline]
    pub fn fields(&self) -> &'a [FieldInfo] {
        let off = self.fields_off();
        words_as(&self.words[off..off + 2 * self.fields_len()])
    }

    #[inline]
    pub fn static_fields(&self) -> &'a [FieldInfo] {
        let off = self.statics_off();
        words_as(&self.words[off..off + 2 * self.statics_len()])
    }

    #[inline]
    pub fn interfaces(&self) -> &'a [IfaceInfo] {
        let off = self.ifaces_off();
        words_as(&self.words[off..off + 3 * self.ifaces_len()])
    }

    #[inline]
    pub fn methods(&self) -> &'a [MethodInfo] {
        let off = self.methods_off();
        words_as(&self.words[off..off + 6 * self.methods_len()])
    }

    #[inline]
    pub fn methodrefs(&self) -> &'a [MethodrefDesc] {
        let off = self.mrefs_off();
        words_as(&self.words[off..off + 2 * self.mrefs_len()])
    }

    /// Position of method `m` in [`Self::methods`].
    #[inline]
    pub fn method_index(&self, m: &MethodInfo) -> Option<usize> {
        let base = self.methods().as_ptr() as usize;
        let at = m as *const MethodInfo as usize;
        if at < base {
            return None;
        }
        let i = (at - base) / core::mem::size_of::<MethodInfo>();
        (i < self.methods_len()).then_some(i)
    }
}

/// A class with its table: the class bytes and the [`Link`] over them,
/// which is where every read that needs both lives.
#[derive(Clone, Copy, Debug)]
pub struct Linked<'a> {
    pub class: &'a [u8],
    pub link: Link<'a>,
}

impl<'a> Linked<'a> {
    /// The bytes of `Utf8` entry `i`.
    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn utf8(&self, i: usize) -> Option<&'a [u8]> {
        if self.link.cp_tag(i)? != TAG_UTF8 {
            return None;
        }
        utf8_at(self.class, self.link.cp_offset(i)?)
    }

    /// This class's name, as the class file spells it.
    #[inline]
    pub fn name(&self) -> Option<&'a [u8]> {
        utf8_at(self.class, self.link.name_off())
    }

    /// The superclass name, `None` when the class has none (it is
    /// `java/lang/Object`). A class that extends Object *does* report
    /// Object here; the runtime recognises that by hash.
    #[inline]
    pub fn super_name(&self) -> Option<&'a [u8]> {
        let off = self.link.super_off();
        if off == 0 {
            return None;
        }
        utf8_at(self.class, off)
    }

    /// Name of interface record `f`.
    #[inline]
    pub fn interface_name(&self, f: &IfaceInfo) -> Option<&'a [u8]> {
        utf8_at(self.class, f.utf8_off as usize)
    }

    /// The name a `Class` entry `i` refers to.
    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_class_name(&self, i: usize) -> Option<&'a [u8]> {
        if self.link.cp_tag(i)? != TAG_CLASS {
            return None;
        }
        let ui = be16(self.class, self.link.cp_offset(i)?)? as usize;
        self.utf8(ui)
    }

    /// `(class name, member name, descriptor)` of a `Methodref`,
    /// `InterfaceMethodref` or `Fieldref` entry `i`: the three share one
    /// layout (JVMS §4.4.2).
    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_member_ref(&self, i: usize) -> Option<(&'a [u8], &'a [u8], &'a [u8])> {
        let tag = self.link.cp_tag(i)?;
        if !matches!(tag, crate::classfile::TAG_FIELDREF) && !is_methodref(tag) {
            return None;
        }
        let off = self.link.cp_offset(i)?;
        let class_idx = be16(self.class, off)? as usize;
        let nat_idx = be16(self.class, off + 2)? as usize;
        let class_name = self.cp_class_name(class_idx)?;
        let (name, desc) = self.cp_name_and_type(nat_idx)?;
        Some((class_name, name, desc))
    }

    /// `(name, descriptor)` of a `NameAndType` entry `i`.
    #[cfg_attr(not(feature = "hot-in-ram"), inline)]
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_name_and_type(&self, i: usize) -> Option<(&'a [u8], &'a [u8])> {
        if self.link.cp_tag(i)? != crate::classfile::TAG_NAME_AND_TYPE {
            return None;
        }
        let off = self.link.cp_offset(i)?;
        let name_idx = be16(self.class, off)? as usize;
        let desc_idx = be16(self.class, off + 2)? as usize;
        Some((self.utf8(name_idx)?, self.utf8(desc_idx)?))
    }

    /// The `Utf8` a `String` entry `i` refers to.
    #[inline]
    pub fn cp_string_utf8(&self, i: usize) -> Option<&'a [u8]> {
        if self.link.cp_tag(i)? != crate::classfile::TAG_STRING {
            return None;
        }
        let ui = be16(self.class, self.link.cp_offset(i)?)? as usize;
        self.utf8(ui)
    }

    /// Constant-pool index of method `m`'s name.
    #[inline]
    pub fn method_name_index(&self, m: &MethodInfo) -> Option<u16> {
        be16(self.class, m.info_offset as usize + 2)
    }

    /// Constant-pool index of method `m`'s descriptor.
    #[inline]
    pub fn method_descriptor_index(&self, m: &MethodInfo) -> Option<u16> {
        be16(self.class, m.info_offset as usize + 4)
    }

    #[inline]
    pub fn method_name(&self, m: &MethodInfo) -> Option<&'a [u8]> {
        self.utf8(self.method_name_index(m)? as usize)
    }

    #[inline]
    pub fn method_descriptor(&self, m: &MethodInfo) -> Option<&'a [u8]> {
        self.utf8(self.method_descriptor_index(m)? as usize)
    }

    /// `max_stack` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_max_stack(&self, m: &MethodInfo) -> u16 {
        if m.code_offset == 0 {
            return 0;
        }
        be16(self.class, m.code_offset as usize - 8).unwrap_or(0)
    }

    /// `max_locals` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_max_locals(&self, m: &MethodInfo) -> u16 {
        if m.code_offset == 0 {
            return 0;
        }
        be16(self.class, m.code_offset as usize - 6).unwrap_or(0)
    }

    /// `code_length` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_code_len(&self, m: &MethodInfo) -> usize {
        if m.code_offset == 0 {
            return 0;
        }
        be32(self.class, m.code_offset as usize - 4).unwrap_or(0) as usize
    }

    /// The bytecode of method `m`; empty for a method without any.
    #[inline]
    pub fn method_code(&self, m: &MethodInfo) -> &'a [u8] {
        let start = m.code_offset as usize;
        self.class
            .get(start..start + self.method_code_len(m))
            .unwrap_or(&[])
    }

    #[inline]
    pub fn field_name(&self, f: &FieldInfo) -> Option<&'a [u8]> {
        self.utf8(f.name_index as usize)
    }

    #[inline]
    pub fn field_descriptor(&self, f: &FieldInfo) -> Option<&'a [u8]> {
        self.utf8(f.descriptor_index as usize)
    }
}
