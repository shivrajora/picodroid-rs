#[derive(Clone, Copy, Debug)]
pub enum Value {
    Int(i32),
    Reference(u16),
    Null,
}

#[derive(Debug)]
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
