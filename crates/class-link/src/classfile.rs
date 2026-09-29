// SPDX-License-Identifier: GPL-3.0-only
//! Reading the class bytes: the constant-pool walk, the survey of what a
//! class declares, and [`derive`] — the derivation of a link table from the
//! class bytes, word by word. The builder runs it with a sink that writes
//! each word; [`crate::Link::validate`] runs it with a sink that compares
//! each word. Neither allocates.

use crate::descriptor::count_args;
use crate::error::LinkError;
use crate::hash::{name_hash, sig_hash};
use crate::layout::{HEADER_WORDS, LINK_MAGIC, MREF_INTERFACE};

/// Every offset a table stores is a `u16`; a class file past this is
/// refused at link time rather than truncated.
pub const MAX_CLASS_BYTES: usize = u16::MAX as usize;

pub const TAG_UTF8: u8 = 1;
pub const TAG_INTEGER: u8 = 3;
pub const TAG_FLOAT: u8 = 4;
pub const TAG_LONG: u8 = 5;
pub const TAG_DOUBLE: u8 = 6;
pub const TAG_CLASS: u8 = 7;
pub const TAG_STRING: u8 = 8;
pub const TAG_FIELDREF: u8 = 9;
pub const TAG_METHODREF: u8 = 10;
pub const TAG_INTERFACE_METHODREF: u8 = 11;
pub const TAG_NAME_AND_TYPE: u8 = 12;
pub const TAG_METHOD_HANDLE: u8 = 15;
pub const TAG_METHOD_TYPE: u8 = 16;
pub const TAG_DYNAMIC: u8 = 17;
pub const TAG_INVOKE_DYNAMIC: u8 = 18;
pub const TAG_MODULE: u8 = 19;
pub const TAG_PACKAGE: u8 = 20;

const ACC_STATIC: u16 = 0x0008;

/// Is `tag` a `Methodref` or `InterfaceMethodref` — the entries that get a
/// descriptor.
#[inline]
pub const fn is_methodref(tag: u8) -> bool {
    matches!(tag, TAG_METHODREF | TAG_INTERFACE_METHODREF)
}

/// Data size of a fixed-size constant-pool entry (after its tag byte).
/// `None` for `Utf8` (variable) and for tags JVMS §4.4 does not define.
pub const fn fixed_entry_len(tag: u8) -> Option<usize> {
    match tag {
        TAG_INTEGER | TAG_FLOAT => Some(4),
        TAG_LONG | TAG_DOUBLE => Some(8),
        TAG_CLASS | TAG_STRING => Some(2),
        TAG_FIELDREF | TAG_METHODREF | TAG_INTERFACE_METHODREF => Some(4),
        TAG_NAME_AND_TYPE => Some(4),
        TAG_METHOD_HANDLE => Some(3),
        TAG_METHOD_TYPE => Some(2),
        TAG_DYNAMIC | TAG_INVOKE_DYNAMIC => Some(4),
        TAG_MODULE | TAG_PACKAGE => Some(2),
        _ => None,
    }
}

