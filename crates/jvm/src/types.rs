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

/// A Java value on the operand stack, in a local, and in transit: what
/// opcode handlers pop and push and what native arms receive. 16 bytes,
/// because a `long` or `double` needs 8 bytes of payload next to its tag.
/// What object fields, the collection buffers and lambda captures *store*
/// is the 8-byte [`Slot`]; a category-2 value crosses into storage as two
/// slots and comes back out as one `Value`. Frames keep `Value`: the
/// per-move tag switch cost the interpreter's integer paths ~45 % in the
/// sim benchmark for under a kilobyte of frames (design doc, "As built").
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// 32-bit signed integer (`int`, `boolean`, `byte`, `char`, `short`).
    Int(i32),
    /// 64-bit signed integer (`long`). Two [`Slot`]s in storage (JVMS §2.6.1).
    Long(i64),
    /// 32-bit IEEE 754 float.
    Float(f32),
    /// 64-bit IEEE 754 double. Two [`Slot`]s in storage (JVMS §2.6.1).
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

impl Value {
    /// Storage width in [`Slot`]s: 2 for `long` and `double`, 1 otherwise
    /// (JVMS §2.6.1 / §2.6.2).
    #[inline]
    pub fn slot_width(&self) -> usize {
        match self {
            Value::Long(_) | Value::Double(_) => 2,
            _ => 1,
        }
    }

    /// Storage width in slots of a field or parameter with this descriptor.
    #[inline]
    pub fn descriptor_slot_width(desc: &[u8]) -> usize {
        match desc.first() {
            Some(b'J') | Some(b'D') => 2,
            _ => 1,
        }
    }

    /// Split into storage slots: `(low, Some(high))` for a category-2 value,
    /// `(slot, None)` otherwise. The low half goes at the lower index.
    #[inline]
    pub fn to_slots(self) -> (Slot, Option<Slot>) {
        match self {
            Value::Int(i) => (Slot::Int(i), None),
            Value::Float(f) => (Slot::Float(f), None),
            Value::Long(l) => {
                let bits = l as u64;
                (
                    Slot::LongLo(bits as u32),
                    Some(Slot::LongHi((bits >> 32) as u32)),
                )
            }
            Value::Double(d) => {
                let bits = d.to_bits();
                (
                    Slot::DoubleLo(bits as u32),
                    Some(Slot::DoubleHi((bits >> 32) as u32)),
                )
            }
            Value::Reference(i) => (Slot::Reference(i), None),
            Value::ObjectRef(i) => (Slot::ObjectRef(i), None),
            Value::ArrayRef(i) => (Slot::ArrayRef(i), None),
            Value::Null => (Slot::Null, None),
        }
    }

    /// Append this value's slots to `out`.
    #[inline]
    pub fn push_slots(self, out: &mut Vec<Slot>) {
        let (lo, hi) = self.to_slots();
        out.push(lo);
        if let Some(hi) = hi {
            out.push(hi);
        }
    }

    /// Total slot width of `values`.
    pub fn slots_len(values: &[Value]) -> usize {
        values.iter().map(Value::slot_width).sum()
    }

    /// Expand `values` into a slot vector, reserving fallibly
    /// (`StackOverflow`, the crate's allocation-failure signal, when the
    /// heap cannot hold it).
    pub fn to_slot_vec(values: &[Value]) -> Result<Vec<Slot>, JvmError> {
        let mut out = Vec::new();
        out.try_reserve(Self::slots_len(values))
            .map_err(|_| JvmError::StackOverflow)?;
        for v in values {
            v.push_slots(&mut out);
        }
        Ok(out)
    }
}

/// What the object fields arena, the collection buffers and lambda
/// captures hold: an 8-byte cell. A `long` or `double` is two cells — the
/// JVM-spec category-2 layout (JVMS §2.6.1) — with the low half at the
/// lower index. The four half variants cost no bytes over a single generic
/// half and keep three things: the GC stays precise (a half can never look
/// like a reference), a lone or mismatched half is detected as a stale
/// native field table, and a reader can rebuild the right [`Value`] without
/// a descriptor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Int(i32),
    Float(f32),
    /// Low 32 bits of a `long`.
    LongLo(u32),
    /// High 32 bits of a `long`; always directly above a [`Slot::LongLo`].
    LongHi(u32),
    /// Low 32 bits of a `double`'s IEEE 754 encoding.
    DoubleLo(u32),
    /// High 32 bits of a `double`; always directly above a [`Slot::DoubleLo`].
    DoubleHi(u32),
    Reference(u16),
    ObjectRef(u16),
    ArrayRef(u16),
    Null,
}

// The whole point: one tag byte plus a 4-byte payload. Identical on every
// target (V1 parity), like `Value` before it.
const _: () = assert!(core::mem::size_of::<Slot>() == 8);
const _: () = assert!(core::mem::size_of::<Value>() == 16);

