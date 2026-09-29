// SPDX-License-Identifier: GPL-3.0-only
//! Reads through the link table: constant-pool entries, members, code.
//! Every accessor is a few loads from flash; none allocates or parses.

use class_link::classfile::{
    be16, be32, TAG_CLASS, TAG_DOUBLE, TAG_FLOAT, TAG_INTEGER, TAG_INVOKE_DYNAMIC, TAG_LONG,
    TAG_METHOD_HANDLE, TAG_UTF8,
};

use super::{BootstrapMethod, ClassFile, ExceptionTable, FieldInfo, IfaceInfo, MethodInfo};
use crate::names::c;

/// The hash of `java/lang/Object`'s loaded spelling: a class whose
/// superclass hashes to this — and spells it — extends Object, which the
/// hierarchy walks treat as having no superclass.
const OBJECT_HASH: u32 = class_link::name_hash(c::java_lang_Object.as_bytes());

impl ClassFile {
    /// Methods declared in this class.
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn methods(&self) -> &'static [MethodInfo] {
        self.link().methods()
    }

    /// Non-static instance fields declared in this class.
    #[inline]
    pub fn fields(&self) -> &'static [FieldInfo] {
        self.link().fields()
    }

    /// Static fields declared in this class.
    #[inline]
    pub fn static_fields(&self) -> &'static [FieldInfo] {
        self.link().static_fields()
    }

    /// Returns the descriptor bytes for the given FieldInfo (e.g. `b"I"`, `b"Lfoo/Bar;"`).
    #[inline]
    pub fn field_descriptor(&self, fi: &FieldInfo) -> Option<&'static [u8]> {
        self.cp_utf8(fi.descriptor_index)
    }

    /// The directly implemented interfaces: each with its name's offset and
    /// hash, so a hierarchy check compares integers first.
    #[inline]
    pub fn interfaces(&self) -> &'static [IfaceInfo] {
        self.link().interfaces()
    }

    /// The name of interface record `f`.
    #[inline]
    pub fn iface_name(&self, f: &IfaceInfo) -> Option<&'static [u8]> {
        self.view().interface_name(f)
    }

    /// Returns the Utf8 name bytes for the Nth implemented interface (0-based).
    pub fn interface_name(&self, pos: usize) -> Option<&'static [u8]> {
        self.iface_name(self.interfaces().get(pos)?)
    }

    /// Entry `idx` of the `BootstrapMethods` class attribute, decoded from
    /// flash (the table keeps only the attribute's offset).
    pub fn bootstrap_method(&self, idx: u16) -> Option<BootstrapMethod> {
        let base = self.link().bsm_off();
        if base == 0 {
            return None;
        }
        let data = self.data();
        let num_methods = be16(data, base)?;
        if idx >= num_methods {
            return None;
        }
        let mut pos = base + 2;
        for _ in 0..idx {
            let num_args = be16(data, pos + 2)? as usize;
            pos += 4 + 2 * num_args;
        }
        let method_ref = be16(data, pos)?;
        let num_args = be16(data, pos + 2)?;
        Some(BootstrapMethod {
            method_ref,
            args_off: u16::try_from(pos + 4).ok()?,
            num_args,
        })
    }

    /// CP index of bootstrap argument `k` of `b`.
    pub fn bootstrap_argument(&self, b: &BootstrapMethod, k: usize) -> Option<u16> {
        if k >= b.num_args as usize {
            return None;
        }
        be16(self.data(), b.args_off as usize + 2 * k)
    }

    /// The method's exception table, read from the class bytes right after
    /// its bytecode (JVMS §4.7.3). Empty for a native method. The count is
    /// clamped to what the class bytes can hold, so a corrupt table never
    /// reads past them.
    pub fn exception_table(&self, m: &MethodInfo) -> ExceptionTable {
        let data = self.data();
        let empty = ExceptionTable {
            data,
            pos: 0,
            remaining: 0,
        };
        if m.code_offset == 0 {
            return empty;
        }
        let base = m.code_offset as usize + self.method_code_len(m);
        let Some(count) = be16(data, base) else {
            return empty;
        };
        let max_entries = (data.len() - base - 2) / 8;
        ExceptionTable {
            data,
            pos: base + 2,
            remaining: (count as usize).min(max_entries),
        }
    }

    /// Raw access flags bitset.
    #[inline]
    pub fn access_flags(&self) -> u16 {
        self.link().access_flags()
    }

    /// Returns the Utf8 bytes for the given constant pool index (must be a Utf8 entry).
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_utf8(&self, index: u16) -> Option<&'static [u8]> {
        self.view().utf8(index as usize)
    }

    /// [`Self::cp_utf8`] for a slot that holds a class name. Names are used
    /// exactly as the class file spells them: under `--shrink` every table
    /// the JVM matches against is spelled the same way (`crate::names`).
    #[inline]
    pub fn cp_class_utf8(&self, index: u16) -> Option<&'static [u8]> {
        self.cp_utf8(index)
    }

    /// Returns the Utf8 bytes for this class's name (e.g. b"apps/HelloWorld").
    #[inline]
    pub fn class_name(&self) -> Option<&'static [u8]> {
        Some(self.scanned_name())
    }

    /// Resolves a CONSTANT_String CP entry to its Utf8 bytes.
    pub fn cp_string_utf8(&self, index: u16) -> Option<&'static [u8]> {
        self.view().cp_string_utf8(index as usize)
    }

    /// Resolves a CONSTANT_Methodref or CONSTANT_InterfaceMethodref to
    /// (class_name_utf8, method_name_utf8, descriptor_utf8).
    /// Both tags (10 and 11) have the same binary layout.
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_methodref(
        &self,
        index: u16,
    ) -> Option<(&'static [u8], &'static [u8], &'static [u8])> {
        let tag = self.link().cp_tag(index as usize)?;
        if !class_link::classfile::is_methodref(tag) {
            return None;
        }
        self.view().cp_member_ref(index as usize)
    }

    /// The pack-time descriptor of a `Methodref` / `InterfaceMethodref`
    /// entry: its argument count without decoding the constant pool.
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn methodref_desc(&self, index: u16) -> Option<&'static super::MethodrefDesc> {
        self.link().methodref_desc(index as usize)
    }

    /// Returns the raw bytecode slice for a method.
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn method_code(&self, m: &MethodInfo) -> &'static [u8] {
        self.view().method_code(m)
    }

    /// `code_length` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_code_len(&self, m: &MethodInfo) -> usize {
        self.view().method_code_len(m)
    }

    /// `max_stack` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_max_stack(&self, m: &MethodInfo) -> u16 {
        self.view().method_max_stack(m)
    }

    /// `max_locals` of method `m`; 0 for a method without bytecode.
    #[inline]
    pub fn method_max_locals(&self, m: &MethodInfo) -> u16 {
        self.view().method_max_locals(m)
    }

    /// Constant-pool index of method `m`'s name.
    #[inline]
    pub fn method_name_index(&self, m: &MethodInfo) -> Option<u16> {
        self.view().method_name_index(m)
    }

    /// Constant-pool index of method `m`'s descriptor.
    #[inline]
    pub fn method_descriptor_index(&self, m: &MethodInfo) -> Option<u16> {
        self.view().method_descriptor_index(m)
    }

    /// Method `m`'s name.
    #[inline]
    pub fn method_name(&self, m: &MethodInfo) -> Option<&'static [u8]> {
        #[cfg(test)]
        super::METHOD_NAME_READS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.view().method_name(m)
    }

    /// Method `m`'s descriptor.
    #[inline]
    pub fn method_descriptor(&self, m: &MethodInfo) -> Option<&'static [u8]> {
        self.view().method_descriptor(m)
    }

    /// Resolves a CONSTANT_Class CP entry to its class name Utf8 bytes.
    pub fn cp_class_name(&self, index: u16) -> Option<&'static [u8]> {
        self.view().cp_class_name(index as usize)
    }

    /// Returns the Utf8 bytes for this class's super class name (e.g. b"apps/Animal").
    /// Returns None if this class directly extends java/lang/Object.
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn super_class_name(&self) -> Option<&'static [u8]> {
        let l = self.link();
        if l.super_off() == 0 {
            return None;
        }
        let name = self.view().super_name()?;
        if l.super_hash() == OBJECT_HASH && name == c::java_lang_Object.as_bytes() {
            return None;
        }
        Some(name)
    }

    /// [`class_link::name_hash`] of the superclass name; 0 when the class
    /// has none. Object's own hash for a class that extends Object — the
    /// walks compare against [`Self::super_class_name`]'s `None` instead.
    #[inline]
    pub fn super_hash(&self) -> u32 {
        self.link().super_hash()
    }

    /// Returns the field name bytes for the field at position `pos` in this class's own
    /// field table (0-based, does not include inherited fields).
    pub fn field_name(&self, pos: usize) -> Option<&'static [u8]> {
        let fi = self.fields().get(pos)?;
        self.cp_utf8(fi.name_index)
    }

    /// Resolves a CONSTANT_Fieldref CP entry to (class_name_utf8, field_name_utf8, descriptor_utf8).
    #[cfg_attr(feature = "hot-in-ram", link_section = ".data.hot")]
    #[cfg_attr(feature = "hot-in-ram", inline(never))]
    pub fn cp_fieldref(&self, index: u16) -> Option<(&'static [u8], &'static [u8], &'static [u8])> {
        if self.link().cp_tag(index as usize)? != class_link::classfile::TAG_FIELDREF {
            return None;
        }
        self.view().cp_member_ref(index as usize)
    }

    /// Returns true if this class file declares an interface (ACC_INTERFACE).
    pub fn is_interface(&self) -> bool {
        self.access_flags() & 0x0200 != 0
    }

    /// Returns true if this class file is abstract (ACC_ABSTRACT).
    pub fn is_abstract(&self) -> bool {
        self.access_flags() & 0x0400 != 0
    }

    /// The data offset of CP entry `index` when its tag is `tag`.
    #[inline]
    fn cp_data(&self, index: u16, tag: u8) -> Option<usize> {
        let l = self.link();
        if l.cp_tag(index as usize)? != tag {
            return None;
        }
        l.cp_offset(index as usize)
    }

    /// Resolves a CONSTANT_Integer CP entry to an i32.
    pub fn cp_integer(&self, index: u16) -> Option<i32> {
        let off = self.cp_data(index, TAG_INTEGER)?;
        Some(be32(self.data(), off)? as i32)
    }

    /// Resolves a CONSTANT_Float CP entry to an f32.
    pub fn cp_float(&self, index: u16) -> Option<f32> {
        let off = self.cp_data(index, TAG_FLOAT)?;
        Some(f32::from_bits(be32(self.data(), off)?))
    }

    /// Resolves a CONSTANT_Long CP entry to an i64.
    pub fn cp_long(&self, index: u16) -> Option<i64> {
        let off = self.cp_data(index, TAG_LONG)?;
        let hi = be32(self.data(), off)? as u64;
        let lo = be32(self.data(), off + 4)? as u64;
        Some(((hi << 32) | lo) as i64)
    }

    /// Resolves a CONSTANT_Double CP entry to an f64.
    pub fn cp_double(&self, index: u16) -> Option<f64> {
        let off = self.cp_data(index, TAG_DOUBLE)?;
        let hi = be32(self.data(), off)? as u64;
        let lo = be32(self.data(), off + 4)? as u64;
        Some(f64::from_bits((hi << 32) | lo))
    }

    /// Resolves a CONSTANT_NameAndType CP entry to (name_utf8, descriptor_utf8).
    pub fn cp_name_and_type(&self, index: u16) -> Option<(&'static [u8], &'static [u8])> {
        self.view().cp_name_and_type(index as usize)
    }

    /// Resolves a CONSTANT_MethodHandle CP entry to (reference_kind, reference_index).
    pub fn cp_method_handle(&self, index: u16) -> Option<(u8, u16)> {
        let off = self.cp_data(index, TAG_METHOD_HANDLE)?;
        let data = self.data();
        Some((*data.get(off)?, be16(data, off + 1)?))
    }

    /// Resolves a CONSTANT_InvokeDynamic CP entry to
    /// (bootstrap_method_attr_index, name_and_type_index).
    pub fn cp_invoke_dynamic(&self, index: u16) -> Option<(u16, u16)> {
        let off = self.cp_data(index, TAG_INVOKE_DYNAMIC)?;
        let data = self.data();
        Some((be16(data, off)?, be16(data, off + 2)?))
    }

    /// Whether CP entry `index` is a `Class` entry.
    #[inline]
    pub fn cp_is_class(&self, index: u16) -> bool {
        self.link().cp_tag(index as usize) == Some(TAG_CLASS)
    }

    /// Whether CP entry `index` is a `Utf8` entry.
    #[inline]
    pub fn cp_is_utf8(&self, index: u16) -> bool {
        self.link().cp_tag(index as usize) == Some(TAG_UTF8)
    }

    /// The class's `SourceFile` attribute (`Main.java`), when it carries one.
    #[cfg(feature = "line-numbers")]
    pub fn source_file(&self) -> Option<&'static [u8]> {
        let idx = self.link().source_file_idx();
        if idx == 0 {
            return None;
        }
        self.cp_utf8(idx)
    }

    /// Maps a bytecode PC to its source line number via a linear scan of the
    /// Flash-backed LineNumberTable. Returns `None` if no LNT was recorded
    /// (native method, compiled without `-g:lines`, or a stripped class).
    #[cfg(feature = "line-numbers")]
    pub fn pc_to_line(&self, m: &MethodInfo, pc: usize) -> Option<u16> {
        let off = m.lnt_offset as usize;
        let data = self.data();
        if off == 0 || off + 2 > data.len() {
            return None;
        }
        let entry_count = u16::from_be_bytes([data[off], data[off + 1]]) as usize;
        let entries_start = off + 2;
        // The attribute's own length was checked at link time; clamp against
        // the class bytes so a corrupt count can never read past them.
        let max_entries = (data.len() - entries_start) / 4;
        let n = entry_count.min(max_entries);
        let mut best: Option<u16> = None;
        for i in 0..n {
            let base = entries_start + i * 4;
            let start_pc = u16::from_be_bytes([data[base], data[base + 1]]) as usize;
            let line_num = u16::from_be_bytes([data[base + 2], data[base + 3]]);
            if start_pc <= pc {
                best = Some(line_num);
            } else {
                // Entries are sorted ascending by start_pc per JVMS §4.7.12.
                break;
            }
        }
        best
    }
}