/// Big-endian `u16` at `off` in the class bytes.
#[inline]
pub fn be16(d: &[u8], off: usize) -> Option<u16> {
    let b = d.get(off..off.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

/// Big-endian `u32` at `off` in the class bytes.
#[inline]
pub fn be32(d: &[u8], off: usize) -> Option<u32> {
    let b = d.get(off..off.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// A bounds-checked reader over the class bytes; every read past the end
/// is [`LinkError::Truncated`].
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn u8(&mut self) -> Result<u8, LinkError> {
        let v = *self.data.get(self.pos).ok_or(LinkError::Truncated)?;
        self.pos += 1;
        Ok(v)
    }

    pub fn u16(&mut self) -> Result<u16, LinkError> {
        let v = be16(self.data, self.pos).ok_or(LinkError::Truncated)?;
        self.pos += 2;
        Ok(v)
    }

    pub fn u32(&mut self) -> Result<u32, LinkError> {
        let v = be32(self.data, self.pos).ok_or(LinkError::Truncated)?;
        self.pos += 4;
        Ok(v)
    }

    pub fn skip(&mut self, n: usize) -> Result<(), LinkError> {
        self.seek(self.pos.checked_add(n).ok_or(LinkError::Truncated)?)
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), LinkError> {
        if pos > self.data.len() {
            return Err(LinkError::Truncated);
        }
        self.pos = pos;
        Ok(())
    }
}

/// Walk the constant pool without allocating, calling `visit(index, tag,
/// data_offset)` for every slot: index 0 and the pad slot after a
/// `Long`/`Double` are visited as `(index, 0, 0)`. Returns `(cp_count,
/// position after the pool)`. Refuses a file over [`MAX_CLASS_BYTES`], a
/// bad magic, an unknown tag or a truncated entry.
pub fn walk_cp(
    class: &[u8],
    mut visit: impl FnMut(usize, u8, usize) -> Result<(), LinkError>,
) -> Result<(usize, usize), LinkError> {
    if class.len() > MAX_CLASS_BYTES {
        return Err(LinkError::ClassTooLarge);
    }
    let mut c = Cursor::at(class, 0);
    if c.u32()? != 0xCAFE_BABE {
        return Err(LinkError::BadMagic);
    }
    c.skip(4)?; // minor_version, major_version
    let cp_count = c.u16()? as usize;
    visit(0, 0, 0)?;
    let mut idx = 1;
    while idx < cp_count {
        let tag = c.u8()?;
        let off = c.pos;
        visit(idx, tag, off)?;
        match tag {
            TAG_UTF8 => {
                let len = c.u16()? as usize;
                c.skip(len)?;
            }
            TAG_LONG | TAG_DOUBLE => {
                c.skip(8)?;
                idx += 1;
                if idx < cp_count {
                    visit(idx, 0, 0)?;
                }
            }
            t => {
                let n = fixed_entry_len(t).ok_or(LinkError::UnknownTag {
                    cp: idx as u16,
                    tag: t,
                })?;
                c.skip(n)?;
            }
        }
        idx += 1;
    }
    Ok((cp_count, c.pos))
}

/// A view of the constant pool's tags and data offsets: the builder's two
/// `Vec`s from [`walk_cp`], or a [`crate::Link`] being validated.
pub trait CpView {
    fn cp_count(&self) -> usize;
    /// Tag of entry `i`; 0 for index 0 and the pad slots; `None` past the pool.
    fn tag(&self, i: usize) -> Option<u8>;
    /// Byte offset of entry `i`'s data (after its tag); 0 where the tag is 0.
    fn offset(&self, i: usize) -> Option<usize>;
}

/// The bytes of `Utf8` entry `i`.
#[inline]
pub fn utf8<'c>(class: &'c [u8], cp: &impl CpView, i: usize) -> Option<&'c [u8]> {
    if cp.tag(i)? != TAG_UTF8 {
        return None;
    }
    utf8_at(class, cp.offset(i)?)
}

/// The bytes of a `Utf8` entry whose data (`[u16 len][bytes]`) starts at
/// `off`.
#[inline]
pub fn utf8_at(class: &[u8], off: usize) -> Option<&[u8]> {
    let len = be16(class, off)? as usize;
    class.get(off + 2..off + 2 + len)
}

/// The `Utf8` index a `Class` entry `i` names.
#[inline]
pub fn class_utf8_index(class: &[u8], cp: &impl CpView, i: usize) -> Option<usize> {
    if cp.tag(i)? != TAG_CLASS {
        return None;
    }
    Some(be16(class, cp.offset(i)?)? as usize)
}

/// For a `Class` entry: the byte offset of its name's `Utf8` data and the
/// name's hash. The name must be valid UTF-8 (the runtime reads it
/// unchecked).
fn class_name_ref(class: &[u8], cp: &impl CpView, i: usize) -> Result<(usize, u32), LinkError> {
    let ui = class_utf8_index(class, cp, i).ok_or(LinkError::BadThisClass)?;
    let off = cp.offset(ui).ok_or(LinkError::BadThisClass)?;
    let bytes = utf8(class, cp, ui).ok_or(LinkError::BadThisClass)?;
    check_utf8(bytes, ui)?;
    Ok((off, name_hash(bytes)))
}

fn check_utf8(bytes: &[u8], cp_index: usize) -> Result<(), LinkError> {
    core::str::from_utf8(bytes)
        .map(|_| ())
        .map_err(|_| LinkError::BadUtf8 {
            cp: cp_index as u16,
        })
}

fn skip_attributes(c: &mut Cursor<'_>) -> Result<(), LinkError> {
    let n = c.u16()? as usize;
    for _ in 0..n {
        c.skip(2)?;
        let len = c.u32()? as usize;
        c.skip(len)?;
    }
    Ok(())
}

/// What a class declares — everything the table's size depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub cp_count: usize,
    pub pos_after_cp: usize,
    pub ifaces: usize,
    pub fields: usize,
    pub statics: usize,
    pub methods: usize,
    pub mrefs: usize,
}

