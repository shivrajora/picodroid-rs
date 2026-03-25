/// A value on the JVM operand stack or in a local variable slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// 32-bit signed integer (`int`, `boolean`, `byte`, `char`, `short`).
    Int(i32),
    /// 64-bit signed integer (`long`).  Occupies two stack slots per the JVM spec.
    Long(i64),
    /// 32-bit IEEE 754 float.
    Float(f32),
    /// 64-bit IEEE 754 double.  Occupies two stack slots per the JVM spec.
    Double(f64),
    /// Index into [`crate::heap::StringTable`] — represents a `java/lang/String` reference.
    Reference(u16),
    /// Index into [`crate::object_heap::ObjectHeap`] — represents any object reference.
    ObjectRef(u16),
    /// Index into [`crate::array_heap::ArrayHeap`] — represents an array reference.
    ArrayRef(u16),
    /// The `null` reference.
    Null,
}

/// Errors that can occur during JVM execution.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
pub enum JvmError {
    /// A referenced class was not found in the loaded class set.
    ClassNotFound,
    /// A method could not be located by name in [`crate::Jvm::invoke_static`] /
    /// [`crate::Jvm::invoke_instance`].
    MethodNotFound,
    /// A native method call was not claimed by any [`crate::NativeMethodHandler`].
    NoSuchMethod,
    /// The `.class` file data is malformed or unsupported.
    InvalidBytecode,
    /// The operand stack or a fixed-size internal buffer overflowed.
    StackOverflow,
    /// An operand stack pop was attempted on an empty stack.
    StackUnderflow,
    /// A heap index (string, object, or array) was out of range or the wrong type.
    InvalidReference,
    /// The interpreter encountered a bytecode opcode it does not implement.
    UnsupportedOpcode(u8),
    /// An array index was negative or beyond the array's length.
    ArrayIndexOutOfBounds,
    /// `newarray` / `anewarray` was called with a negative size.
    NegativeArraySize,
    /// An attempt was made to invoke a method on an abstract class or interface.
    AbstractMethodError,
    /// A Java exception was thrown; the `u16` is the [`crate::object_heap::ObjectHeap`]
    /// index of the exception object.
    Exception(u16),
    /// The interpreter was asked to stop cooperatively (e.g. by `pdb install`).
    /// Not a real error — signals a clean exit for app hot-swap.
    Interrupted,
}
