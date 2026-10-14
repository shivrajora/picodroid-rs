// SPDX-License-Identifier: GPL-3.0-only
//! The builder (feature `build`): link tables, class indices and whole
//! class sections, for the packer and the firmware build.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::classfile::{
    be16, derive, pool_utf8, survey, walk_cp, CpView, Layout, Sink, Utf8Form, MAX_POOL_BYTES,
    POOL_ALIGN, TAG_UTF8,
};
use crate::error::LinkError;
use crate::hash::{name_hash, string_hash};
use crate::layout::{Link, Linked, HEADER_WORDS, POOL_REL_WORD, POOL_WORDS_WORD, SUPER_IDX_WORD};
use crate::section::{
    ClassSection, IndexEntry, DIRECTORY_ENTRY_LEN, INDEX_ENTRY_LEN, LITERAL_LEN, LIT_UTF8,
    MAX_CLASSES, MAX_LITERALS, SECTION_HEADER_LEN,
};

/// The constant pool as walked from a pooled class: each entry's data
/// offset (a `Utf8`'s ref) and tag, and the pool the refs name.
struct VecCp<'p> {
    offsets: Vec<u16>,
    tags: Vec<u8>,
    pool: &'p [u8],
}

impl CpView for VecCp<'_> {
    fn cp_count(&self) -> usize {
        self.tags.len()
    }
    fn tag(&self, i: usize) -> Option<u8> {
        self.tags.get(i).copied()
    }
    fn offset(&self, i: usize) -> Option<usize> {
        self.offsets.get(i).map(|&o| o as usize)
    }
    fn pool(&self) -> &[u8] {
        self.pool
    }
}

struct VecSink<'a>(&'a mut [u16]);

impl Sink for VecSink<'_> {
    fn put(&mut self, word: usize, value: u16) -> Result<(), LinkError> {
        *self.0.get_mut(word).ok_or(LinkError::Internal)? = value;
        Ok(())
    }

    fn put_external(&mut self, word: usize, placeholder: u16) -> Result<(), LinkError> {
        self.put(word, placeholder)
    }
}

/// A set's `Utf8` pool under construction: each distinct byte string once,
/// in order of first appearance, entry 0 the empty string.
struct Pool<'c> {
    bytes: Vec<u8>,
    seen: BTreeMap<&'c [u8], u16>,
}

impl<'c> Pool<'c> {
    fn new() -> Self {
        let mut seen = BTreeMap::new();
        seen.insert(&[][..], 0);
        Pool {
            bytes: alloc::vec![0; POOL_ALIGN],
            seen,
        }
    }

    /// The ref of `s`, added if it is new.
    fn intern(&mut self, s: &'c [u8]) -> Result<u16, LinkError> {
        if let Some(&r) = self.seen.get(s) {
            return Ok(r);
        }
        let at = self.bytes.len();
        if (at + 2 + s.len()).next_multiple_of(POOL_ALIGN) > MAX_POOL_BYTES {
            return Err(LinkError::PoolTooLarge);
        }
        let r = (at / POOL_ALIGN) as u16;
        self.bytes
            .extend_from_slice(&(s.len() as u16).to_be_bytes());
        self.bytes.extend_from_slice(s);
        pad_to(&mut self.bytes, POOL_ALIGN);
        self.seen.insert(s, r);
        Ok(r)
    }
}

/// `class`, a standard class file, as a section holds it: each `Utf8`
/// entry's bytes moved into `pool` and its tag byte left behind. Returns
/// the pooled bytes and each `Utf8` entry's ref, in constant-pool order.
fn pool_class<'c>(class: &'c [u8], pool: &mut Pool<'c>) -> Result<(Vec<u8>, Vec<u16>), LinkError> {
    // The `[u16 len][bytes]` span of each Utf8 entry. The walk checks
    // every span lies in the class before it returns.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    walk_cp(class, Utf8Form::Inline, |_, tag, off| {
        if tag == TAG_UTF8 {
            let len = be16(class, off).ok_or(LinkError::Truncated)? as usize;
            spans.push((off, off + 2 + len));
        }
        Ok(())
    })?;
    let mut out: Vec<u8> = Vec::with_capacity(class.len());
    let mut refs: Vec<u16> = Vec::with_capacity(spans.len());
    let mut at = 0;
    for (start, end) in spans {
        out.extend_from_slice(&class[at..start]);
        refs.push(pool.intern(&class[start + 2..end])?);
        at = end;
    }
    out.extend_from_slice(&class[at..]);
    Ok((out, refs))
}

