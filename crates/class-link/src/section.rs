// SPDX-License-Identifier: GPL-3.0-only
//! A set of linked classes with a sorted index: the payload of a PAPK
//! `CLSS` section, and — the same bytes — the framework corpus embedded in
//! firmware. Little-endian; offsets are from the section's first byte.
//!
//! ```text
//! [u32 class_count][u32 index_off]
//! directory: class_count × { u32 class_off, u32 link_off }   (both 4-aligned)
//! records:   per class, in directory order:
//!              class bytes, zero pad to 4
//!              link table (class_len and total_words are in its header), pad to 4
//! index:     class_count × IndexEntry { u32 hash, u16 idx, u16 0 }, at index_off
//!            (8-aligned), sorted by (hash, idx); no two entries share a hash
//! literals:  [u32 lit_count], then lit_count × Literal { u32 hash, u32 off,
//!            u16 len, u16 flags }, right after the index
//! ```
//!
//! The literal pool is the set's `String` constants, each distinct byte
//! string once, in order of first appearance. A row points at the bytes
//! where they already are — inside the constant pool of the first class
//! that spells them — and every class's [`crate::StringDesc`] names its
//! row. An `ldc` of a string is then a row number, the same for every
//! class of the set, with no table in RAM and nothing to search.
//!
//! The section itself must sit at a 4-byte aligned address: every table and
//! the index are then aligned for in-place reads. A PAPK's sections are
//! 4-aligned in the file and the file sits on a flash sector; the firmware
//! static is declared aligned.

use crate::error::LinkError;
use crate::hash::{name_hash, string_hash};
use crate::layout::{Link, Linked};

/// `class_count` and `index_off`.
pub const SECTION_HEADER_LEN: usize = 8;
/// `class_off` and `link_off`.
pub const DIRECTORY_ENTRY_LEN: usize = 8;
pub const INDEX_ENTRY_LEN: usize = 8;
/// Bound on the classes one section indexes; [`ClassSection::validate`]
/// keeps a bitmap this big on the stack.
pub const MAX_CLASSES: usize = 4096;

/// Bound on the literals one section numbers: what leaves room for two
/// pools and the runtime's own strings in a `u16` of string references.
pub const MAX_LITERALS: usize = 0x3000;
pub const LITERAL_LEN: usize = 12;
/// [`Literal::flags`]: the bytes are valid UTF-8 (a constant with an
/// embedded NUL or a supplementary character is modified UTF-8 and is not).
pub const LIT_UTF8: u16 = 1 << 0;

/// One row of the literal pool: `{ u32 hash, u32 off, u16 len, u16 flags }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Literal {
    /// [`crate::string_hash`] of the bytes: the string's `hashCode()`.
    pub hash: u32,
    /// Offset of the bytes from the section's first byte.
    pub off: u32,
    pub len: u16,
    pub flags: u16,
}

/// A section's literal pool, read in place.
#[derive(Clone, Copy, Debug)]
pub struct Literals<'a> {
    data: &'a [u8],
    /// Offset of the first row in `data`.
    rows_off: usize,
    count: usize,
}

impl<'a> Literals<'a> {
    /// No literals: what a class set outside a section has.
    pub const EMPTY: Literals<'static> = Literals {
        data: &[],
        rows_off: 0,
        count: 0,
    };

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[inline]
    pub fn row(&self, i: usize) -> Option<Literal> {
        if i >= self.count {
            return None;
        }
        let at = self.rows_off + LITERAL_LEN * i;
        Some(Literal {
            hash: le32(self.data, at)?,
            off: le32(self.data, at + 4)?,
            len: le16(self.data, at + 8)?,
            flags: le16(self.data, at + 10)?,
        })
    }

    /// The bytes of literal `i`.
    #[inline]
    pub fn bytes(&self, i: usize) -> Option<&'a [u8]> {
        let r = self.row(i)?;
        let off = r.off as usize;
        self.data.get(off..off + r.len as usize)
    }

    /// The section these literals belong to, as bytes: a class of the set
    /// lies inside them.
    #[inline]
    pub fn section_bytes(&self) -> &'a [u8] {
        self.data
    }
}

/// One row of the class index.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    /// [`name_hash`] of the class's name.
    pub hash: u32,
    /// Position of the class in the section's directory.
    pub idx: u16,
    pub pad: u16,
}

