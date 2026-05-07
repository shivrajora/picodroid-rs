// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;

use super::{
    BootstrapMethod, ClassFile, Cursor, ExceptionEntry, FieldInfo, MethodInfo, Parsed, TAG_CLASS,
    TAG_UTF8,
};

/// Walks the constant pool, producing (cp_offsets, cp_tags, position after CP).
/// Shared between `scan_name` (lazy registration) and `Parsed::parse` (full parse).
fn parse_cp(data: &[u8]) -> Result<(Vec<usize>, Vec<u8>, usize), &'static str> {
    let mut c = Cursor::new(data);

    let magic = c.u32().ok_or("truncated")?;
    if magic != 0xCAFEBABE {
        return Err("bad magic");
    }
    let _minor = c.u16().ok_or("truncated")?;
    let _major = c.u16().ok_or("truncated")?;

    let cp_count = c.u16().ok_or("truncated")? as usize;
    let mut cp_offsets: Vec<usize> = Vec::new();
    let mut cp_tags: Vec<u8> = Vec::new();
    cp_offsets.push(0);
    cp_tags.push(0);

    let mut idx = 1;
    while idx < cp_count {
        let tag = c.u8().ok_or("truncated")?;
        let data_offset = c.pos();
        cp_tags.push(tag);
        cp_offsets.push(data_offset);

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
                cp_tags.push(0);
                cp_offsets.push(0);
                idx += 1;
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
            _ => return Err("unknown CP tag"),
        }
        idx += 1;
    }

    Ok((cp_offsets, cp_tags, c.pos()))
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
            if parsed.cp_tags.get(i) != Some(&TAG_UTF8) {
                return Err("bad class name");
            }
            let off = parsed.cp_offsets[i];
            let len = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
            data.get(off + 2..off + 2 + len).ok_or("truncated name")?
        };
        Ok(ClassFile::new_eager(data, name, parsed))
    }
}