/// The table of a pooled class whose `Utf8` entries are `refs` into
/// `pool`, with the section's words at their placeholders.
fn link_pooled(class: &[u8], refs: &[u16], pool: &[u8]) -> Result<Vec<u16>, LinkError> {
    let mut cp = VecCp {
        offsets: Vec::new(),
        tags: Vec::new(),
        pool,
    };
    let mut k = 0;
    let (cp_count, pos_after_cp) = walk_cp(class, Utf8Form::Pooled, |_, tag, off| {
        let word = if tag == TAG_UTF8 {
            k += 1;
            *refs.get(k - 1).ok_or(LinkError::Internal)?
        } else {
            off as u16
        };
        cp.tags.push(tag);
        cp.offsets.push(word);
        Ok(())
    })?;
    let counts = survey(class, &cp, cp_count, pos_after_cp)?;
    let layout = Layout::new(&counts)?;
    let mut words: Vec<u16> = Vec::new();
    words
        .try_reserve_exact(layout.total_words)
        .map_err(|_| LinkError::OutOfMemory)?;
    words.resize(layout.total_words, 0);
    derive(class, &cp, &counts, &layout, &mut VecSink(&mut words))?;
    debug_assert!(Link::new(&words, pool).is_ok());
    Ok(words)
}

/// Link one class: a section of it alone. Fails on anything the runtime
/// could not rely on (see [`LinkError`]).
pub fn link_class(class: &[u8]) -> Result<Vec<u8>, LinkError> {
    build_section(&[class])
}

/// The standard class file a section's pooled class came from: each
/// `Utf8` entry's bytes put back after its tag. What a tool that relinks a
/// section's classes (`papk-pack --repack`) or compares them with their
/// source reads.
pub fn unpooled_class(linked: &Linked<'_>) -> Result<Vec<u8>, LinkError> {
    let class = linked.class;
    let mut inserts: Vec<(usize, &[u8])> = Vec::new();
    walk_cp(class, Utf8Form::Pooled, |i, tag, off| {
        if tag == TAG_UTF8 {
            let bytes = linked.utf8(i).ok_or(LinkError::BadOffset {
                word: (HEADER_WORDS + i) as u16,
            })?;
            inserts.push((off, bytes));
        }
        Ok(())
    })?;
    let extra: usize = inserts.iter().map(|(_, b)| 2 + b.len()).sum();
    let mut out: Vec<u8> = Vec::with_capacity(class.len() + extra);
    let mut at = 0;
    for (off, bytes) in inserts {
        out.extend_from_slice(&class[at..off]);
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(bytes);
        at = off;
    }
    out.extend_from_slice(&class[at..]);
    Ok(out)
}

/// A table's words as little-endian bytes.
pub fn link_bytes(words: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 * words.len());
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// The index of a set of class names: one entry per name, sorted by
/// `(hash, idx)`. Two names that hash alike are refused — the set could
/// not be searched by hash — as are two equal names.
pub fn class_index(names: &[&[u8]]) -> Result<Vec<IndexEntry>, LinkError> {
    if names.len() > MAX_CLASSES {
        return Err(LinkError::TooManyClasses);
    }
    let mut index: Vec<IndexEntry> = names
        .iter()
        .enumerate()
        .map(|(i, n)| IndexEntry {
            hash: name_hash(n),
            idx: i as u16,
            pad: 0,
        })
        .collect();
    index.sort_unstable_by_key(|e| (e.hash, e.idx));
    for pair in index.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.hash == b.hash {
            return Err(if names[a.idx as usize] == names[b.idx as usize] {
                LinkError::DuplicateClass { a: a.idx, b: b.idx }
            } else {
                LinkError::HashCollision { a: a.idx, b: b.idx }
            });
        }
    }
    Ok(index)
}

/// A class file with no members: `class <name> extends java/lang/Object`,
/// public. For tests and fixtures that need several distinct, linkable
/// classes without a compiler; `name` is the JVM internal name.
pub fn minimal_class(name: &[u8]) -> Vec<u8> {
    simple_class(name, b"java/lang/Object", None)
}