const _: () = assert!(core::mem::size_of::<IndexEntry>() == INDEX_ENTRY_LEN);
const _: () = assert!(core::mem::align_of::<IndexEntry>() == 4);

#[inline]
fn le16(d: &[u8], off: usize) -> Option<u16> {
    let b = d.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

#[inline]
fn le32(d: &[u8], off: usize) -> Option<u32> {
    let b = d.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A class section, read in place.
#[derive(Clone, Copy, Debug)]
pub struct ClassSection<'a> {
    data: &'a [u8],
    count: usize,
    index_off: usize,
    lit_count: usize,
}

impl<'a> ClassSection<'a> {
    /// Check the header, the directory's and the index's bounds and the
    /// alignment, nothing per class; see [`Self::validate`] for that.
    pub fn parse(data: &'a [u8]) -> Result<Self, LinkError> {
        if !(data.as_ptr() as usize).is_multiple_of(4) {
            return Err(LinkError::Misaligned);
        }
        let count = le32(data, 0).ok_or(LinkError::BadSection)? as usize;
        let index_off = le32(data, 4).ok_or(LinkError::BadSection)? as usize;
        if count > MAX_CLASSES {
            return Err(LinkError::TooManyClasses);
        }
        let dir_end = SECTION_HEADER_LEN + DIRECTORY_ENTRY_LEN * count;
        let index_end = index_off
            .checked_add(INDEX_ENTRY_LEN * count)
            .ok_or(LinkError::BadSection)?;
        if dir_end > data.len() || index_off < dir_end || index_end > data.len() {
            return Err(LinkError::BadSection);
        }
        if !index_off.is_multiple_of(4) {
            return Err(LinkError::Misaligned);
        }
        let lit_count = le32(data, index_end).ok_or(LinkError::BadSection)? as usize;
        if lit_count > MAX_LITERALS {
            return Err(LinkError::TooManyLiterals);
        }
        if index_end + 4 + LITERAL_LEN * lit_count > data.len() {
            return Err(LinkError::BadSection);
        }
        Ok(Self {
            data,
            count,
            index_off,
            lit_count,
        })
    }

    /// The section's bytes.
    #[inline]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.data
    }

    /// Number of classes.
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Class `i` with its table; `None` past the end or if the record is
    /// malformed (a section that passed [`Self::validate`] has none).
    #[inline]
    pub fn class(&self, i: usize) -> Option<Linked<'a>> {
        self.class_checked(i).ok()
    }

    fn class_checked(&self, i: usize) -> Result<Linked<'a>, LinkError> {
        if i >= self.count {
            return Err(LinkError::BadSection);
        }
        let dir = SECTION_HEADER_LEN + DIRECTORY_ENTRY_LEN * i;
        let class_off = le32(self.data, dir).ok_or(LinkError::BadSection)? as usize;
        let link_off = le32(self.data, dir + 4).ok_or(LinkError::BadSection)? as usize;
        if !class_off.is_multiple_of(4) || !link_off.is_multiple_of(4) {
            return Err(LinkError::Misaligned);
        }
        // The table's header says how long it and the class are.
        let total_words = le16(self.data, link_off + 2).ok_or(LinkError::BadSection)? as usize;
        let class_len = le16(self.data, link_off + 4).ok_or(LinkError::BadSection)? as usize;
        let class = self
            .data
            .get(class_off..class_off + class_len)
            .ok_or(LinkError::BadSection)?;
        let link_bytes = self
            .data
            .get(link_off..link_off + 2 * total_words)
            .ok_or(LinkError::BadSection)?;
        let link = Link::from_bytes(link_bytes)?;
        Ok(Linked { class, link })
    }

    /// Every class, in directory order.
    pub fn classes(&self) -> impl Iterator<Item = Linked<'a>> + '_ {
        (0..self.count).filter_map(move |i| self.class(i))
    }

    /// The index, sorted by `(hash, idx)`.
    #[inline]
    pub fn index(&self) -> &'a [IndexEntry] {
        let bytes = &self.data[self.index_off..self.index_off + INDEX_ENTRY_LEN * self.count];
        // SAFETY: `parse` checked the section is 4-aligned and `index_off`
        // is a multiple of 4, so the entries are aligned for `IndexEntry`
        // (alignment 4); the length is a whole number of 8-byte entries;
        // the record is `#[repr(C)]` plain integers, every bit pattern
        // valid; little-endian target (asserted at the crate root).
        unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast::<IndexEntry>(), self.count) }
    }

    /// The literal pool.
    #[inline]
    pub fn literals(&self) -> Literals<'a> {
        Literals {
            data: self.data,
            rows_off: self.index_off + INDEX_ENTRY_LEN * self.count + 4,
            count: self.lit_count,
        }
    }

    /// The index entries whose hash is `hash` — at most one in a section
    /// that validates, found by binary search.
    pub fn find(&self, hash: u32) -> &'a [IndexEntry] {
        let index = self.index();
        let lo = index.partition_point(|e| e.hash < hash);
        let hi = lo + index[lo..].partition_point(|e| e.hash == hash);
        &index[lo..hi]
    }

    /// The directory position of the class named `name`, if the section
    /// holds one. Hash first; the bytes are compared only on a hash hit.
    pub fn find_class(&self, name: &[u8]) -> Option<usize> {
        let hash = name_hash(name);
        self.find(hash).iter().find_map(|e| {
            let idx = e.idx as usize;
            (self.class(idx)?.name()? == name).then_some(idx)
        })
    }

    /// Deep check: every class's table validates against its bytes
    /// ([`Link::validate`]), and the index is sorted, names every class
    /// exactly once with its own hash, and has no two entries with one
    /// hash; every literal row's hash and flags are its bytes', and every
    /// class's String descriptors name the row that holds their bytes, and
    /// its `super_idx` the class its superclass name spells.
    /// What the packer, the firmware build and an install run.
    pub fn validate(&self) -> Result<(), LinkError> {
        let lits = self.literals();
        for i in 0..lits.len() {
            let bytes = lits.bytes(i).ok_or(LinkError::BadLiteral)?;
            let row = lits.row(i).ok_or(LinkError::BadLiteral)?;
            let flags = if core::str::from_utf8(bytes).is_ok() {
                LIT_UTF8
            } else {
                0
            };
            if row.hash != string_hash(bytes) || row.flags != flags {
                return Err(LinkError::BadLiteral);
            }
        }
        for i in 0..self.count {
            let linked = self.class_checked(i)?;
            linked.link.validate(linked.class)?;
            if linked.name().is_none() {
                return Err(LinkError::BadOffset { word: 13 });
            }
            for d in linked.link.strings() {
                let bytes = linked.string_bytes(d).ok_or(LinkError::BadLiteral)?;
                if lits.bytes(d.lit as usize) != Some(bytes) {
                    return Err(LinkError::BadLiteral);
                }
            }
            // `super_idx` is where the section holds the superclass, and
            // nothing else: the class of that name if there is one.
            // Searched by the hash the table already carries (the table
            // validated above, so the hash is the name's).
            let expected = linked.super_name().and_then(|n| {
                self.find(linked.link.super_hash()).iter().find_map(|e| {
                    let idx = e.idx as usize;
                    (self.class(idx)?.name()? == n).then_some(idx)
                })
            });
            if linked.link.super_idx() != expected {
                return Err(LinkError::BadSuperIndex { idx: i as u16 });
            }
        }
        let index = self.index();
        let mut seen = [0u64; MAX_CLASSES / 64];
        for (n, e) in index.iter().enumerate() {
            let idx = e.idx as usize;
            if idx >= self.count || e.pad != 0 {
                return Err(LinkError::BadIndex);
            }
            let (w, b) = (idx / 64, idx % 64);
            if seen[w] & (1 << b) != 0 {
                return Err(LinkError::BadIndex);
            }
            seen[w] |= 1 << b;
            let linked = self.class_checked(idx)?;
            if linked.link.name_hash() != e.hash {
                return Err(LinkError::BadIndex);
            }
            if n > 0 {
                let prev = index[n - 1];
                if prev.hash > e.hash {
                    return Err(LinkError::BadIndex);
                }
                if prev.hash == e.hash {
                    let a = self.class_checked(prev.idx as usize)?;
                    return Err(if a.name() == linked.name() {
                        LinkError::DuplicateClass {
                            a: prev.idx,
                            b: e.idx,
                        }
                    } else {
                        LinkError::HashCollision {
                            a: prev.idx,
                            b: e.idx,
                        }
                    });
                }
            }
        }
        Ok(())
    }
}