impl Parsed {
    pub(crate) fn parse(data: &'static [u8]) -> Result<Self, &'static str> {
        let (cp_offsets, cp_tags, pos_after_cp) = parse_cp(data)?;
        let mut c = Cursor::new(data);
        c.pos = pos_after_cp;

        // Access flags, this_class, super_class
        let access_flags = c.u16().ok_or("truncated")?;
        let this_class_idx = c.u16().ok_or("truncated")?;
        let super_class_cp = c.u16().ok_or("truncated")?;

        // Resolve class name: this_class_idx → Class CP entry → Utf8 index
        let class_name_utf8_idx = {
            let ci = this_class_idx as usize;
            if cp_tags.get(ci) != Some(&TAG_CLASS) {
                return Err("bad this_class");
            }
            let off = cp_offsets[ci];
            u16::from_be_bytes([data[off], data[off + 1]])
        };

        // Resolve super class name Utf8 index; 0 means java/lang/Object (not tracked)
        let super_class_name_index: u16 = if super_class_cp == 0 {
            0
        } else {
            let ci = super_class_cp as usize;
            if cp_tags.get(ci) != Some(&TAG_CLASS) {
                return Err("bad super_class");
            }
            let off = cp_offsets[ci];
            let utf8_idx = u16::from_be_bytes([data[off], data[off + 1]]);
            // Check if it's java/lang/Object — if so, treat as no superclass
            let ui = utf8_idx as usize;
            if cp_tags.get(ui) == Some(&TAG_UTF8) {
                let uoff = cp_offsets[ui];
                let ulen = u16::from_be_bytes([data[uoff], data[uoff + 1]]) as usize;
                if data.get(uoff + 2..uoff + 2 + ulen) == Some(b"java/lang/Object") {
                    0
                } else {
                    utf8_idx
                }
            } else {
                0
            }
        };

        // Parse interface list
        let iface_count = c.u16().ok_or("truncated")? as usize;
        let mut interfaces: Vec<u16> = Vec::new();
        for _ in 0..iface_count {
            let iface_cp_idx = c.u16().ok_or("truncated")?;
            let ci = iface_cp_idx as usize;
            if cp_tags.get(ci) == Some(&TAG_CLASS) {
                let off = cp_offsets[ci];
                let utf8_idx = u16::from_be_bytes([data[off], data[off + 1]]);
                interfaces.push(utf8_idx);
            }
        }

        // Parse fields, splitting into instance and static lists.
        let field_count = c.u16().ok_or("truncated")? as usize;
        let mut fields: Vec<FieldInfo> = Vec::new();
        let mut static_fields: Vec<FieldInfo> = Vec::new();
        for _ in 0..field_count {
            let access_flags = c.u16().ok_or("truncated")?;
            let name_idx = c.u16().ok_or("truncated")?;
            let descriptor_idx = c.u16().ok_or("truncated")?;
            let attr_count = c.u16().ok_or("truncated")? as usize;
            for _ in 0..attr_count {
                c.skip(2).ok_or("truncated")?; // attr name index
                let len = c.u32().ok_or("truncated")? as usize;
                c.skip(len).ok_or("truncated")?;
            }
            let info = FieldInfo {
                name_index: name_idx,
                descriptor_index: descriptor_idx,
            };
            // ACC_STATIC = 0x0008
            if access_flags & 0x0008 == 0 {
                fields.push(info);
            } else {
                static_fields.push(info);
            }
        }

        // Parse methods
        let method_count = c.u16().ok_or("truncated")? as usize;
        let mut methods: Vec<MethodInfo> = Vec::new();

        for _ in 0..method_count {
            let access_flags = c.u16().ok_or("truncated")?;
            let name_index = c.u16().ok_or("truncated")?;
            let descriptor_index = c.u16().ok_or("truncated")?;
            let attr_count = c.u16().ok_or("truncated")? as usize;

            let mut code_offset = 0usize;
            let mut code_len = 0usize;
            let mut max_stack = 0u16;
            let mut max_locals = 0u16;
            let mut exception_table: Vec<ExceptionEntry> = Vec::new();
            #[cfg(debug_assertions)]
            let mut lnt_offset = 0usize;
            #[cfg(debug_assertions)]
            let mut lnt_len = 0usize;

            for _ in 0..attr_count {
                let attr_name_idx = c.u16().ok_or("truncated")?;
                let attr_len = c.u32().ok_or("truncated")? as usize;
                let attr_start = c.pos();

                // Check if this is the "Code" attribute
                let is_code = {
                    let ni = attr_name_idx as usize;
                    cp_tags.get(ni) == Some(&TAG_UTF8) && {
                        let off = cp_offsets[ni];
                        let slen = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
                        data.get(off + 2..off + 2 + slen) == Some(b"Code")
                    }
                };

                if is_code {
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
                    // Skip over bytecode to reach the exception table
                    c.skip(cl).ok_or("truncated")?;
                    // Parse exception table
                    let exc_count = c.u16().ok_or("truncated")? as usize;
                    for _ in 0..exc_count {
                        let start_pc = c.u16().ok_or("truncated")?;
                        let end_pc = c.u16().ok_or("truncated")?;
                        let handler_pc = c.u16().ok_or("truncated")?;
                        let catch_type_index = c.u16().ok_or("truncated")?;
                        exception_table.push(ExceptionEntry {
                            start_pc,
                            end_pc,
                            handler_pc,
                            catch_type_index,
                        });
                    }
                    // Debug: scan Code sub-attributes for LineNumberTable.
                    #[cfg(debug_assertions)]
                    {
                        let sub_count = c.u16().ok_or("truncated")? as usize;
                        for _ in 0..sub_count {
                            let sub_name_idx = c.u16().ok_or("truncated")?;
                            let sub_len = c.u32().ok_or("truncated")? as usize;
                            let sub_start = c.pos();
                            let is_lnt = {
                                let ni = sub_name_idx as usize;
                                cp_tags.get(ni) == Some(&TAG_UTF8) && {
                                    let off = cp_offsets[ni];
                                    let slen =
                                        u16::from_be_bytes([data[off], data[off + 1]]) as usize;
                                    data.get(off + 2..off + 2 + slen) == Some(b"LineNumberTable")
                                }
                            };
                            if is_lnt && lnt_offset == 0 {
                                lnt_offset = sub_start;
                                lnt_len = sub_len;
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

            methods.push(MethodInfo {
                name_index,
                descriptor_index,
                code_offset,
                code_len,
                max_stack,
                max_locals,
                access_flags,
                exception_table,
                #[cfg(debug_assertions)]
                lnt_offset,
                #[cfg(debug_assertions)]
                lnt_len,
            });
        }

        // Parse class-level attributes (looking for BootstrapMethods)
        let mut bootstrap_methods: Vec<BootstrapMethod> = Vec::new();
        let class_attr_count = c.u16().ok_or("truncated")? as usize;
        for _ in 0..class_attr_count {
            let attr_name_idx = c.u16().ok_or("truncated")?;
            let attr_len = c.u32().ok_or("truncated")? as usize;
            let attr_start = c.pos();

            let is_bootstrap = {
                let ni = attr_name_idx as usize;
                cp_tags.get(ni) == Some(&TAG_UTF8) && {
                    let off = cp_offsets[ni];
                    let slen = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
                    data.get(off + 2..off + 2 + slen) == Some(b"BootstrapMethods")
                }
            };

            if is_bootstrap {
                let num_methods = c.u16().ok_or("truncated")? as usize;
                for _ in 0..num_methods {
                    let method_ref = c.u16().ok_or("truncated")?;
                    let num_args = c.u16().ok_or("truncated")? as usize;
                    let mut arguments = Vec::with_capacity(num_args);
                    for _ in 0..num_args {
                        arguments.push(c.u16().ok_or("truncated")?);
                    }
                    bootstrap_methods.push(BootstrapMethod {
                        method_ref,
                        arguments,
                    });
                }
            }
            // Skip to end of attribute (handles both BootstrapMethods and unknown attrs)
            c.pos = attr_start + attr_len;
        }

        Ok(Parsed {
            cp_offsets,
            cp_tags,
            methods,
            class_name_index: class_name_utf8_idx,
            super_class_name_index,
            fields,
            static_fields,
            access_flags,
            interfaces,
            bootstrap_methods,
        })
    }
}
