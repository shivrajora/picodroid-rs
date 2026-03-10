/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use heapless::Vec;

// Constant pool tag constants
const TAG_UTF8: u8 = 1;
const TAG_CLASS: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_METHODREF: u8 = 10;
const TAG_NAME_AND_TYPE: u8 = 12;

#[derive(Debug)]
#[allow(dead_code)]
pub struct MethodInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    /// Byte offset of the Code attribute's bytecode array inside `data`.
    /// 0 means the method is native (no Code attribute).
    pub code_offset: usize,
    pub code_len: usize,
    pub max_stack: u8,
    pub max_locals: u8,
    pub access_flags: u16,
}

/// A parsed class file backed by a `&'static [u8]` slice in Flash.
pub struct ClassFile {
    data: &'static [u8],
    /// Byte offset of each CP entry's *data* (after the tag byte) within `data`.
    /// Index 0 is unused (CP is 1-based); index N corresponds to CP entry N.
    cp_offsets: Vec<usize, 64>,
    /// Tag of each CP entry (same indexing as cp_offsets).
    cp_tags: Vec<u8, 64>,
    pub methods: Vec<MethodInfo, 16>,
    pub class_name_index: u16, // index of the Utf8 entry for this class's name
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    fn u16(&mut self) -> Option<u16> {
        let hi = self.u8()? as u16;
        let lo = self.u8()? as u16;
        Some((hi << 8) | lo)
    }

    fn u32(&mut self) -> Option<u32> {
        let hi = self.u16()? as u32;
        let lo = self.u16()? as u32;
        Some((hi << 16) | lo)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.pos = self.pos.checked_add(n)?;
        if self.pos > self.data.len() {
            return None;
        }
        Some(())
    }

