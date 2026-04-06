use alloc::vec::Vec;

use super::{
    BootstrapMethod, ClassFile, Cursor, ExceptionEntry, FieldInfo, MethodInfo, TAG_CLASS, TAG_UTF8,
};

impl ClassFile {
    pub fn parse(data: &'static [u8]) -> Result<Self, &'static str> {
        let mut c = Cursor::new(data);

        // Magic
        let magic = c.u32().ok_or("truncated")?;
        if magic != 0xCAFEBABE {
            return Err("bad magic");
        }

        // Version
        let _minor = c.u16().ok_or("truncated")?;
        let _major = c.u16().ok_or("truncated")?;

        // Constant pool
        let cp_count = c.u16().ok_or("truncated")? as usize;
        // cp_offsets[0] unused; entries 1..cp_count
        let mut cp_offsets: Vec<usize> = Vec::new();
        let mut cp_tags: Vec<u8> = Vec::new();
        cp_offsets.push(0); // index 0 placeholder
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
                    c.skip(2).ok_or("truncated")?; // name_index
                }
                8 => {
                    c.skip(2).ok_or("truncated")?; // string_index
                }
                10 => {
                    c.skip(4).ok_or("truncated")?; // class_index + name_and_type_index
                }
                12 => {
                    c.skip(4).ok_or("truncated")?; // name_index + descriptor_index
                }
                // Long/Double take two CP slots (rare in M1 code)
                5 | 6 => {
                    c.skip(8).ok_or("truncated")?;
                    cp_tags.push(0);
                    cp_offsets.push(0);
                    idx += 1;
                }
                // Integer, Float, InvokeDynamic, FieldRef, InterfaceMethodRef, etc.
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

        // Parse instance fields
        let field_count = c.u16().ok_or("truncated")? as usize;
        let mut fields: Vec<FieldInfo> = Vec::new();
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
            // Store non-static instance fields only (ACC_STATIC = 0x0008)
            if access_flags & 0x0008 == 0 {
                fields.push(FieldInfo {
                    name_index: name_idx,
                    descriptor_index: descriptor_idx,
                });
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
            let mut max_stack = 0u8;
            let mut max_locals = 0u8;
            let mut exception_table: Vec<ExceptionEntry> = Vec::new();

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
                    max_stack = ms.min(255) as u8;
                    max_locals = ml.min(255) as u8;
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
                    // Skip remaining Code sub-attributes
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

        Ok(ClassFile {
            data,
            cp_offsets,
            cp_tags,
            methods,
            class_name_index: class_name_utf8_idx,
            super_class_name_index,
            fields,
            access_flags,
            interfaces,
            bootstrap_methods,
        })
    }
}
