// SPDX-License-Identifier: GPL-3.0-only

/// Why a class could not be linked, or a table or section failed
/// validation. Every variant is a refusal: the packer stops, the firmware
/// build panics, an install is rejected. Indices are constant-pool indices
/// (`cp`), table word indices (`word`) or class indices in a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// The class bytes end before a structure they declare.
    Truncated,
    /// Not `0xCAFEBABE`.
    BadMagic,
    /// A class file over 65,535 bytes: every offset here is a `u16`.
    ClassTooLarge,
    /// A constant-pool tag this crate does not know (JVMS §4.4 names 1–20).
    UnknownTag { cp: u16, tag: u8 },
    /// `this_class` is not a `Class` entry naming a `Utf8`.
    BadThisClass,
    /// `super_class` is neither 0 nor a `Class` entry naming a `Utf8`.
    BadSuperClass,
    /// An `interfaces[]` entry is not a `Class` entry naming a `Utf8`.
    BadInterface { index: u16 },
    /// A field or method whose name or descriptor is not a `Utf8` entry.
    BadMember { index: u16 },
    /// A `Methodref` whose class is not a `Class` or whose name-and-type is
    /// not a `NameAndType`.
    BadMethodref { cp: u16 },
    /// A `String` entry that does not name a `Utf8`.
    BadString { cp: u16 },
    /// A method descriptor that does not parse, or has over 255 parameters.
    BadDescriptor { cp: u16 },
    /// A `BootstrapMethods` attribute whose entries overrun its length.
    BadBootstrapMethods,
    /// A class, member or descriptor name that is not valid UTF-8. The
    /// runtime reads these without re-checking, so the builder refuses them.
    BadUtf8 { cp: u16 },
    /// The table's header is not a link table of this version, or its
    /// length words disagree with the bytes it was read from.
    BadHeader,
    /// A table word differs from what the class bytes derive to.
    WordMismatch { word: u16 },
    /// A table word points outside the table or the class bytes.
    BadOffset { word: u16 },
    /// A section header, directory or index that does not fit its bytes.
    BadSection,
    /// A table or index at an address its records cannot be read from.
    Misaligned,
    /// The class index is not sorted by `(hash, idx)`, or names a class
    /// the section does not hold, or names one twice.
    BadIndex,
    /// A class whose own name is not the one it was handed over under (a
    /// packer names classes by path; the path and the bytes disagree).
    NameMismatch { idx: u16 },
    /// Two classes in one set spell the same name.
    DuplicateClass { a: u16, b: u16 },
    /// Two different names in one set hash alike; the set cannot be
    /// searched by hash alone. Rename one (a `--shrink` map rolls a new
    /// spelling) rather than weaken the lookup.
    HashCollision { a: u16, b: u16 },
    /// More classes than a section can index.
    TooManyClasses,
    /// The literal pool does not fit its bytes, a row's bytes, hash or
    /// flags are not what its string derives to, or a class names a row
    /// that holds another string.
    BadLiteral,
    /// A class whose `super_idx` is not where the section holds its
    /// superclass (or names one when the section holds none).
    BadSuperIndex { idx: u16 },
    /// More distinct string constants than a pool can number.
    TooManyLiterals,
    /// The builder's one allocation was refused.
    OutOfMemory,
    /// A builder invariant failed: a bug, not an input problem.
    Internal,
}

impl LinkError {
    /// A short, fixed description — what a panic message or an install
    /// refusal prints.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Truncated => "class file truncated",
            Self::BadMagic => "not a class file (bad magic)",
            Self::ClassTooLarge => "class file over 65535 bytes",
            Self::UnknownTag { .. } => "unknown constant-pool tag",
            Self::BadThisClass => "bad this_class",
            Self::BadSuperClass => "bad super_class",
            Self::BadInterface { .. } => "bad interface entry",
            Self::BadMember { .. } => "field or method name/descriptor is not Utf8",
            Self::BadMethodref { .. } => "malformed Methodref",
            Self::BadString { .. } => "String constant does not name a Utf8",
            Self::BadDescriptor { .. } => "malformed method descriptor",
            Self::BadBootstrapMethods => "malformed BootstrapMethods attribute",
            Self::BadUtf8 { .. } => "name is not valid UTF-8",
            Self::BadHeader => "not a link table (bad header)",
            Self::WordMismatch { .. } => "link table disagrees with its class bytes",
            Self::BadOffset { .. } => "link table offset out of range",
            Self::BadSection => "class section header, directory or index out of bounds",
            Self::Misaligned => "class section is not 4-byte aligned",
            Self::BadIndex => "class index unsorted or inconsistent",
            Self::NameMismatch { .. } => "class name differs from the name it was given",
            Self::DuplicateClass { .. } => "two classes spell the same name",
            Self::HashCollision { .. } => "two class names hash alike",
            Self::TooManyClasses => "too many classes for one section",
            Self::BadLiteral => "string literal pool inconsistent",
            Self::BadSuperIndex { .. } => "superclass index does not name the superclass",
            Self::TooManyLiterals => "too many distinct string constants for one section",
            Self::OutOfMemory => "out of memory",
            Self::Internal => "internal linker error",
        }
    }
}

impl core::fmt::Display for LinkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())?;
        match self {
            Self::UnknownTag { cp, tag } => write!(f, " (cp #{cp}, tag {tag})"),
            Self::BadInterface { index } => write!(f, " (interface {index})"),
            Self::BadMember { index } => write!(f, " (member {index})"),
            Self::BadMethodref { cp }
            | Self::BadString { cp }
            | Self::BadDescriptor { cp }
            | Self::BadUtf8 { cp } => {
                write!(f, " (cp #{cp})")
            }
            Self::WordMismatch { word } | Self::BadOffset { word } => write!(f, " (word {word})"),
            Self::NameMismatch { idx } | Self::BadSuperIndex { idx } => {
                write!(f, " (class {idx})")
            }
            Self::DuplicateClass { a, b } | Self::HashCollision { a, b } => {
                write!(f, " (classes {a} and {b})")
            }
            _ => Ok(()),
        }
    }
}