    fn pos(&self) -> usize {
        self.pos
    }
}

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
        let mut cp_offsets: Vec<usize, 64> = Vec::new();
        let mut cp_tags: Vec<u8, 64> = Vec::new();
        cp_offsets.push(0).ok(); // index 0 placeholder
        cp_tags.push(0).ok();

        let mut idx = 1;
        while idx < cp_count {
            let tag = c.u8().ok_or("truncated")?;
            let data_offset = c.pos();
            cp_tags.push(tag).map_err(|_| "cp too large")?;
            cp_offsets.push(data_offset).map_err(|_| "cp too large")?;

            match tag {
                TAG_UTF8 => {
                    let len = c.u16().ok_or("truncated")? as usize;
                    c.skip(len).ok_or("truncated")?;
                }
                TAG_CLASS => {
                    c.skip(2).ok_or("truncated")?; // name_index
                }
                TAG_STRING => {
                    c.skip(2).ok_or("truncated")?; // string_index
                }
                TAG_METHODREF => {
                    c.skip(4).ok_or("truncated")?; // class_index + name_and_type_index
                }
                TAG_NAME_AND_TYPE => {
                    c.skip(4).ok_or("truncated")?; // name_index + descriptor_index
                }
                // Long/Double take two CP slots (rare in M1 code)
                5 | 6 => {
                    c.skip(8).ok_or("truncated")?;
                    cp_tags.push(0).map_err(|_| "cp too large")?;
                    cp_offsets.push(0).map_err(|_| "cp too large")?;
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
        let _access_flags = c.u16().ok_or("truncated")?;
        let this_class_idx = c.u16().ok_or("truncated")?;
        let _super_class = c.u16().ok_or("truncated")?;

        // Resolve class name: this_class_idx → Class CP entry → Utf8 index
        let class_name_utf8_idx = {
            let ci = this_class_idx as usize;
            if cp_tags.get(ci) != Some(&TAG_CLASS) {
                return Err("bad this_class");
            }
            let off = cp_offsets[ci];
            u16::from_be_bytes([data[off], data[off + 1]])
        };

        // Skip interfaces
        let iface_count = c.u16().ok_or("truncated")? as usize;
        c.skip(iface_count * 2).ok_or("truncated")?;

        // Skip fields
        let field_count = c.u16().ok_or("truncated")? as usize;
        for _ in 0..field_count {
            c.skip(6).ok_or("truncated")?; // access_flags, name_idx, descriptor_idx
            let attr_count = c.u16().ok_or("truncated")? as usize;
            for _ in 0..attr_count {
                c.skip(2).ok_or("truncated")?; // attr name index
                let len = c.u32().ok_or("truncated")? as usize;
                c.skip(len).ok_or("truncated")?;
            }
        }

        // Parse methods
        let method_count = c.u16().ok_or("truncated")? as usize;
        let mut methods: Vec<MethodInfo, 16> = Vec::new();

        for _ in 0..method_count {
            let access_flags = c.u16().ok_or("truncated")?;
            let name_index = c.u16().ok_or("truncated")?;
            let descriptor_index = c.u16().ok_or("truncated")?;
            let attr_count = c.u16().ok_or("truncated")? as usize;

            let mut code_offset = 0usize;
            let mut code_len = 0usize;
            let mut max_stack = 0u8;
            let mut max_locals = 0u8;

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
                    // u16 max_stack, u16 max_locals, u32 code_length, [u8; code_length], ...
                    let ms = c.u16().ok_or("truncated")?;
                    let ml = c.u16().ok_or("truncated")?;
                    let cl = c.u32().ok_or("truncated")? as usize;
                    max_stack = ms.min(255) as u8;
                    max_locals = ml.min(255) as u8;
                    code_offset = c.pos();
                    code_len = cl;
                    // Skip the rest of the Code attribute
                    c.pos = attr_start + attr_len;
                } else {
                    c.skip(attr_len).ok_or("truncated")?;
                }
            }

            methods
                .push(MethodInfo {
                    name_index,
                    descriptor_index,
                    code_offset,
                    code_len,
                    max_stack,
                    max_locals,
                    access_flags,
                })
                .map_err(|_| "too many methods")?;
        }

        Ok(ClassFile {
            data,
            cp_offsets,
            cp_tags,
            methods,
            class_name_index: class_name_utf8_idx,
        })
    }

    /// Returns the Utf8 bytes for the given constant pool index (must be a Utf8 entry).
    pub fn cp_utf8(&self, index: u16) -> Option<&'static [u8]> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_UTF8) {
            return None;
        }
        let off = self.cp_offsets[i];
        let len = u16::from_be_bytes([self.data[off], self.data[off + 1]]) as usize;
        self.data.get(off + 2..off + 2 + len)
    }

    /// Returns the Utf8 bytes for this class's name (e.g. b"apps/HelloWorld").
    pub fn class_name(&self) -> Option<&'static [u8]> {
        self.cp_utf8(self.class_name_index)
    }

    /// Resolves a CONSTANT_String CP entry to its Utf8 bytes.
    pub fn cp_string_utf8(&self, index: u16) -> Option<&'static [u8]> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_STRING) {
            return None;
        }
        let off = self.cp_offsets[i];
        let utf8_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        self.cp_utf8(utf8_idx)
    }

    /// Resolves a CONSTANT_Methodref to (class_name_utf8, method_name_utf8, descriptor_utf8).
    pub fn cp_methodref(
        &self,
        index: u16,
    ) -> Option<(&'static [u8], &'static [u8], &'static [u8])> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_METHODREF) {
            return None;
        }
        let off = self.cp_offsets[i];
        let class_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        let nat_idx = u16::from_be_bytes([self.data[off + 2], self.data[off + 3]]);

        // Resolve class name
        let ci = class_idx as usize;
        if self.cp_tags.get(ci) != Some(&TAG_CLASS) {
            return None;
        }
        let class_off = self.cp_offsets[ci];
        let class_name_utf8 = u16::from_be_bytes([self.data[class_off], self.data[class_off + 1]]);
        let class_name = self.cp_utf8(class_name_utf8)?;

        // Resolve name_and_type
        let ni = nat_idx as usize;
        if self.cp_tags.get(ni) != Some(&TAG_NAME_AND_TYPE) {
            return None;
        }
        let nat_off = self.cp_offsets[ni];
        let method_name_idx = u16::from_be_bytes([self.data[nat_off], self.data[nat_off + 1]]);
        let descriptor_idx = u16::from_be_bytes([self.data[nat_off + 2], self.data[nat_off + 3]]);

        let method_name = self.cp_utf8(method_name_idx)?;
        let descriptor = self.cp_utf8(descriptor_idx)?;

        Some((class_name, method_name, descriptor))
    }

    /// Returns the raw bytecode slice for a method.
    pub fn method_code(&self, m: &MethodInfo) -> &'static [u8] {
        if m.code_offset == 0 {
            &[]
        } else {
            &self.data[m.code_offset..m.code_offset + m.code_len]
        }
    }

    /// Resolves a CONSTANT_Class CP entry to its class name Utf8 bytes.
    pub fn cp_class_name(&self, index: u16) -> Option<&'static [u8]> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_CLASS) {
            return None;
        }
        let off = self.cp_offsets[i];
        let utf8_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        self.cp_utf8(utf8_idx)
    }

    /// Resolves a CONSTANT_Integer CP entry to an i32.
    pub fn cp_integer(&self, index: u16) -> Option<i32> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&3u8) {
            return None;
        }
        let off = self.cp_offsets[i];
        let bytes = [
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ];
        Some(i32::from_be_bytes(bytes))
    }
}
