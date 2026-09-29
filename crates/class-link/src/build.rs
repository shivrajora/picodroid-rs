// SPDX-License-Identifier: GPL-3.0-only
//! The builder (feature `build`): link tables, class indices and whole
//! class sections, for the packer and the firmware build.

use alloc::vec::Vec;

use crate::classfile::{derive, survey, walk_cp, CpView, Layout, Sink};
use crate::error::LinkError;
use crate::hash::name_hash;
use crate::layout::{Link, Linked};
use crate::section::{
    ClassSection, IndexEntry, DIRECTORY_ENTRY_LEN, INDEX_ENTRY_LEN, MAX_CLASSES, SECTION_HEADER_LEN,
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
        let linked = Linked {
            class,
            link: Link::new(words)?,
        };
        let name = linked.name().ok_or(LinkError::Internal)?;
        if expected.is_some_and(|e| e.get(i).copied() != Some(name)) {
            return Err(LinkError::NameMismatch { idx: i as u16 });
        }
        names.push(name);
    }
    let index = class_index(&names)?;

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // index_off, patched below
    let dir_at = out.len();
    out.resize(dir_at + DIRECTORY_ENTRY_LEN * n, 0);
    for (i, (class, words)) in classes.iter().zip(&links).enumerate() {
        pad_to(&mut out, 4);
        let class_off = out.len();
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
    out[4..8].copy_from_slice(
        &u32::try_from(index_off)
            .map_err(|_| LinkError::BadSection)?
            .to_le_bytes(),
    );
    debug_assert!(ClassSection::parse(&out).is_ok());
    Ok(out)
}
