// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use super::{
    words_as_mut, ClassFile, Cursor, FieldInfo, MethodInfo, Parsed, WordRecord, TAG_CLASS, TAG_UTF8,
};

/// Every byte offset the parser records is a `u16`: `cp_offsets`,
/// `code_offset`, `lnt_offset`, `bsm_offset`. No class file that ships is
/// anywhere near this (the largest example class is 51 KB); one that is
/// gets refused at registration rather than truncated.
const MAX_CLASS_BYTES: usize = u16::MAX as usize;

/// Walks the constant pool without allocating, calling `visit(index, tag,
/// data_offset)` for every slot (the pad slot after a Long/Double is
/// visited as `(index, 0, 0)`, as is the unused index 0). Returns
/// `(cp_count, position after the pool)`.
fn walk_cp(
    data: &[u8],
    mut visit: impl FnMut(usize, u8, usize),
) -> Result<(usize, usize), &'static str> {
    if data.len() > MAX_CLASS_BYTES {
        return Err("class file too large");
    }
    let mut c = Cursor::new(data);

    let magic = c.u32().ok_or("truncated")?;
    if magic != 0xCAFEBABE {
        return Err("bad magic");
    }
    let _minor = c.u16().ok_or("truncated")?;
    let _major = c.u16().ok_or("truncated")?;

    let cp_count = c.u16().ok_or("truncated")? as usize;
    visit(0, 0, 0);

    let mut idx = 1;
    while idx < cp_count {
        let tag = c.u8().ok_or("truncated")?;
        let data_offset = c.pos();
        visit(idx, tag, data_offset);

        match tag {
            TAG_UTF8 => {
                let len = c.u16().ok_or("truncated")? as usize;
                c.skip(len).ok_or("truncated")?;
            }
            TAG_CLASS => {
                c.skip(2).ok_or("truncated")?;
            }
            8 => {
                c.skip(2).ok_or("truncated")?;
            }
            10 => {
                c.skip(4).ok_or("truncated")?;
            }
            12 => {
                c.skip(4).ok_or("truncated")?;
            }
            5 | 6 => {
                c.skip(8).ok_or("truncated")?;
                idx += 1;
                if idx < cp_count {
                    visit(idx, 0, 0);
                }
            }
            3 | 4 => {
                c.skip(4).ok_or("truncated")?;
            }
            11 => {
                c.skip(4).ok_or("truncated")?;
            }
            9 => {
                c.skip(4).ok_or("truncated")?;
            }
            18 => {
                c.skip(4).ok_or("truncated")?;
            }
            15 => {
                c.skip(3).ok_or("truncated")?;
            }
            16 => {
                c.skip(2).ok_or("truncated")?;
            }
            // CONSTANT_Dynamic (17, bootstrap_method_attr_index + name_and_type_index)
            // and CONSTANT_Module / CONSTANT_Package (19 / 20, one name index)
            // are skipped by size like every other entry nothing resolves —
            // a class that carries one is registered, and only an `ldc` of
            // the condy entry itself fails (JVMS §4.4.13, §4.4.11/12). Same
            // rows as tools/class-shrink/src/classfile.rs.
            17 => {
                c.skip(4).ok_or("truncated")?;
            }
            19 | 20 => {
                c.skip(2).ok_or("truncated")?;
            }
            _ => return Err("unknown CP tag"),
        }
        idx += 1;
    }

    Ok((cp_count, c.pos()))
}

/// The constant pool as two transient `Vec`s, for the registration scan
/// (`register`), which only needs the class name and frees them before it
/// returns. `Parsed::parse` writes the same walk straight into its record.
fn parse_cp(data: &[u8]) -> Result<(Vec<usize>, Vec<u8>, usize), &'static str> {
    let mut cp_offsets: Vec<usize> = Vec::new();
    let mut cp_tags: Vec<u8> = Vec::new();
    let (_, pos) = walk_cp(data, |_, tag, off| {
        cp_tags.push(tag);
        cp_offsets.push(off);
    })?;
    Ok((cp_offsets, cp_tags, pos))
}

/// Resolves a CP Class index to its UTF8 class-name bytes.
fn cp_class_utf8(
    data: &'static [u8],
    cp_offsets: &[usize],
    cp_tags: &[u8],
    class_idx: u16,
) -> Option<&'static [u8]> {
    let ci = class_idx as usize;
    if cp_tags.get(ci) != Some(&TAG_CLASS) {
        return None;
    }
    let off = cp_offsets[ci];
    let utf8_idx = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
    if cp_tags.get(utf8_idx) != Some(&TAG_UTF8) {
        return None;
    }
    let uoff = cp_offsets[utf8_idx];
    let ulen = u16::from_be_bytes([data[uoff], data[uoff + 1]]) as usize;
    data.get(uoff + 2..uoff + 2 + ulen)
}

