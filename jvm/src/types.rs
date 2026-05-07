// SPDX-License-Identifier: GPL-3.0-only
use alloc::vec::Vec;
use core::fmt;

/// Identifies which heap entity a Java monitor is associated with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorKey {
    Object(u16),
    Array(u16),
    String(u16),
}

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

/// Returns the JVMS §2.3 default value for a field with the given descriptor.
///
/// Per JVMS §2.4 and §5.5 step 2, every instance field is set to this value on
/// object creation and every static field is set to this value before the
/// declaring class's `<clinit>` runs.
pub fn default_for_descriptor(desc: &[u8]) -> Value {
    match desc.first() {
        Some(b'I') | Some(b'S') | Some(b'B') | Some(b'C') | Some(b'Z') => Value::Int(0),
        Some(b'J') => Value::Long(0),
        Some(b'F') => Value::Float(0.0),
        Some(b'D') => Value::Double(0.0),
        Some(b'L') | Some(b'[') => Value::Null,
        _ => Value::Null,
    }
}

/// One frame in a Java stack trace (class, method, bytecode offset).
#[derive(Debug, PartialEq)]
pub struct StackTraceEntry {
    pub class_name: &'static str,
    pub method_name: &'static str,
    pub pc: usize,
    /// Source line resolved from the `LineNumberTable`. Debug builds only.
    #[cfg(debug_assertions)]
    pub line: Option<u16>,
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
    /// index of the exception object.  Used internally during exception unwinding.
    Exception(u16),
    /// A Java exception propagated past all frames without being caught.
    UncaughtException {
        exception_class: &'static str,
        trace: Vec<StackTraceEntry>,
    },
    /// A `monitorexit` was executed by a thread that does not own the monitor.
    IllegalMonitorState,
    /// The interpreter was asked to stop cooperatively (e.g. by `pdb install`).
    /// Not a real error — signals a clean exit for app hot-swap.
    Interrupted,
}

impl fmt::Display for JvmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JvmError::UnsupportedOpcode(op) => {
                write!(f, "UnsupportedOpcode: {} (0x{:02x})", opcode_name(*op), op)
            }
            JvmError::UncaughtException {
                exception_class,
                trace,
            } => {
                let dotted = |name: &str| name.replace('/', ".");
                write!(
                    f,
                    "Exception in thread \"main\" {}",
                    dotted(exception_class)
                )?;
                for entry in trace {
                    #[cfg(debug_assertions)]
                    if let Some(line) = entry.line {
                        write!(
                            f,
                            "\n    at {}.{}(:{})",
                            dotted(entry.class_name),
                            entry.method_name,
                            line
                        )?;
                        continue;
                    }
                    write!(
                        f,
                        "\n    at {}.{}(pc={})",
                        dotted(entry.class_name),
                        entry.method_name,
                        entry.pc
                    )?;
                }
                Ok(())
            }
            other => fmt::Debug::fmt(other, f),
        }
    }
}

/// Returns a human-readable name for a JVM bytecode opcode.
pub fn opcode_name(op: u8) -> &'static str {
    match op {
        0x00 => "nop",
        0x01 => "aconst_null",
        0x02..=0x08 => "iconst",
        0x09..=0x0a => "lconst",
        0x0b..=0x0d => "fconst",
        0x0e..=0x0f => "dconst",
        0x10 => "bipush",
        0x11 => "sipush",
        0x12 => "ldc",
        0x13 => "ldc_w",
        0x14 => "ldc2_w",
        0x15..=0x19 => "xload",
        0x1a..=0x2d => "xload_N",
        0x2e..=0x35 => "xaload",
        0x36..=0x3a => "xstore",
        0x3b..=0x4e => "xstore_N",
        0x4f..=0x56 => "xastore",
        0x57 => "pop",
        0x58 => "pop2",
        0x59 => "dup",
        0x60..=0x84 => "arithmetic",
        0x85..=0x93 => "x2y",
        0x94..=0x98 => "xcmp",
        0x99..=0x9e => "ifxx",
        0x9f..=0xa4 => "if_icmpxx",
        0xa5..=0xa6 => "if_acmpxx",
        0xa7 => "goto",
        0xaa => "tableswitch",
        0xab => "lookupswitch",
        0xac..=0xb0 => "xreturn",
        0xb1 => "return",
        0xb2 => "getstatic",
        0xb3 => "putstatic",
        0xb4 => "getfield",
        0xb5 => "putfield",
        0xb6 => "invokevirtual",
        0xb7 => "invokespecial",
        0xb8 => "invokestatic",
        0xb9 => "invokeinterface",
        0xba => "invokedynamic",
        0xbb => "new",
        0xbc => "newarray",
        0xbd => "anewarray",
        0xbe => "arraylength",
        0xbf => "athrow",
        0xc0 => "checkcast",
        0xc1 => "instanceof",
        0xc2 => "monitorenter",
        0xc3 => "monitorexit",
        0xc6 => "ifnull",
        0xc7 => "ifnonnull",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_descriptor_covers_primitives_and_refs() {
        assert_eq!(default_for_descriptor(b"I"), Value::Int(0));
        assert_eq!(default_for_descriptor(b"S"), Value::Int(0));
        assert_eq!(default_for_descriptor(b"B"), Value::Int(0));
        assert_eq!(default_for_descriptor(b"C"), Value::Int(0));
        assert_eq!(default_for_descriptor(b"Z"), Value::Int(0));
        assert_eq!(default_for_descriptor(b"J"), Value::Long(0));
        assert_eq!(default_for_descriptor(b"F"), Value::Float(0.0));
        assert_eq!(default_for_descriptor(b"D"), Value::Double(0.0));
        assert_eq!(default_for_descriptor(b"Ljava/lang/String;"), Value::Null);
        assert_eq!(default_for_descriptor(b"[I"), Value::Null);
        assert_eq!(default_for_descriptor(b""), Value::Null);
    }
}