/// A public `class <name> extends <sup>` with no fields and, when `code`
/// is given, one `static int m()` whose `Code` is exactly those bytes
/// (`max_stack` 4, `max_locals` 4, no exception table). For tests and
/// fixtures; nothing checks that the bytecode makes sense.
pub fn simple_class(name: &[u8], sup: &[u8], code: Option<&[u8]>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(96 + name.len() + sup.len());
    out.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34]);
    let cp_count: u16 = if code.is_some() { 8 } else { 5 };
    out.extend_from_slice(&cp_count.to_be_bytes()); // entries #1..#(cp_count - 1)
    out.extend_from_slice(&[0x07, 0x00, 0x02]); // #1 Class -> #2
    out.push(0x01); // #2 Utf8 name
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&[0x07, 0x00, 0x04]); // #3 Class -> #4
    out.push(0x01); // #4 Utf8 superclass
    out.extend_from_slice(&(sup.len() as u16).to_be_bytes());
    out.extend_from_slice(sup);
    if code.is_some() {
        out.extend_from_slice(&[0x01, 0x00, 0x01, b'm']); // #5 Utf8 "m"
        out.extend_from_slice(&[0x01, 0x00, 0x03, b'(', b')', b'I']); // #6 Utf8 "()I"
        out.extend_from_slice(&[0x01, 0x00, 0x04, b'C', b'o', b'd', b'e']); // #7 Utf8 "Code"
    }
    out.extend_from_slice(&0x0021u16.to_be_bytes()); // ACC_PUBLIC | ACC_SUPER
    out.extend_from_slice(&1u16.to_be_bytes()); // this_class
    out.extend_from_slice(&3u16.to_be_bytes()); // super_class
    out.extend_from_slice(&[0, 0, 0, 0]); // no interfaces, no fields
    match code {
        None => out.extend_from_slice(&[0, 0]),
        Some(code) => {
            out.extend_from_slice(&1u16.to_be_bytes()); // methods_count
            out.extend_from_slice(&0x0008u16.to_be_bytes()); // ACC_STATIC
            out.extend_from_slice(&[0x00, 0x05, 0x00, 0x06, 0x00, 0x01]); // name, desc, 1 attr
            out.extend_from_slice(&[0x00, 0x07]); // Code
            out.extend_from_slice(&((12 + code.len()) as u32).to_be_bytes());
            out.extend_from_slice(&[0x00, 0x04, 0x00, 0x04]); // max_stack, max_locals
            out.extend_from_slice(&(code.len() as u32).to_be_bytes());
            out.extend_from_slice(code);
            out.extend_from_slice(&[0, 0, 0, 0]); // no exception table, no sub-attributes
        }
    }
    out.extend_from_slice(&[0, 0]); // no class attributes
    out
}

fn pad_to(out: &mut Vec<u8>, align: usize) {
    while !out.len().is_multiple_of(align) {
        out.push(0);
    }
}

/// A class section over `classes`: each linked, the index built, the whole
/// laid out as [`crate::section`] describes. The result parses and
/// validates as a [`ClassSection`].
pub fn build_section(classes: &[&[u8]]) -> Result<Vec<u8>, LinkError> {
    build_section_impl(classes, None)
}

/// [`build_section`] for classes handed over with the name each is expected
/// to spell — a packer names them by path — refusing one whose own name
/// differs ([`LinkError::NameMismatch`]).
pub fn build_section_named(classes: &[(&[u8], &[u8])]) -> Result<Vec<u8>, LinkError> {
    let bytes: Vec<&[u8]> = classes.iter().map(|(_, c)| *c).collect();
    let names: Vec<&[u8]> = classes.iter().map(|(n, _)| *n).collect();
    build_section_impl(&bytes, Some(&names))
}

