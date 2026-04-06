/// Minimal Java .class file parser for Picodroid Milestone 1.
/// Parses only the subset needed to run a simple static-method call
/// (e.g. HelloWorld.main → Log.i).
use alloc::vec::Vec;

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
    pub max_stack: u8,
    pub max_locals: u8,
    pub access_flags: u16,
    /// Exception table parsed from the Code attribute.
    pub exception_table: Vec<ExceptionEntry>,
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
    cp_offsets: Vec<usize>,
    /// Tag of each CP entry (same indexing as cp_offsets).
    cp_tags: Vec<u8>,
    pub methods: Vec<MethodInfo>,
    pub class_name_index: u16,       // Utf8 entry for this class's name
    pub super_class_name_index: u16, // Utf8 entry for super class name; 0 = java/lang/Object
    pub fields: Vec<FieldInfo>,      // Instance field declarations (non-static)
    pub access_flags: u16,           // ACC_INTERFACE=0x0200, ACC_ABSTRACT=0x0400, etc.
    pub interfaces: Vec<u16>,        // Utf8 indices for each implemented interface name
    pub bootstrap_methods: Vec<BootstrapMethod>, // BootstrapMethods attribute entries
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