/// First pass: sizes only. `cp_count` and `pos_after_cp` come from
/// [`walk_cp`]; `cp` supplies the tags (to count the `Methodref`s).
pub fn survey(
    class: &[u8],
    cp: &impl CpView,
    cp_count: usize,
    pos_after_cp: usize,
) -> Result<Counts, LinkError> {
    let mrefs = (0..cp_count)
        .filter(|&i| cp.tag(i).is_some_and(is_methodref))
        .count();
    let mut c = Cursor::at(class, pos_after_cp);
    c.skip(6)?; // access_flags, this_class, super_class
    let ifaces = c.u16()? as usize;
    c.skip(2 * ifaces)?;
    let field_count = c.u16()? as usize;
    let (mut fields, mut statics) = (0, 0);
    for _ in 0..field_count {
        let access = c.u16()?;
        c.skip(4)?;
        skip_attributes(&mut c)?;
        if access & ACC_STATIC == 0 {
            fields += 1;
        } else {
            statics += 1;
        }
    }
    let methods = c.u16()? as usize;
    for _ in 0..methods {
        c.skip(6)?;
        skip_attributes(&mut c)?;
    }
    Ok(Counts {
        cp_count,
        pos_after_cp,
        ifaces,
        fields,
        statics,
        methods,
        mrefs,
    })
}

/// Words the packed tag bytes take.
#[inline]
pub const fn tag_words(cp_count: usize) -> usize {
    cp_count.div_ceil(2)
}

/// Where each region of the table starts, in words, and the table's total
/// length (padded to an even word count, so the table is a whole number
/// of 4-byte units).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub fields_off: usize,
    pub statics_off: usize,
    pub ifaces_off: usize,
    pub methods_off: usize,
    pub mrefs_off: usize,
    pub total_words: usize,
}

impl Layout {
    pub fn new(n: &Counts) -> Result<Layout, LinkError> {
        let fields_off = HEADER_WORDS + n.cp_count + tag_words(n.cp_count);
        let statics_off = fields_off + 2 * n.fields;
        let ifaces_off = statics_off + 2 * n.statics;
        let methods_off = ifaces_off + 3 * n.ifaces;
        let mrefs_off = methods_off + 6 * n.methods;
        let end = mrefs_off + 2 * n.mrefs;
        let total_words = end + (end & 1);
        let fits = |v: usize| v <= u16::MAX as usize;
        if !(fits(total_words)
            && fits(n.fields)
            && fits(n.statics)
            && fits(n.ifaces)
            && fits(n.methods)
            && fits(n.mrefs))
        {
            return Err(LinkError::ClassTooLarge);
        }
        Ok(Layout {
            fields_off,
            statics_off,
            ifaces_off,
            methods_off,
            mrefs_off,
            total_words,
        })
    }
}

/// Where [`derive`] puts each word: the builder's table under construction,
/// or the validator's comparison against an existing table.
pub trait Sink {
    fn put(&mut self, word: usize, value: u16) -> Result<(), LinkError>;
}

