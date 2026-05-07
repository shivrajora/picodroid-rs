// SPDX-License-Identifier: GPL-3.0-only
/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use alloc::vec::Vec;
use core::cell::OnceCell;

mod accessors;
mod parse;
#[cfg(test)]
mod tests;

// Constant pool tag constants
const TAG_UTF8: u8 = 1;
const TAG_CLASS: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_FIELDREF: u8 = 9;
const TAG_METHODREF: u8 = 10;
const TAG_NAME_AND_TYPE: u8 = 12;
const TAG_METHOD_HANDLE: u8 = 15;
const TAG_METHOD_TYPE: u8 = 16;
const TAG_INVOKE_DYNAMIC: u8 = 18;

/// One entry in the BootstrapMethods class attribute.
#[derive(Debug, Clone)]
pub struct BootstrapMethod {
    /// CP index of CONSTANT_MethodHandle for the bootstrap method.
    pub method_ref: u16,
    /// CP indices of the bootstrap arguments.
    pub arguments: Vec<u16>,
}

/// One entry in a method's exception table (try/catch region).
#[derive(Debug, Clone, Copy)]
pub struct ExceptionEntry {
    /// Start of the guarded region (inclusive), as a bytecode offset.
    pub start_pc: u16,
    /// End of the guarded region (exclusive), as a bytecode offset.
    pub end_pc: u16,
    /// Bytecode offset of the catch handler.
    pub handler_pc: u16,
    /// CP index of the caught class (CONSTANT_Class), or 0 to catch any (finally).
    pub catch_type_index: u16,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MethodInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    /// Byte offset of the Code attribute's bytecode array inside `data`.
    /// 0 means the method is native (no Code attribute).
    pub code_offset: usize,
    pub code_len: usize,
    pub max_stack: u16,
    pub max_locals: u16,
    pub access_flags: u16,
    /// Exception table parsed from the Code attribute.
    pub exception_table: Vec<ExceptionEntry>,
    /// Byte offset of the LineNumberTable body (entry_count u16 + entries) inside
    /// the Flash-backed class data. 0 = not present. Debug builds only.
    #[cfg(debug_assertions)]
    pub lnt_offset: usize,
    /// Byte length of the LineNumberTable body (= 2 + entry_count*4). Debug builds only.
    #[cfg(debug_assertions)]
    pub lnt_len: usize,
}

#[derive(Debug)]
pub struct FieldInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
}

/// Fully-parsed internals of a class file.  Populated lazily on first access.
#[derive(Debug)]
pub(crate) struct Parsed {
    /// Byte offset of each CP entry's *data* (after the tag byte) within `data`.
    /// Index 0 is unused (CP is 1-based); index N corresponds to CP entry N.
    pub cp_offsets: Vec<usize>,
    /// Tag of each CP entry (same indexing as cp_offsets).
    pub cp_tags: Vec<u8>,
    pub methods: Vec<MethodInfo>,
    pub class_name_index: u16,
    pub super_class_name_index: u16,
    pub fields: Vec<FieldInfo>,
    pub static_fields: Vec<FieldInfo>,
    pub access_flags: u16,
    pub interfaces: Vec<u16>,
    pub bootstrap_methods: Vec<BootstrapMethod>,
}

/// A class file backed by a `&'static [u8]` slice in Flash.
///
/// The class name is scanned eagerly at registration so name-based lookups
/// (e.g. `find_method`, `class_name_to_static_in`) can iterate all registered
/// classes without forcing a full parse.  All other accessors route through
/// [`Parsed`] which is populated on first access.
#[derive(Debug)]
pub struct ClassFile {
    data: &'static [u8],
    /// Pre-scanned class name (Flash-backed UTF8 bytes from the constant pool).
    name: &'static [u8],
    /// Fully-parsed internals; filled on first access via `parsed()`.
    parsed: OnceCell<Parsed>,
}

impl ClassFile {
    /// Returns the raw bytecode slice backing this class file.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// Returns a reference to the parsed internals, parsing on first call.
    ///
    /// Panics only if the class data is malformed — registration (`register`)
    /// already validated the constant pool enough to extract the class name,
    /// so in practice a subsequent full parse should not fail.
    pub(crate) fn parsed(&self) -> &Parsed {
        self.parsed.get_or_init(|| {
            Parsed::parse(self.data).expect("class file became unparseable after registration")
        })
    }

    /// Returns `true` if the full parse has already been performed.
    pub fn is_parsed(&self) -> bool {
        self.parsed.get().is_some()
    }

    pub(crate) fn new_lazy(data: &'static [u8], name: &'static [u8]) -> Self {
        Self {
            data,
            name,
            parsed: OnceCell::new(),
        }
    }

    pub(crate) fn new_eager(data: &'static [u8], name: &'static [u8], parsed: Parsed) -> Self {
        let cell = OnceCell::new();
        let _ = cell.set(parsed);
        Self {
            data,
            name,
            parsed: cell,
        }
    }

    /// Returns the pre-scanned class name (does not trigger a full parse).
    pub(crate) fn scanned_name(&self) -> &'static [u8] {
        self.name
    }
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
