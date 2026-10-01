// SPDX-License-Identifier: GPL-3.0-only
//! The builder (feature `build`): link tables, class indices and whole
//! class sections, for the packer and the firmware build.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::classfile::{be16, derive, survey, utf8_at, walk_cp, CpView, Layout, Sink};
use crate::error::LinkError;
use crate::hash::{name_hash, string_hash};
use crate::layout::{Link, SUPER_IDX_WORD};
use crate::section::{
    ClassSection, IndexEntry, DIRECTORY_ENTRY_LEN, INDEX_ENTRY_LEN, LIT_UTF8, MAX_CLASSES,
    MAX_LITERALS, SECTION_HEADER_LEN,
};

/// The constant pool as walked from the class bytes.
struct VecCp {
    offsets: Vec<u16>,
    tags: Vec<u8>,
}

impl CpView for VecCp {
    fn cp_count(&self) -> usize {
        self.tags.len()
    }
    fn tag(&self, i: usize) -> Option<u8> {
        self.tags.get(i).copied()
    }
    fn offset(&self, i: usize) -> Option<usize> {
        self.offsets.get(i).map(|&o| o as usize)
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

/// Link one class: its table as words. Fails on anything the runtime
/// could not rely on (see [`LinkError`]).
pub fn link_class(class: &[u8]) -> Result<Vec<u16>, LinkError> {
    let mut cp = VecCp {
        offsets: Vec::new(),
        tags: Vec::new(),
    };
    let (cp_count, pos_after_cp) = walk_cp(class, |_, tag, off| {
        cp.tags.push(tag);
        cp.offsets.push(off as u16);
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
    debug_assert!(Link::new(&words).is_ok());
    Ok(words)
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
    const OBJECT: &[u8] = b"java/lang/Object";
    let mut out: Vec<u8> = Vec::with_capacity(48 + name.len());
    out.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34]);
    out.extend_from_slice(&5u16.to_be_bytes()); // cp_count: entries #1..#4
    out.extend_from_slice(&[0x07, 0x00, 0x02]); // #1 Class -> #2
    out.push(0x01); // #2 Utf8 name
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&[0x07, 0x00, 0x04]); // #3 Class -> #4
    out.push(0x01); // #4 Utf8 java/lang/Object
    out.extend_from_slice(&(OBJECT.len() as u16).to_be_bytes());
    out.extend_from_slice(OBJECT);
    out.extend_from_slice(&0x0021u16.to_be_bytes()); // ACC_PUBLIC | ACC_SUPER
    out.extend_from_slice(&1u16.to_be_bytes()); // this_class
    out.extend_from_slice(&3u16.to_be_bytes()); // super_class
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // no interfaces, fields, methods, attributes
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
    let mut links: Vec<Vec<u16>> = Vec::with_capacity(n);
    for class in classes {
        links.push(link_class(class)?);
    }
    let mut names: Vec<&[u8]> = Vec::with_capacity(n);
    for (i, (class, words)) in classes.iter().zip(&links).enumerate() {
        // Read through the class bytes, so the names borrow the classes
        // and not the tables, which are patched below.
        let name = utf8_at(class, Link::new(words)?.name_off()).ok_or(LinkError::Internal)?;
        if expected.is_some_and(|e| e.get(i).copied() != Some(name)) {
            return Err(LinkError::NameMismatch { idx: i as u16 });
        }
        names.push(name);
    }
    let index = class_index(&names)?;

    // Where the section holds each class's superclass, if it does.
    for i in 0..n {
        let super_off = Link::new(&links[i])?.super_off();
        let sup = (super_off != 0)
            .then(|| utf8_at(classes[i], super_off))
            .flatten()
            .and_then(|s| names.iter().position(|n| *n == s));
        if let Some(j) = sup {
            links[i][SUPER_IDX_WORD] = j as u16;
        }
    }

    // The literal pool: every String constant of the set, each distinct
    // byte string once, in order of first appearance. A row remembers the
    // class that first spelled it and where, so its bytes stay where they
    // are; each class's String descriptors get their row's number.
    struct Row<'c> {
        bytes: &'c [u8],
        class: usize,
        /// Offset of the bytes in that class.
        at: usize,
    }
    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut seen: BTreeMap<&[u8], u16> = BTreeMap::new();
    for (i, class) in classes.iter().enumerate() {
        let class: &[u8] = class;
        let link = Link::new(&links[i])?;
        let strs_off = link.strs_off();
        let mut ids: Vec<u16> = Vec::with_capacity(link.strs_len());
        let descs: Vec<_> = link.strings().collect();
        for d in descs {
            // Through the class bytes alone, so the row borrows the class
            // and not the table that is patched below.
            let ui = be16(class, d.cp_off as usize).ok_or(LinkError::Internal)? as usize;
            let bytes = link
                .cp_offset(ui)
                .and_then(|off| utf8_at(class, off))
                .ok_or(LinkError::Internal)?;
            let id = match seen.get(bytes) {
                Some(&id) => id,
                None => {
                    if rows.len() >= MAX_LITERALS {
                        return Err(LinkError::TooManyLiterals);
                    }
                    let id = rows.len() as u16;
                    rows.push(Row {
                        bytes,
                        class: i,
                        at: bytes.as_ptr() as usize - class.as_ptr() as usize,
                    });
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
    let mut class_offs: Vec<usize> = Vec::with_capacity(n);

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // index_off, patched below
    let dir_at = out.len();
    out.resize(dir_at + DIRECTORY_ENTRY_LEN * n, 0);
    for (i, (class, words)) in classes.iter().zip(&links).enumerate() {
        pad_to(&mut out, 4);
        let class_off = out.len();
        class_offs.push(class_off);
        out.extend_from_slice(class);
        pad_to(&mut out, 4);
        let link_off = out.len();
        out.extend_from_slice(&link_bytes(words));
        pad_to(&mut out, 4);
        let d = dir_at + DIRECTORY_ENTRY_LEN * i;
        out[d..d + 4].copy_from_slice(
            &u32::try_from(class_off)
                .map_err(|_| LinkError::BadSection)?
                .to_le_bytes(),
        );
        out[d + 4..d + 8].copy_from_slice(
            &u32::try_from(link_off)
                .map_err(|_| LinkError::BadSection)?
                .to_le_bytes(),
        );
    }
    pad_to(&mut out, 8);
    let index_off = out.len();
    for e in &index {
        out.extend_from_slice(&e.hash.to_le_bytes());
        out.extend_from_slice(&e.idx.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    debug_assert_eq!(out.len(), index_off + INDEX_ENTRY_LEN * n);
    debug_assert!(out.len() >= SECTION_HEADER_LEN);
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for r in &rows {
        let off = u32::try_from(class_offs[r.class] + r.at).map_err(|_| LinkError::BadSection)?;
        let flags = if core::str::from_utf8(r.bytes).is_ok() {
            LIT_UTF8
        } else {
            0
        };
        out.extend_from_slice(&string_hash(r.bytes).to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&(r.bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
    }
    out[4..8].copy_from_slice(
        &u32::try_from(index_off)
            .map_err(|_| LinkError::BadSection)?
            .to_le_bytes(),
    );
    debug_assert!(ClassSection::parse(&out).is_ok());
    Ok(out)
}