fn build_section_impl(classes: &[&[u8]], expected: Option<&[&[u8]]>) -> Result<Vec<u8>, LinkError> {
    let n = classes.len();
    if n > MAX_CLASSES {
        return Err(LinkError::TooManyClasses);
    }
    // Every Utf8 of the set into one pool first, then each class linked
    // against the finished pool.
    let mut interner = Pool::new();
    let mut pooled: Vec<(Vec<u8>, Vec<u16>)> = Vec::with_capacity(n);
    for class in classes {
        pooled.push(pool_class(class, &mut interner)?);
    }
    let pool = interner.bytes;
    let mut links: Vec<Vec<u16>> = Vec::with_capacity(n);
    for (class, refs) in &pooled {
        links.push(link_pooled(class, refs, &pool)?);
    }
    // Names, read through the pool so that they borrow it and not the
    // tables, which are patched below.
    let mut names: Vec<&[u8]> = Vec::with_capacity(n);
    for (i, words) in links.iter().enumerate() {
        let name =
            pool_utf8(&pool, Link::new(words, &pool)?.name_ref()).ok_or(LinkError::Internal)?;
        if expected.is_some_and(|e| e.get(i).copied() != Some(name)) {
            return Err(LinkError::NameMismatch { idx: i as u16 });
        }
        names.push(name);
    }
    let index = class_index(&names)?;

    // Where the section holds each class's superclass, if it does.
    for words in links.iter_mut() {
        let super_ref = Link::new(words, &pool)?.super_ref();
        let sup = (super_ref != 0)
            .then(|| pool_utf8(&pool, super_ref))
            .flatten()
            .and_then(|s| names.iter().position(|n| *n == s));
        if let Some(j) = sup {
            words[SUPER_IDX_WORD] = j as u16;
        }
    }
    crate::section::check_super_chains(n, |i| Link::new(&links[i], &pool).ok()?.super_idx())?;

    // The literal pool: every String constant of the set, each distinct
    // byte string once, in order of first appearance. A row points at the
    // bytes in the Utf8 pool; each class's String descriptors get their
    // row's number.
    struct Row<'p> {
        bytes: &'p [u8],
        /// The bytes' Utf8 ref.
        r: u16,
    }
    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut seen: BTreeMap<&[u8], u16> = BTreeMap::new();
    for (i, (class, _)) in pooled.iter().enumerate() {
        let link = Link::new(&links[i], &pool)?;
        let strs_off = link.strs_off();
        let mut ids: Vec<u16> = Vec::with_capacity(link.strs_len());
        for d in link.strings() {
            // `derive` checked the entry names a Utf8.
            let ui = be16(class, d.cp_off as usize).ok_or(LinkError::Internal)? as usize;
            let r = link.cp_word(ui).ok_or(LinkError::Internal)?;
            let bytes = pool_utf8(&pool, r as usize).ok_or(LinkError::Internal)?;
            let id = match seen.get(bytes) {
                Some(&id) => id,
                None => {
                    if rows.len() >= MAX_LITERALS {
                        return Err(LinkError::TooManyLiterals);
                    }
                    let id = rows.len() as u16;
                    rows.push(Row { bytes, r });
                    seen.insert(bytes, id);
                    id
                }
            };
            ids.push(id);
        }
        for (k, id) in ids.into_iter().enumerate() {
            links[i][strs_off + 2 * k + 1] = id;
        }
    }
    let mut link_offs: Vec<usize> = Vec::with_capacity(n);

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(n as u32).to_le_bytes());
    // index_off, pool_off and pool_len, patched below.
    out.resize(SECTION_HEADER_LEN, 0);
    let dir_at = out.len();
    out.resize(dir_at + DIRECTORY_ENTRY_LEN * n, 0);
    for (i, ((class, _), words)) in pooled.iter().zip(&links).enumerate() {
        pad_to(&mut out, 4);
        let class_off = out.len();
        out.extend_from_slice(class);
        pad_to(&mut out, 4);
        let link_off = out.len();
        link_offs.push(link_off);
        out.extend_from_slice(&link_bytes(words));
        pad_to(&mut out, 4);
        let d = dir_at + DIRECTORY_ENTRY_LEN * i;
        out[d..d + 4].copy_from_slice(&le32_of(class_off)?);
        out[d + 4..d + 8].copy_from_slice(&le32_of(link_off)?);
    }
    pad_to(&mut out, 8);
    let index_off = out.len();
    for e in &index {
        out.extend_from_slice(&e.hash.to_le_bytes());
        out.extend_from_slice(&e.idx.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    debug_assert_eq!(out.len(), index_off + INDEX_ENTRY_LEN * n);
    // The pool follows the literal rows, which are a whole number of
    // 4-byte units after the 8-aligned index.
    let pool_off = out.len() + 4 + LITERAL_LEN * rows.len();
    debug_assert!(pool_off.is_multiple_of(POOL_ALIGN));
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for row in &rows {
        let flags = if core::str::from_utf8(row.bytes).is_ok() {
            LIT_UTF8
        } else {
            0
        };
        out.extend_from_slice(&string_hash(row.bytes).to_le_bytes());
        out.extend_from_slice(&le32_of(pool_off + POOL_ALIGN * row.r as usize + 2)?);
        out.extend_from_slice(&(row.bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
    }
    debug_assert_eq!(out.len(), pool_off);
    out.extend_from_slice(&pool);
    out[4..8].copy_from_slice(&le32_of(index_off)?);
    out[8..12].copy_from_slice(&le32_of(pool_off)?);
    out[12..16].copy_from_slice(&le32_of(pool.len())?);
    // Each table's pool words, now that the pool has its place.
    let pool_words = (pool.len() / POOL_ALIGN) as u16;
    for link_off in link_offs {
        let rel = link_off + 2 * POOL_REL_WORD;
        out[rel..rel + 4].copy_from_slice(&le32_of(pool_off - link_off)?);
        let pw = link_off + 2 * POOL_WORDS_WORD;
        out[pw..pw + 2].copy_from_slice(&pool_words.to_le_bytes());
    }
    debug_assert!(ClassSection::parse(&out).is_ok());
    Ok(out)
}

fn le32_of(v: usize) -> Result<[u8; 4], LinkError> {
    Ok(u32::try_from(v)
        .map_err(|_| LinkError::BadSection)?
        .to_le_bytes())
}