/// Derive the link table of `class` word by word into `out`. Every word of
/// `layout.total_words` is put exactly once. `cp` must describe the class's
/// own constant pool (the builder's walk, or a table already cross-checked
/// against one). Fails on anything the runtime could not rely on: a
/// `Methodref` whose parts are the wrong kind, an unparseable descriptor, a
/// member whose name is not `Utf8`, a name that is not UTF-8.
pub fn derive(
    class: &[u8],
    cp: &impl CpView,
    n: &Counts,
    l: &Layout,
    out: &mut impl Sink,
) -> Result<(), LinkError> {
    let cp_count = n.cp_count;
    let put32 = |out: &mut dyn Sink, word: usize, v: u32| -> Result<(), LinkError> {
        out.put(word, v as u16)?;
        out.put(word + 1, (v >> 16) as u16)
    };

    // Constant pool: each entry's data offset — for a Methodref, the word
    // offset of its descriptor instead — and the tags, packed two a word.
    let mut k = 0usize;
    let mut lo_tag = 0u8;
    for i in 0..cp_count {
        let tag = cp.tag(i).ok_or(LinkError::Internal)?;
        let word = if is_methodref(tag) {
            let w = l.mrefs_off + 2 * k;
            k += 1;
            w
        } else {
            cp.offset(i).ok_or(LinkError::Internal)?
        };
        out.put(HEADER_WORDS + i, word as u16)?;
        if i.is_multiple_of(2) {
            lo_tag = tag;
        } else {
            out.put(
                HEADER_WORDS + cp_count + i / 2,
                lo_tag as u16 | (tag as u16) << 8,
            )?;
        }
    }
    if !cp_count.is_multiple_of(2) {
        out.put(HEADER_WORDS + cp_count + cp_count / 2, lo_tag as u16)?;
    }

    // Methodref descriptors, in constant-pool order.
    k = 0;
    for i in 0..cp_count {
        let tag = cp.tag(i).ok_or(LinkError::Internal)?;
        if !is_methodref(tag) {
            continue;
        }
        let bad = LinkError::BadMethodref { cp: i as u16 };
        let off = cp.offset(i).ok_or(LinkError::Internal)?;
        let class_idx = be16(class, off).ok_or(LinkError::Truncated)? as usize;
        if cp.tag(class_idx) != Some(TAG_CLASS) {
            return Err(bad);
        }
        let nat = be16(class, off + 2).ok_or(LinkError::Truncated)? as usize;
        if cp.tag(nat) != Some(TAG_NAME_AND_TYPE) {
            return Err(bad);
        }
        let nat_off = cp.offset(nat).ok_or(LinkError::Internal)?;
        let desc_idx = be16(class, nat_off + 2).ok_or(LinkError::Truncated)? as usize;
        let desc = utf8(class, cp, desc_idx).ok_or(bad)?;
        let argc = count_args(desc).ok_or(LinkError::BadDescriptor { cp: i as u16 })?;
        let flags = if tag == TAG_INTERFACE_METHODREF {
            MREF_INTERFACE
        } else {
            0
        };
        out.put(l.mrefs_off + 2 * k, off as u16)?;
        out.put(l.mrefs_off + 2 * k + 1, argc as u16 | (flags as u16) << 8)?;
        k += 1;
    }
    if k != n.mrefs {
        return Err(LinkError::Internal);
    }

    // Every name the runtime decodes without re-checking must be UTF-8: the
    // targets of Class entries and both halves of every NameAndType.
    for i in 0..cp_count {
        match cp.tag(i) {
            Some(TAG_CLASS) => {
                let ui = class_utf8_index(class, cp, i).ok_or(LinkError::Truncated)?;
                let bytes = utf8(class, cp, ui).ok_or(LinkError::BadUtf8 { cp: i as u16 })?;
                check_utf8(bytes, ui)?;
            }
            Some(TAG_NAME_AND_TYPE) => {
                let off = cp.offset(i).ok_or(LinkError::Internal)?;
                for j in 0..2 {
                    let ui = be16(class, off + 2 * j).ok_or(LinkError::Truncated)? as usize;
                    let bytes = utf8(class, cp, ui).ok_or(LinkError::BadUtf8 { cp: i as u16 })?;
                    check_utf8(bytes, ui)?;
                }
            }
            _ => {}
        }
    }

    // The class itself: access flags, its name, its superclass.
    let mut c = Cursor::at(class, n.pos_after_cp);
    let access_flags = c.u16()?;
    let this_idx = c.u16()? as usize;
    let super_idx = c.u16()? as usize;
    let (name_off, name_h) = class_name_ref(class, cp, this_idx)?;
    let (super_off, super_h) = if super_idx == 0 {
        (0, 0)
    } else {
        class_name_ref(class, cp, super_idx).map_err(|_| LinkError::BadSuperClass)?
    };

    // Interfaces: the name's Utf8 offset and hash.
    let ifaces = c.u16()? as usize;
    if ifaces != n.ifaces {
        return Err(LinkError::Internal);
    }
    for j in 0..ifaces {
        let idx = c.u16()? as usize;
        let (off, h) = class_name_ref(class, cp, idx)
            .map_err(|_| LinkError::BadInterface { index: j as u16 })?;
        let base = l.ifaces_off + 3 * j;
        out.put(base, off as u16)?;
        put32(out, base + 1, h)?;
    }

    // Fields, split into the instance and static regions.
    let field_count = c.u16()? as usize;
    let (mut fi, mut si) = (0usize, 0usize);
    for j in 0..field_count {
        let access = c.u16()?;
        let name = c.u16()?;
        let desc = c.u16()?;
        skip_attributes(&mut c)?;
        let bad = LinkError::BadMember { index: j as u16 };
        check_utf8(utf8(class, cp, name as usize).ok_or(bad)?, name as usize)?;
        check_utf8(utf8(class, cp, desc as usize).ok_or(bad)?, desc as usize)?;
        let base = if access & ACC_STATIC == 0 {
            fi += 1;
            l.fields_off + 2 * (fi - 1)
        } else {
            si += 1;
            l.statics_off + 2 * (si - 1)
        };
        out.put(base, name)?;
        out.put(base + 1, desc)?;
    }
    if fi != n.fields || si != n.statics {
        return Err(LinkError::Internal);
    }

    // Methods: where the method_info lies, where its bytecode starts, its
    // flags, its LineNumberTable, its signature hash.
    let method_count = c.u16()? as usize;
    if method_count != n.methods {
        return Err(LinkError::Internal);
    }
    for mi in 0..method_count {
        let info_offset = c.pos;
        let access = c.u16()?;
        let name = c.u16()? as usize;
        let desc = c.u16()? as usize;
        let attr_count = c.u16()? as usize;
        let bad = LinkError::BadMember { index: mi as u16 };
        let name_b = utf8(class, cp, name).ok_or(bad)?;
        let desc_b = utf8(class, cp, desc).ok_or(bad)?;
        check_utf8(name_b, name)?;
        check_utf8(desc_b, desc)?;
        let mut code_offset = 0usize;
        let mut lnt_offset = 0usize;
        for _ in 0..attr_count {
            let attr_name = c.u16()? as usize;
            let attr_len = c.u32()? as usize;
            let attr_start = c.pos;
            if utf8(class, cp, attr_name) == Some(b"Code") {
                // u16 max_stack, u16 max_locals, u32 code_length, code,
                // exception table, sub-attributes. The bytecode offset is
                // all the table keeps; the three header words are read
                // from the class bytes just before it.
                c.skip(4)?;
                let code_len = c.u32()? as usize;
                code_offset = c.pos;
                c.skip(code_len)?;
                let exc = c.u16()? as usize;
                c.skip(8 * exc)?;
                let subs = c.u16()? as usize;
                for _ in 0..subs {
                    let sub_name = c.u16()? as usize;
                    let sub_len = c.u32()? as usize;
                    let sub_start = c.pos;
                    if lnt_offset == 0 && utf8(class, cp, sub_name) == Some(b"LineNumberTable") {
                        lnt_offset = sub_start;
                    }
                    c.seek(sub_start + sub_len)?;
                }
                c.seek(attr_start + attr_len)?;
            } else {
                c.skip(attr_len)?;
            }
        }
        let base = l.methods_off + 6 * mi;
        out.put(base, info_offset as u16)?;
        out.put(base + 1, code_offset as u16)?;
        out.put(base + 2, access)?;
        out.put(base + 3, lnt_offset as u16)?;
        put32(out, base + 4, sig_hash(name_b, desc_b))?;
    }

    // Class attributes: BootstrapMethods stays in the class bytes (its
    // body offset is kept, its entries checked once here), SourceFile is a
    // Utf8 index.
    let attr_count = c.u16()? as usize;
    let mut bsm_off = 0usize;
    let mut source_file_idx = 0u16;
    for _ in 0..attr_count {
        let attr_name = c.u16()? as usize;
        let attr_len = c.u32()? as usize;
        let attr_start = c.pos;
        let name = utf8(class, cp, attr_name);
        if name == Some(b"BootstrapMethods") {
            let num_methods = c.u16()? as usize;
            for _ in 0..num_methods {
                c.skip(2)?;
                let num_args = c.u16()? as usize;
                c.skip(2 * num_args)?;
            }
            if c.pos > attr_start + attr_len {
                return Err(LinkError::BadBootstrapMethods);
            }
            bsm_off = attr_start;
        } else if attr_len == 2 && name == Some(b"SourceFile") {
            source_file_idx = c.u16()?;
        }
        c.seek(attr_start + attr_len)?;
    }

    // Header.
    out.put(0, LINK_MAGIC)?;
    out.put(1, l.total_words as u16)?;
    out.put(2, class.len() as u16)?;
    out.put(3, cp_count as u16)?;
    out.put(4, n.fields as u16)?;
    out.put(5, n.statics as u16)?;
    out.put(6, n.ifaces as u16)?;
    out.put(7, n.methods as u16)?;
    out.put(8, n.mrefs as u16)?;
    out.put(9, l.methods_off as u16)?;
    out.put(10, l.mrefs_off as u16)?;
    out.put(11, bsm_off as u16)?;
    out.put(12, access_flags)?;
    out.put(13, name_off as u16)?;
    out.put(14, super_off as u16)?;
    out.put(15, source_file_idx)?;
    put32(out, 16, name_h)?;
    put32(out, 18, super_h)?;

    // The pad word that makes the table a whole number of 4-byte units.
    let end = l.mrefs_off + 2 * n.mrefs;
    if end < l.total_words {
        out.put(end, 0)?;
    }
    Ok(())
}
