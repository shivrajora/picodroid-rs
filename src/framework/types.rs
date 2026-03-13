#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    Reference(u16), // index into StringTable
    ObjectRef(u16), // index into ObjectHeap
    ArrayRef(u16),  // index into ArrayHeap
    Null,
}

#[derive(Debug, PartialEq)]
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
    ArrayIndexOutOfBounds,
    NegativeArraySize,
    AbstractMethodError,
}