impl Slot {
    /// `true` for the upper half of a category-2 value.
    #[inline]
    pub fn is_hi(&self) -> bool {
        matches!(self, Slot::LongHi(_) | Slot::DoubleHi(_))
    }

    /// `true` for the lower half of a category-2 value.
    #[inline]
    pub fn is_lo(&self) -> bool {
        matches!(self, Slot::LongLo(_) | Slot::DoubleLo(_))
    }

    /// The one-slot value this cell holds, or `None` for either half of a
    /// category-2 value (use [`Slot::assemble`] with its partner).
    #[inline]
    pub fn to_value(self) -> Option<Value> {
        Some(match self {
            Slot::Int(i) => Value::Int(i),
            Slot::Float(f) => Value::Float(f),
            Slot::Reference(i) => Value::Reference(i),
            Slot::ObjectRef(i) => Value::ObjectRef(i),
            Slot::ArrayRef(i) => Value::ArrayRef(i),
            Slot::Null => Value::Null,
            Slot::LongLo(_) | Slot::LongHi(_) | Slot::DoubleLo(_) | Slot::DoubleHi(_) => {
                return None
            }
        })
    }

    /// The one-slot cell for a category-1 value; `None` for a `long` or
    /// `double`, which need two cells. Used by the collection buffers, whose
    /// elements are always references.
    #[inline]
    pub fn from_narrow(v: Value) -> Option<Slot> {
        match v.to_slots() {
            (s, None) => Some(s),
            _ => None,
        }
    }

    /// Rebuild a category-2 value from its two halves. `None` when the pair
    /// is not a matching `Lo`/`Hi` — a lone or crossed half is bad bytecode
    /// (or a native table addressing the wrong slot), never a value.
    #[inline]
    pub fn assemble(lo: Slot, hi: Slot) -> Option<Value> {
        match (lo, hi) {
            (Slot::LongLo(l), Slot::LongHi(h)) => {
                Some(Value::Long((((h as u64) << 32) | l as u64) as i64))
            }
            (Slot::DoubleLo(l), Slot::DoubleHi(h)) => {
                Some(Value::Double(f64::from_bits(((h as u64) << 32) | l as u64)))
            }
            _ => None,
        }
    }

    /// Read one `Value` starting at `slots[0]`, consuming one or two cells.
    /// Returns the value and its width; `None` on a lone or mismatched half.
    #[inline]
    pub fn read_value(slots: &[Slot]) -> Option<(Value, usize)> {
        let first = *slots.first()?;
        if let Some(v) = first.to_value() {
            return Some((v, 1));
        }
        Some((Slot::assemble(first, *slots.get(1)?)?, 2))
    }

    /// Decode a slot sequence into values, appending to `out`.
    /// `InvalidBytecode` on a lone or mismatched half; `StackOverflow` (the
    /// allocation-failure signal) when `out` cannot grow.
    pub fn to_values(slots: &[Slot], out: &mut Vec<Value>) -> Result<(), JvmError> {
        // One value per slot is the upper bound.
        out.try_reserve(slots.len())
            .map_err(|_| JvmError::StackOverflow)?;
        let mut i = 0;
        while i < slots.len() {
            let (v, w) = Slot::read_value(&slots[i..]).ok_or(JvmError::InvalidBytecode)?;
            out.push(v);
            i += w;
        }
        Ok(())
    }
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
    /// Source line resolved from the `LineNumberTable`. `line-numbers` only.
    #[cfg(feature = "line-numbers")]
    pub line: Option<u16>,
    /// The class's `SourceFile` attribute (`Main.java`). `line-numbers` only.
    #[cfg(feature = "line-numbers")]
    pub source_file: Option<&'static str>,
}