impl ClassFile {
    /// Registers a class file without fully parsing it.
    ///
    /// Scans magic, the constant pool, and the `this_class` index to extract
    /// the class name (returned via [`ClassFile::scanned_name`]).  The full
    /// method/field/interface tables are parsed lazily on first access.
    ///
    /// This keeps startup RAM low: classes never referenced by the running app
    /// stay in "registered-only" state and never allocate parsed metadata.
    pub fn register(data: &'static [u8]) -> Result<Self, &'static str> {
        let (cp_offsets, cp_tags, pos_after_cp) = parse_cp(data)?;
        // After CP: access_flags (u16), this_class (u16).  Read this_class.
        let this_class_pos = pos_after_cp + 2;
        if data.len() < this_class_pos + 2 {
            return Err("truncated");
        }
        let this_class_idx = u16::from_be_bytes([data[this_class_pos], data[this_class_pos + 1]]);
        let name =
            cp_class_utf8(data, &cp_offsets, &cp_tags, this_class_idx).ok_or("bad this_class")?;
        Ok(ClassFile::new_lazy(data, name))
    }

    /// Fully parses a class file eagerly.  Kept for tests and callers that
    /// want to fail-fast on malformed bytecode at load time.
    pub fn parse(data: &'static [u8]) -> Result<Self, &'static str> {
        let parsed = Parsed::parse(data)?;
        let name = {
            let i = parsed.class_name_index as usize;
            if parsed.cp_tag(i) != Some(TAG_UTF8) {
                return Err("bad class name");
            }
            let off = parsed.cp_offset(i);
            let len = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
            data.get(off + 2..off + 2 + len).ok_or("truncated name")?
        };
        Ok(ClassFile::new_eager(data, name, parsed))
    }
}

/// What the first pass counts: everything the record's size depends on.
struct Counts {
    cp_count: usize,
    pos_after_cp: usize,
    iface_count: usize,
    fields: usize,
    statics: usize,
    methods: usize,
}

/// Skips `attributes_count` attributes by their declared lengths.
fn skip_attributes(c: &mut Cursor<'_>) -> Result<(), &'static str> {
    let attr_count = c.u16().ok_or("truncated")? as usize;
    for _ in 0..attr_count {
        c.skip(2).ok_or("truncated")?; // attribute name index
        let len = c.u32().ok_or("truncated")? as usize;
        c.skip(len).ok_or("truncated")?;
    }
    Ok(())
}

/// First pass: sizes only, no allocation and no constant-pool lookups.
fn survey(data: &[u8]) -> Result<Counts, &'static str> {
    let (cp_count, pos_after_cp) = walk_cp(data, |_, _, _| {})?;
    let mut c = Cursor::new(data);
    c.pos = pos_after_cp;
    c.skip(6).ok_or("truncated")?; // access_flags, this_class, super_class
    let iface_count = c.u16().ok_or("truncated")? as usize;
    c.skip(2 * iface_count).ok_or("truncated")?;

    let field_count = c.u16().ok_or("truncated")? as usize;
    let mut fields = 0;
    let mut statics = 0;
    for _ in 0..field_count {
        let access_flags = c.u16().ok_or("truncated")?;
        c.skip(4).ok_or("truncated")?; // name, descriptor
        skip_attributes(&mut c)?;
        // ACC_STATIC = 0x0008
        if access_flags & 0x0008 == 0 {
            fields += 1;
        } else {
            statics += 1;
        }
    }

    let methods = c.u16().ok_or("truncated")? as usize;
    for _ in 0..methods {
        c.skip(6).ok_or("truncated")?; // access, name, descriptor
        skip_attributes(&mut c)?;
    }
    Ok(Counts {
        cp_count,
        pos_after_cp,
        iface_count,
        fields,
        statics,
        methods,
    })
}

/// Is CP entry `ni` the Utf8 string `s`? Reads the pool straight from the
/// half-built record.
fn cp_utf8_is(blob: &[u16], cp_count: usize, data: &[u8], ni: usize, s: &[u8]) -> bool {
    if ni >= cp_count || Parsed::tag_bytes(blob, cp_count)[ni] != TAG_UTF8 {
        return false;
    }
    let off = blob[ni] as usize;
    let slen = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
    data.get(off + 2..off + 2 + slen) == Some(s)
}

impl Parsed {
    /// Two passes over the class bytes: the first counts, the second fills
    /// the one exact allocation the counts size. Nothing here grows.
    pub(crate) fn parse(data: &'static [u8]) -> Result<Self, &'static str> {
        let n = survey(data)?;
        let cp_count = n.cp_count;
        let tag_words = Parsed::tag_words(cp_count);
        let fields_off = cp_count + tag_words;
        let statics_off = fields_off + 2 * n.fields;
        let ifaces_off = statics_off + 2 * n.statics;
        let methods_off = ifaces_off + n.iface_count;
        let words = methods_off + n.methods * MethodInfo::WORDS;
        if words > u16::MAX as usize
            || n.fields > u16::MAX as usize
            || n.statics > u16::MAX as usize
            || n.methods > u16::MAX as usize
        {
            return Err("class file too large");
        }

        let mut v: Vec<u16> = Vec::new();
        v.try_reserve_exact(words).map_err(|_| "out of memory")?;
        v.resize(words, 0);
        let blob = v.as_mut_slice();

        // Second pass over the pool: offsets into the first region, tags
        // packed into the second.
        walk_cp(data, |idx, tag, off| {
            blob[idx] = off as u16;
            Parsed::tag_bytes_mut(blob, cp_count)[idx] = tag;
        })?;

        let mut c = Cursor::new(data);
        c.pos = n.pos_after_cp;

        // Access flags, this_class, super_class
        let access_flags = c.u16().ok_or("truncated")?;
        let this_class_idx = c.u16().ok_or("truncated")?;
        let super_class_cp = c.u16().ok_or("truncated")?;

        let tag_at = |blob: &[u16], i: usize| -> Option<u8> {
            (i < cp_count).then(|| Parsed::tag_bytes(blob, cp_count)[i])
        };

        // Resolve class name: this_class_idx → Class CP entry → Utf8 index
        let class_name_utf8_idx = {
            let ci = this_class_idx as usize;
            if tag_at(blob, ci) != Some(TAG_CLASS) {
                return Err("bad this_class");
            }
            let off = blob[ci] as usize;
            u16::from_be_bytes([data[off], data[off + 1]])
        };

        // Resolve super class name Utf8 index; 0 means java/lang/Object (not tracked)
        let super_class_name_index: u16 = if super_class_cp == 0 {
            0
        } else {
            let ci = super_class_cp as usize;
            if tag_at(blob, ci) != Some(TAG_CLASS) {
                return Err("bad super_class");
            }
            let off = blob[ci] as usize;
            let utf8_idx = u16::from_be_bytes([data[off], data[off + 1]]);
            // Check if it's java/lang/Object — if so, treat as no superclass.
            if cp_utf8_is(
                blob,
                cp_count,
                data,
                utf8_idx as usize,
                crate::names::c::java_lang_Object.as_bytes(),
            ) {
                0
            } else if tag_at(blob, utf8_idx as usize) == Some(TAG_UTF8) {
                utf8_idx
            } else {
                0
            }
        };

        // Interface list: Class entries resolved to their Utf8 index; a
        // non-Class entry (malformed) is dropped and its reserved word stays
        // zero past `ifaces_len`.
        let iface_count = c.u16().ok_or("truncated")? as usize;
        let mut ifaces_len = 0usize;
        for _ in 0..iface_count {
            let iface_cp_idx = c.u16().ok_or("truncated")?;
            let ci = iface_cp_idx as usize;
            if tag_at(blob, ci) == Some(TAG_CLASS) {
                let off = blob[ci] as usize;
                blob[ifaces_off + ifaces_len] = u16::from_be_bytes([data[off], data[off + 1]]);
                ifaces_len += 1;
            }
        }

        // Fields, split into the instance and static regions.
        let field_count = c.u16().ok_or("truncated")? as usize;
        let mut fi = 0usize;
        let mut si = 0usize;
        for _ in 0..field_count {
            let access_flags = c.u16().ok_or("truncated")?;
            let name_idx = c.u16().ok_or("truncated")?;
            let descriptor_idx = c.u16().ok_or("truncated")?;
            skip_attributes(&mut c)?;
            let info = FieldInfo {
                name_index: name_idx,
                descriptor_index: descriptor_idx,
            };
            // ACC_STATIC = 0x0008
            if access_flags & 0x0008 == 0 {
                words_as_mut::<FieldInfo>(&mut blob[fields_off..statics_off])[fi] = info;
                fi += 1;
            } else {
                words_as_mut::<FieldInfo>(&mut blob[statics_off..ifaces_off])[si] = info;
                si += 1;
            }
        }

        // Methods
        let method_count = c.u16().ok_or("truncated")? as usize;
        for mi in 0..method_count {
            let access_flags = c.u16().ok_or("truncated")?;
            let name_index = c.u16().ok_or("truncated")?;
            let descriptor_index = c.u16().ok_or("truncated")?;
            let attr_count = c.u16().ok_or("truncated")? as usize;

            let mut code_offset = 0usize;
            let mut code_len = 0usize;
            let mut max_stack = 0u16;
            let mut max_locals = 0u16;
            #[cfg(feature = "line-numbers")]
            let mut lnt_offset = 0u16;

            for _ in 0..attr_count {
                let attr_name_idx = c.u16().ok_or("truncated")?;
                let attr_len = c.u32().ok_or("truncated")? as usize;
                let attr_start = c.pos();

                if cp_utf8_is(blob, cp_count, data, attr_name_idx as usize, b"Code") {
                    // Code attribute layout:
                    // u16 max_stack, u16 max_locals, u32 code_length, [u8; code_length],
                    // u16 exception_table_length, [exception_entry; N], ...
                    let ms = c.u16().ok_or("truncated")?;
                    let ml = c.u16().ok_or("truncated")?;
                    let cl = c.u32().ok_or("truncated")? as usize;
                    max_stack = ms;
                    max_locals = ml;
                    code_offset = c.pos();
                    code_len = cl;
                    // Skip over bytecode to reach the exception table, then
                    // over the table itself: it stays in flash, read by
                    // `ClassFile::exception_table` from `code_offset + code_len`.
                    c.skip(cl).ok_or("truncated")?;
                    let exc_count = c.u16().ok_or("truncated")? as usize;
                    c.skip(8 * exc_count).ok_or("truncated")?;
                    // line-numbers: scan Code sub-attributes for LineNumberTable.
                    #[cfg(feature = "line-numbers")]
                    {
                        let sub_count = c.u16().ok_or("truncated")? as usize;
                        for _ in 0..sub_count {
                            let sub_name_idx = c.u16().ok_or("truncated")?;
                            let sub_len = c.u32().ok_or("truncated")? as usize;
                            let sub_start = c.pos();
                            let is_lnt = cp_utf8_is(
                                blob,
                                cp_count,
                                data,
                                sub_name_idx as usize,
                                b"LineNumberTable",
                            );
                            if is_lnt && lnt_offset == 0 {
                                lnt_offset = u16::try_from(sub_start).unwrap_or(0);
                            }
                            c.pos = sub_start + sub_len;
                        }
                    }
                    // Always skip to end of Code attribute (corrects position in both profiles).
                    c.pos = attr_start + attr_len;
                } else {
                    c.skip(attr_len).ok_or("truncated")?;
                }
            }

            let methods = words_as_mut::<MethodInfo>(&mut blob[methods_off..words]);
            methods[mi] = MethodInfo {
                name_index,
                descriptor_index,
                // Both fit: the whole file is under 64 KB (`walk_cp`).
                code_offset: code_offset as u16,
                code_len: code_len as u16,
                max_stack,
                max_locals,
                access_flags,
                #[cfg(feature = "line-numbers")]
                lnt_offset,
            };
        }

        // Class-level attributes: BootstrapMethods stays in flash (its
        // body offset is all the record keeps), SourceFile when line
        // numbers are on.
        let mut bsm_offset = 0u16;
        #[cfg(feature = "line-numbers")]
        let mut source_file_index = 0u16;
        let class_attr_count = c.u16().ok_or("truncated")? as usize;
        for _ in 0..class_attr_count {
            let attr_name_idx = c.u16().ok_or("truncated")?;
            let attr_len = c.u32().ok_or("truncated")? as usize;
            let attr_start = c.pos();
            let ni = attr_name_idx as usize;

            if cp_utf8_is(blob, cp_count, data, ni, b"BootstrapMethods") {
                // Validate the body once so the reader can trust its
                // bounds: num_bootstrap_methods, then per entry a
                // method_ref, num_args and the args.
                let num_methods = c.u16().ok_or("truncated")? as usize;
                for _ in 0..num_methods {
                    c.skip(2).ok_or("truncated")?;
                    let num_args = c.u16().ok_or("truncated")? as usize;
                    c.skip(2 * num_args).ok_or("truncated")?;
                }
                if c.pos() > attr_start + attr_len {
                    return Err("bad BootstrapMethods");
                }
                bsm_offset = attr_start as u16;
            }
            #[cfg(feature = "line-numbers")]
            {
                if attr_len == 2 && cp_utf8_is(blob, cp_count, data, ni, b"SourceFile") {
                    source_file_index = c.u16().ok_or("truncated")?;
                }
            }
            // Skip to end of attribute (handles both BootstrapMethods and unknown attrs)
            c.pos = attr_start + attr_len;
        }

        Ok(Parsed {
            blob: v.into_boxed_slice(),
            cp_count: cp_count as u16,
            fields_off: fields_off as u16,
            fields_len: n.fields as u16,
            statics_len: n.statics as u16,
            ifaces_len: ifaces_len as u16,
            methods_off: methods_off as u16,
            methods_len: n.methods as u16,
            bsm_offset,
            class_name_index: class_name_utf8_idx,
            super_class_name_index,
            access_flags,
            #[cfg(feature = "line-numbers")]
            source_file_index,
        })
    }
}
