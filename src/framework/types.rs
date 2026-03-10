#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Reference(u16), // index into StringTable
    ObjectRef(u16), // index into ObjectHeap
    Null,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum JvmError {
    ClassNotFound,
    MethodNotFound,
    NoSuchMethod,
    InvalidBytecode,
    StackOverflow,
    StackUnderflow,
    InvalidReference,
    UnsupportedOpcode(u8),
}
