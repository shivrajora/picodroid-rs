/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use heapless::Vec;

// Constant pool tag constants
const TAG_UTF8: u8 = 1;
const TAG_CLASS: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_FIELDREF: u8 = 9;
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

#[derive(Debug)]
pub struct FieldInfo {
    pub name_index: u16,
    #[allow(dead_code)]
    pub descriptor_index: u16,
}

/// A parsed class file backed by a `&'static [u8]` slice in Flash.
#[derive(Debug)]
pub struct ClassFile {
    data: &'static [u8],
    /// Byte offset of each CP entry's *data* (after the tag byte) within `data`.
    /// Index 0 is unused (CP is 1-based); index N corresponds to CP entry N.
    cp_offsets: Vec<usize, 96>,
    /// Tag of each CP entry (same indexing as cp_offsets).
    cp_tags: Vec<u8, 96>,
    pub methods: Vec<MethodInfo, 16>,
    pub class_name_index: u16,       // Utf8 entry for this class's name
    pub super_class_name_index: u16, // Utf8 entry for super class name; 0 = java/lang/Object
    pub fields: Vec<FieldInfo, 8>,   // Instance field declarations (non-static)
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
        let mut cp_offsets: Vec<usize, 96> = Vec::new();
        let mut cp_tags: Vec<u8, 96> = Vec::new();
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

        // Skip interfaces
        let iface_count = c.u16().ok_or("truncated")? as usize;
        c.skip(iface_count * 2).ok_or("truncated")?;

        // Parse instance fields
        let field_count = c.u16().ok_or("truncated")? as usize;
        let mut fields: Vec<FieldInfo, 8> = Vec::new();
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
                fields
                    .push(FieldInfo {
                        name_index: name_idx,
                        descriptor_index: descriptor_idx,
                    })
                    .ok();
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
            super_class_name_index,
            fields,
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

