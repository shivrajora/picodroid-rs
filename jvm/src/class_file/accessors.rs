use super::{
    ClassFile, MethodInfo, TAG_CLASS, TAG_FIELDREF, TAG_INVOKE_DYNAMIC, TAG_METHODREF,
    TAG_METHOD_HANDLE, TAG_METHOD_TYPE, TAG_NAME_AND_TYPE, TAG_STRING, TAG_UTF8,
};

impl ClassFile {
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

    /// Resolves a CONSTANT_Methodref or CONSTANT_InterfaceMethodref to
    /// (class_name_utf8, method_name_utf8, descriptor_utf8).
    /// Both tags (10 and 11) have the same binary layout.
    pub fn cp_methodref(
        &self,
        index: u16,
    ) -> Option<(&'static [u8], &'static [u8], &'static [u8])> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_METHODREF) && self.cp_tags.get(i) != Some(&11u8) {
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

    /// Returns true if this class file declares an interface (ACC_INTERFACE).
    pub fn is_interface(&self) -> bool {
        self.access_flags & 0x0200 != 0
    }

    /// Returns true if this class file is abstract (ACC_ABSTRACT).
    pub fn is_abstract(&self) -> bool {
        self.access_flags & 0x0400 != 0
    }

    /// Returns the Utf8 name bytes for the Nth implemented interface (0-based).
    #[allow(dead_code)]
    pub fn interface_name(&self, pos: usize) -> Option<&'static [u8]> {
        self.cp_utf8(*self.interfaces.get(pos)?)
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

    /// Resolves a CONSTANT_Float CP entry to an f32.
    pub fn cp_float(&self, index: u16) -> Option<f32> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&4u8) {
            return None;
        }
        let off = self.cp_offsets[i];
        let bits = u32::from_be_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ]);
        Some(f32::from_bits(bits))
    }

    /// Resolves a CONSTANT_Long CP entry to an i64.
    pub fn cp_long(&self, index: u16) -> Option<i64> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&5u8) {
            return None;
        }
        let off = self.cp_offsets[i];
        Some(i64::from_be_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
            self.data[off + 4],
            self.data[off + 5],
            self.data[off + 6],
            self.data[off + 7],
        ]))
    }

    /// Resolves a CONSTANT_Double CP entry to an f64.
    pub fn cp_double(&self, index: u16) -> Option<f64> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&6u8) {
            return None;
        }
        let off = self.cp_offsets[i];
        let bits = u64::from_be_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
            self.data[off + 4],
            self.data[off + 5],
            self.data[off + 6],
            self.data[off + 7],
        ]);
        Some(f64::from_bits(bits))
    }

    /// Resolves a CONSTANT_NameAndType CP entry to (name_utf8, descriptor_utf8).
    pub fn cp_name_and_type(&self, index: u16) -> Option<(&'static [u8], &'static [u8])> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_NAME_AND_TYPE) {
            return None;
        }
        let off = self.cp_offsets[i];
        let name_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        let desc_idx = u16::from_be_bytes([self.data[off + 2], self.data[off + 3]]);
        Some((self.cp_utf8(name_idx)?, self.cp_utf8(desc_idx)?))
    }

    /// Resolves a CONSTANT_MethodHandle CP entry to (reference_kind, reference_index).
    pub fn cp_method_handle(&self, index: u16) -> Option<(u8, u16)> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_METHOD_HANDLE) {
            return None;
        }
        let off = self.cp_offsets[i];
        let ref_kind = self.data[off];
        let ref_idx = u16::from_be_bytes([self.data[off + 1], self.data[off + 2]]);
        Some((ref_kind, ref_idx))
    }

    /// Resolves a CONSTANT_MethodType CP entry to its descriptor Utf8 index.
    #[allow(dead_code)]
    pub fn cp_method_type(&self, index: u16) -> Option<u16> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_METHOD_TYPE) {
            return None;
        }
        let off = self.cp_offsets[i];
        Some(u16::from_be_bytes([self.data[off], self.data[off + 1]]))
    }

    /// Resolves a CONSTANT_InvokeDynamic CP entry to
    /// (bootstrap_method_attr_index, name_and_type_index).
    pub fn cp_invoke_dynamic(&self, index: u16) -> Option<(u16, u16)> {
        let i = index as usize;
        if self.cp_tags.get(i) != Some(&TAG_INVOKE_DYNAMIC) {
            return None;
        }
        let off = self.cp_offsets[i];
        let bsm_idx = u16::from_be_bytes([self.data[off], self.data[off + 1]]);
        let nat_idx = u16::from_be_bytes([self.data[off + 2], self.data[off + 3]]);
        Some((bsm_idx, nat_idx))
    }
}