/// Errors that can occur during JVM execution.
#[derive(Debug, PartialEq)]
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
    /// An `invokedynamic` whose bootstrap method is not
    /// `java/lang/invoke/LambdaMetafactory.metafactory`/`altMetafactory`, or
    /// whose implementation handle is a constructor reference
    /// (`REF_newInvokeSpecial`). The payload names the bootstrap owner class
    /// (e.g. `java/lang/invoke/StringConcatFactory` from a class compiled for
    /// Java 9+).
    UnsupportedInvokeDynamic(&'static str),
    /// A Java exception was thrown; the `u16` is the [`crate::object_heap::ObjectHeap`]
    /// index of the exception object.  Used internally during exception unwinding.
    Exception(u16),
    /// A Java exception propagated past all frames without being caught.
    UncaughtException {
        exception_class: &'static str,
        /// Message string captured from `Throwable.<init>(String, ...)`, if any.
        /// `None` for no-arg constructors or messages backed by dynamic strings.
        message: Option<&'static str>,
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
            JvmError::UnsupportedInvokeDynamic(bsm) => {
                write!(f, "UnsupportedInvokeDynamic: bootstrap {}", bsm)
            }
            JvmError::UncaughtException {
                exception_class,
                message,
                trace,
            } => {
                let dotted = |name: &str| name.replace('/', ".");
                write!(
                    f,
                    "Exception in thread \"main\" {}",
                    dotted(exception_class)
                )?;
                if let Some(msg) = message {
                    write!(f, ": {}", msg)?;
                }
                for entry in trace {
                    // Android's StackTraceElement spelling: `(File.java:39)`,
                    // `(Unknown Source:39)` when the class has no SourceFile,
                    // and the bytecode offset when there is no line at all.
                    #[cfg(feature = "line-numbers")]
                    if let Some(line) = entry.line {
                        write!(
                            f,
                            "\n    at {}.{}({}:{})",
                            dotted(entry.class_name),
                            entry.method_name,
                            entry.source_file.unwrap_or("Unknown Source"),
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
    use crate::names::d;

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
        assert_eq!(default_for_descriptor(d::t_String.as_bytes()), Value::Null);
        assert_eq!(default_for_descriptor(b"[I"), Value::Null);
        assert_eq!(default_for_descriptor(b""), Value::Null);
    }

    #[test]
    fn category2_values_split_and_reassemble_exactly() {
        for l in [
            0i64,
            1,
            -1,
            i64::MIN,
            i64::MAX,
            0x1234_5678_9abc_def0u64 as i64,
        ] {
            let (lo, hi) = Value::Long(l).to_slots();
            assert!(lo.is_lo() && hi.unwrap().is_hi());
            assert_eq!(Slot::assemble(lo, hi.unwrap()), Some(Value::Long(l)));
        }
        for d in [0.0f64, -0.0, 1.5, f64::MIN_POSITIVE, f64::INFINITY] {
            let (lo, hi) = Value::Double(d).to_slots();
            let Some(Value::Double(back)) = Slot::assemble(lo, hi.unwrap()) else {
                panic!("double did not reassemble");
            };
            assert_eq!(back.to_bits(), d.to_bits());
        }
        // NaN payloads survive too: the split is on bits, not on the value.
        let nan = f64::from_bits(0x7ff8_dead_beef_0001);
        let (lo, hi) = Value::Double(nan).to_slots();
        let Some(Value::Double(back)) = Slot::assemble(lo, hi.unwrap()) else {
            panic!("NaN did not reassemble");
        };
        assert_eq!(back.to_bits(), nan.to_bits());
    }

    #[test]
    fn narrow_values_are_one_slot_and_round_trip() {
        for v in [
            Value::Int(-7),
            Value::Float(2.5),
            Value::Reference(3),
            Value::ObjectRef(4),
            Value::ArrayRef(5),
            Value::Null,
        ] {
            assert_eq!(v.slot_width(), 1);
            let (s, hi) = v.to_slots();
            assert_eq!(hi, None);
            assert_eq!(s.to_value(), Some(v));
            assert_eq!(Slot::from_narrow(v), Some(s));
        }
        assert_eq!(Slot::from_narrow(Value::Long(1)), None);
        assert_eq!(Slot::from_narrow(Value::Double(1.0)), None);
    }

    #[test]
    fn mismatched_halves_are_rejected() {
        let (llo, lhi) = Value::Long(1).to_slots();
        let (dlo, dhi) = Value::Double(1.0).to_slots();
        assert_eq!(Slot::assemble(llo, dhi.unwrap()), None);
        assert_eq!(Slot::assemble(dlo, lhi.unwrap()), None);
        assert_eq!(Slot::assemble(lhi.unwrap(), llo), None);
        assert_eq!(Slot::assemble(Slot::Int(1), lhi.unwrap()), None);
        assert_eq!(llo.to_value(), None);
        assert_eq!(lhi.unwrap().to_value(), None);
        // A lone low half at the end of a sequence is bad bytecode.
        let mut out = Vec::new();
        assert_eq!(
            Slot::to_values(&[Slot::Int(1), llo], &mut out),
            Err(JvmError::InvalidBytecode)
        );
    }

    #[test]
    fn slot_sequences_decode_to_values() {
        let vals = [
            Value::Int(1),
            Value::Long(-2),
            Value::Null,
            Value::Double(3.0),
        ];
        let slots = Value::to_slot_vec(&vals).unwrap();
        assert_eq!(slots.len(), 6);
        assert_eq!(Value::slots_len(&vals), 6);
        let mut out = Vec::new();
        Slot::to_values(&slots, &mut out).unwrap();
        assert_eq!(out, vals);
        assert_eq!(Value::descriptor_slot_width(b"J"), 2);
        assert_eq!(Value::descriptor_slot_width(b"D"), 2);
        assert_eq!(Value::descriptor_slot_width(b"I"), 1);
        assert_eq!(Value::descriptor_slot_width(b"Ljava/lang/Long;"), 1);
    }
}