    /// Returns the Utf8 bytes for this class's super class name (e.g. b"apps/Animal").
    /// Returns None if this class directly extends java/lang/Object.
    pub fn super_class_name(&self) -> Option<&'static [u8]> {
        if self.super_class_name_index == 0 {
            return None;
        }
        self.cp_utf8(self.super_class_name_index)
    }

    /// Returns the field name bytes for the field at position `pos` in this class's own
    /// field table (0-based, does not include inherited fields).
    pub fn field_name(&self, pos: usize) -> Option<&'static [u8]> {
        let fi = self.fields.get(pos)?;
        self.cp_utf8(fi.name_index)
    }

    /// Resolves a CONSTANT_Fieldref CP entry to (class_name_utf8, field_name_utf8, descriptor_utf8).
    pub fn cp_fieldref(&self, index: u16) -> Option<(&'static [u8], &'static [u8], &'static [u8])> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_FIELDREF) {
            return None;
        }
        let off = self.cp_offsets[i];
        let class_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        let nat_idx = u16::from_be_bytes([self.data[off + 2], self.data[off + 3]]);

        let ci = class_idx as usize;
        if self.cp_tags.get(ci) != Some(&TAG_CLASS) {
            return None;
        }
        let class_off = self.cp_offsets[ci];
        let class_name_utf8 = u16::from_be_bytes([self.data[class_off], self.data[class_off + 1]]);
        let class_name = self.cp_utf8(class_name_utf8)?;

        let ni = nat_idx as usize;
        if self.cp_tags.get(ni) != Some(&TAG_NAME_AND_TYPE) {
            return None;
        }
        let nat_off = self.cp_offsets[ni];
        let field_name_idx = u16::from_be_bytes([self.data[nat_off], self.data[nat_off + 1]]);
        let descriptor_idx = u16::from_be_bytes([self.data[nat_off + 2], self.data[nat_off + 3]]);

        Some((
            class_name,
            self.cp_utf8(field_name_idx)?,
            self.cp_utf8(descriptor_idx)?,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal valid .class file:
    //   class "TC" extends java/lang/Object
    //   one method: public void run() { return; }
    //
    // Constant pool (8 entries, cp_count=8 means indices 1..7):
    //   #1: Class        -> #2
    //   #2: Utf8         "TC"
    //   #3: Class        -> #4
    //   #4: Utf8         "java/lang/Object"
    //   #5: Utf8         "run"
    //   #6: Utf8         "()V"
    //   #7: Utf8         "Code"
    static MINIMAL_CLASS: &[u8] = &[
        // Magic
        0xCA, 0xFE, 0xBA, 0xBE, // Minor version = 0, Major version = 52 (Java 8)
        0x00, 0x00, 0x00, 0x34, // cp_count = 8  (valid entries are indices 1..7)
        0x00, 0x08, // #1: Class -> #2
        0x07, 0x00, 0x02, // #2: Utf8 "TC"  (length=2)
        0x01, 0x00, 0x02, b'T', b'C', // #3: Class -> #4
        0x07, 0x00, 0x04, // #4: Utf8 "java/lang/Object"  (length=16)
        0x01, 0x00, 0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b',
        b'j', b'e', b'c', b't', // #5: Utf8 "run"  (length=3)
        0x01, 0x00, 0x03, b'r', b'u', b'n', // #6: Utf8 "()V"  (length=3)
        0x01, 0x00, 0x03, b'(', b')', b'V', // #7: Utf8 "Code"  (length=4)
        0x01, 0x00, 0x04, b'C', b'o', b'd', b'e', // access_flags = ACC_PUBLIC (0x0001)
        0x00, 0x01, // this_class = #1
        0x00, 0x01, // super_class = #3
        0x00, 0x03, // interfaces_count = 0
        0x00, 0x00, // fields_count = 0
        0x00, 0x00, // methods_count = 1
        0x00, 0x01,
        // Method: access_flags=0x0001, name_index=#5 ("run"), descriptor_index=#6 ("()V")
        0x00, 0x01, // access_flags
        0x00, 0x05, // name_index = #5
        0x00, 0x06, // descriptor_index = #6
        0x00, 0x01, // attributes_count = 1
        // Code attribute: attr_name_index=#7 ("Code")
        0x00, 0x07, // attr_name_index = #7
        // attr_length = 2(max_stack) + 2(max_locals) + 4(code_length) + 1(bytecode) + 2(exception_table_len) + 2(inner_attributes_count)
        //             = 13
        0x00, 0x00, 0x00, 0x0D, // attr_length = 13
        0x00, 0x01, // max_stack = 1
        0x00, 0x01, // max_locals = 1
        0x00, 0x00, 0x00, 0x01, // code_length = 1
        0xB1, // bytecode: return
        0x00, 0x00, // exception_table_length = 0
        0x00, 0x00, // Code inner attributes_count = 0
        // class attributes_count = 0
        0x00, 0x00,
    ];

    // Wrong magic bytes — should trigger "bad magic" error.
    static BAD_MAGIC: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x34];

    // Only the magic — version/cp bytes are missing, should trigger "truncated".
    static TRUNCATED: &[u8] = &[0xCA, 0xFE, 0xBA, 0xBE];

    #[test]
    fn parse_minimal_class_succeeds() {
        let result = ClassFile::parse(MINIMAL_CLASS);
        assert!(result.is_ok(), "expected Ok but got {:?}", result.err());
    }

    #[test]
    fn class_name_is_tc() {
        let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
        // class_name_index resolves to CP #2, the Utf8 entry for "TC"
        assert_eq!(cf.class_name(), Some(b"TC" as &[u8]));
    }

    #[test]
    fn one_method_parsed() {
        let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
        assert_eq!(cf.methods.len(), 1);
    }

    #[test]
    fn method_name_is_run() {
        let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
        // methods[0].name_index = #5 ("run")
        assert_eq!(cf.cp_utf8(cf.methods[0].name_index), Some(b"run" as &[u8]));
    }

    #[test]
    fn method_code_is_return() {
        let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
        // The only bytecode instruction is 0xB1 (return)
        assert_eq!(cf.method_code(&cf.methods[0]), &[0xB1u8]);
    }

    #[test]
    fn bad_magic_returns_error() {
        let result = ClassFile::parse(BAD_MAGIC);
        assert_eq!(result.unwrap_err(), "bad magic");
    }

    #[test]
    fn truncated_returns_error() {
        let result = ClassFile::parse(TRUNCATED);
        assert!(result.is_err(), "expected Err for truncated input");
    }

    #[test]
    fn cp_utf8_wrong_tag_returns_none() {
        let cf = ClassFile::parse(MINIMAL_CLASS).unwrap();
        // CP #1 is a Class entry (tag=7), not a Utf8 entry — must return None
        assert_eq!(cf.cp_utf8(1), None);
    }
}
