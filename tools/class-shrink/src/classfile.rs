// SPDX-License-Identifier: GPL-3.0-only
//! Minimal JVM class file parser/serializer.
//!
//! Only the constant pool is decoded — every other section (access flags,
//! interfaces, fields, methods, attributes) is kept as opaque trailing bytes.
//! That's sufficient for class-name shrinking because every identifier or
//! descriptor the JVM sees flows through a `CONSTANT_Utf8_info` entry, and
//! all other references into the CP use u16 indices that are position-stable
//! across rewrites.
//!
//! Rewriting a Utf8 entry changes the CP's byte length but does NOT shift
//! CP indices. The trailing section is therefore byte-copyable verbatim.
//!
//! [`ClassFile::members`] walks the tail read-only (attributes skipped by
//! length) so `cut-release` can enumerate declared members; member
//! *rewriting* is not done here — the Gradle-side ASM pass
//! (`buildSrc/.../ShrinkMembersTask.kt`) owns that, because it rebuilds the
//! constant pool and so never has to split a Utf8 slot shared between a
//! member name and an `ldc` string literal.

use std::collections::HashSet;
use std::io;

/// One constant-pool entry. Non-Utf8 entries are stored as opaque payload
/// bytes so the serializer can write them back unchanged.
#[derive(Clone, Debug)]
pub enum CpEntry {
    /// `CONSTANT_Utf8_info` (tag 1). Variable-length UTF-8 (modified, but we
    /// treat it as raw bytes — we only rewrite when replacing class-name
    /// substrings, which are ASCII).
    Utf8(Vec<u8>),
    /// Any other tag. `tag` is the first byte; `payload` holds the bytes
    /// that follow (fixed size per tag per JVMS §4.4).
    Other { tag: u8, payload: Vec<u8> },
    /// Phantom slot occupied by the second half of a `CONSTANT_Long_info`
    /// or `CONSTANT_Double_info` (JVMS §4.4.5).
    LongOrDoubleTail,
}

/// Parsed class file with a decoded constant pool and opaque tail.
pub struct ClassFile {
    /// Bytes before the CP count word (magic + minor + major, 8 bytes).
    pub header: Vec<u8>,
    /// Constant pool entries. `entries[0]` is always a dummy slot (JVM CP
    /// indices are 1-based). `Long` / `Double` occupy two slots; the second
    /// is `LongOrDoubleTail`.
    pub entries: Vec<CpEntry>,
    /// Bytes from `access_flags` to EOF. Never mutated.
    pub tail: Vec<u8>,
}

impl ClassFile {
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 10 {
            return Err(invalid("truncated class file header"));
        }
        if &data[0..4] != b"\xCA\xFE\xBA\xBE" {
            return Err(invalid("bad class file magic"));
        }
        let header = data[0..8].to_vec();
        let mut p = 8usize;
        let cp_count = u16::from_be_bytes([data[p], data[p + 1]]) as usize;
        p += 2;

        let mut entries: Vec<CpEntry> = Vec::with_capacity(cp_count);
        entries.push(CpEntry::Other {
            tag: 0,
            payload: Vec::new(),
        }); // 0th slot is reserved

        let mut i = 1;
        while i < cp_count {
            if p >= data.len() {
                return Err(invalid("truncated constant pool"));
            }
            let tag = data[p];
            p += 1;
            let entry = match tag {
                1 => {
                    // Utf8: u2 length + bytes
                    if p + 2 > data.len() {
                        return Err(invalid("truncated Utf8 length"));
                    }
                    let len = u16::from_be_bytes([data[p], data[p + 1]]) as usize;
                    p += 2;
                    if p + len > data.len() {
                        return Err(invalid("truncated Utf8 bytes"));
                    }
                    let bytes = data[p..p + len].to_vec();
                    p += len;
                    CpEntry::Utf8(bytes)
                }
                // Fixed-size payloads per JVMS §4.4.
                3 | 4 => read_fixed(data, &mut p, tag, 4)?, // Integer / Float
                5 | 6 => read_fixed(data, &mut p, tag, 8)?, // Long / Double
                7 | 8 => read_fixed(data, &mut p, tag, 2)?, // Class / String
                9..=11 => read_fixed(data, &mut p, tag, 4)?, // *ref
                12 => read_fixed(data, &mut p, tag, 4)?,    // NameAndType
                15 => read_fixed(data, &mut p, tag, 3)?,    // MethodHandle
                16 => read_fixed(data, &mut p, tag, 2)?,    // MethodType
                17 | 18 => read_fixed(data, &mut p, tag, 4)?, // Dynamic / InvokeDynamic
                19 | 20 => read_fixed(data, &mut p, tag, 2)?, // Module / Package
                _ => return Err(invalid(&format!("unsupported CP tag {tag} at index {i}"))),
            };
            let is_long_or_double = matches!(tag, 5 | 6);
            entries.push(entry);
            i += 1;
            if is_long_or_double {
                entries.push(CpEntry::LongOrDoubleTail);
                i += 1;
            }
        }

        let tail = data[p..].to_vec();
        Ok(Self {
            header,
            entries,
            tail,
        })
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.header.len() + self.tail.len() + 64);
        out.extend_from_slice(&self.header);
        out.extend_from_slice(&(self.entries.len() as u16).to_be_bytes());
        for e in self.entries.iter().skip(1) {
            match e {
                CpEntry::Utf8(bytes) => {
                    out.push(1);
                    out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
                    out.extend_from_slice(bytes);
                }
                CpEntry::Other { tag, payload } => {
                    out.push(*tag);
                    out.extend_from_slice(payload);
                }
                CpEntry::LongOrDoubleTail => {
                    // Written as part of the preceding Long/Double entry — skip.
                }
            }
        }
        out.extend_from_slice(&self.tail);
        out
    }

    /// Iterate over mutable references to every Utf8 entry's byte vec.
    pub fn utf8_entries_mut(&mut self) -> impl Iterator<Item = &mut Vec<u8>> {
        self.entries.iter_mut().filter_map(|e| match e {
            CpEntry::Utf8(b) => Some(b),
            _ => None,
        })
    }

    /// Which constant-pool entries point at each Utf8 slot, by role. Lets
    /// callers tell a class name (`CONSTANT_Class`), a descriptor
    /// (`CONSTANT_NameAndType` / `CONSTANT_MethodType`) and a string literal
    /// (`CONSTANT_String`) apart even though all three are plain Utf8 bytes.
    /// javac dedupes identical Utf8s, so one slot can carry several roles.
    /// Own-member descriptors and attribute payloads (`Signature`, …) are
    /// referenced only from the opaque tail and appear in no set.
    pub fn utf8_refs(&self) -> Utf8Refs {
        let mut refs = Utf8Refs::default();
        for e in &self.entries {
            let CpEntry::Other { tag, payload } = e else {
                continue;
            };
            let idx = |at: usize| -> Option<usize> {
                Some(u16::from_be_bytes([*payload.get(at)?, *payload.get(at + 1)?]) as usize)
            };
            match tag {
                7 => refs.class_names.extend(idx(0)),
                8 => refs.strings.extend(idx(0)),
                12 => refs.descriptors.extend(idx(2)),
                16 => refs.descriptors.extend(idx(0)),
                _ => {}
            }
        }
        refs
    }
}

/// One declared field or method, as read by [`ClassFile::members`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberInfo {
    pub access_flags: u16,
    pub name: Vec<u8>,
    pub descriptor: Vec<u8>,
}

/// The declared members of a class file plus its own access flags.
#[derive(Clone, Debug, Default)]
pub struct Members {
    /// The class's `access_flags` (`ACC_ANNOTATION` = 0x2000, `ACC_ENUM` = 0x4000, …).
    pub class_access: u16,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
}

impl ClassFile {
    fn utf8(&self, idx: usize) -> io::Result<&[u8]> {
        match self.entries.get(idx) {
            Some(CpEntry::Utf8(b)) => Ok(b),
            _ => Err(invalid(&format!("CP #{idx} is not a Utf8"))),
        }
    }

    /// Enumerate the declared fields and methods by walking the tail:
    /// `access_flags, this_class, super_class, interfaces[], fields[],
    /// methods[]`, with every attribute skipped by its `attribute_length`
    /// (JVMS §4.1). Read-only; nothing is interpreted past the member
    /// headers.
    pub fn members(&self) -> io::Result<Members> {
        let t = &self.tail;
        let mut p = 0usize;
        let u2 = |p: &mut usize| -> io::Result<usize> {
            let v = t
                .get(*p..*p + 2)
                .ok_or_else(|| invalid("truncated class body"))?;
            *p += 2;
            Ok(u16::from_be_bytes([v[0], v[1]]) as usize)
        };
        let class_access = u2(&mut p)? as u16;
        let _this = u2(&mut p)?;
        let _super = u2(&mut p)?;
        let ifaces = u2(&mut p)?;
        p += 2 * ifaces;
        let mut out = Members {
            class_access,
            ..Default::default()
        };
        for kind in 0..2 {
            let count = u2(&mut p)?;
            for _ in 0..count {
                let access_flags = u2(&mut p)? as u16;
                let name = self.utf8(u2(&mut p)?)?.to_vec();
                let descriptor = self.utf8(u2(&mut p)?)?.to_vec();
                let attrs = u2(&mut p)?;
                for _ in 0..attrs {
                    let _name = u2(&mut p)?;
                    let len = t
                        .get(p..p + 4)
                        .ok_or_else(|| invalid("truncated attribute"))?;
                    p += 4 + u32::from_be_bytes([len[0], len[1], len[2], len[3]]) as usize;
                    if p > t.len() {
                        return Err(invalid("attribute overruns class body"));
                    }
                }
                let info = MemberInfo {
                    access_flags,
                    name,
                    descriptor,
                };
                if kind == 0 {
                    out.fields.push(info);
                } else {
                    out.methods.push(info);
                }
            }
        }
        Ok(out)
    }

    /// Member names this class *references* — the `name_index` of every
    /// `CONSTANT_NameAndType` (field refs, method refs, `invokedynamic`
    /// call-site names). Together with [`members`](Self::members) this is
    /// the complete set of member names a class file spells out.
    pub fn referenced_member_names(&self) -> Vec<&[u8]> {
        let mut out = Vec::new();
        for e in &self.entries {
            let CpEntry::Other { tag: 12, payload } = e else {
                continue;
            };
            let idx = u16::from_be_bytes([payload[0], payload[1]]) as usize;
            if let Some(CpEntry::Utf8(b)) = self.entries.get(idx) {
                out.push(b.as_slice());
            }
        }
        out
    }
}

/// Utf8 slot indices grouped by the kind of entry that references them; see
/// [`ClassFile::utf8_refs`].
#[derive(Debug, Default)]
pub struct Utf8Refs {
    /// Referenced by a `CONSTANT_Class` — bare internal class names (or
    /// array descriptors such as `[Ljava/lang/String;`).
    pub class_names: HashSet<usize>,
    /// Referenced as the descriptor of a `NameAndType` or `MethodType`.
    pub descriptors: HashSet<usize>,
    /// Referenced by a `CONSTANT_String` — `ldc` string literals.
    pub strings: HashSet<usize>,
}

fn read_fixed(data: &[u8], p: &mut usize, tag: u8, len: usize) -> io::Result<CpEntry> {
    if *p + len > data.len() {
        return Err(invalid(&format!(
            "truncated CP entry tag {tag} ({len} bytes)"
        )));
    }
    let payload = data[*p..*p + len].to_vec();
    *p += len;
    Ok(CpEntry::Other { tag, payload })
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handwritten tiny class: public class A {}.
    ///
    /// Produced by `echo 'public class A {}' > A.java && javac A.java &&
    /// xxd A.class` — we hardcode to keep the test self-contained.
    fn sample_class_a() -> Vec<u8> {
        // minimal javac-8 output for `class A {}`:
        // CP: #1=Methodref #3.#10, #2=Class #11, #3=Class #12, #4=Utf8 "<init>",
        //     #5=Utf8 "()V", #6=Utf8 "Code", #7=Utf8 "LineNumberTable",
        //     #8=Utf8 "SourceFile", #9=Utf8 "A.java", #10=NameAndType #4:#5,
        //     #11=Utf8 "A", #12=Utf8 "java/lang/Object"
        // Regenerated via javac; bytes taken from a real build for stability.
        #[rustfmt::skip]
        const BYTES: &[u8] = &[
            0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34, 0x00, 0x0D, 0x0A,
            0x00, 0x03, 0x00, 0x0A, 0x07, 0x00, 0x0B, 0x07, 0x00, 0x0C, 0x01,
            0x00, 0x06, 0x3C, 0x69, 0x6E, 0x69, 0x74, 0x3E, 0x01, 0x00, 0x03,
            0x28, 0x29, 0x56, 0x01, 0x00, 0x04, 0x43, 0x6F, 0x64, 0x65, 0x01,
            0x00, 0x0F, 0x4C, 0x69, 0x6E, 0x65, 0x4E, 0x75, 0x6D, 0x62, 0x65,
            0x72, 0x54, 0x61, 0x62, 0x6C, 0x65, 0x01, 0x00, 0x0A, 0x53, 0x6F,
            0x75, 0x72, 0x63, 0x65, 0x46, 0x69, 0x6C, 0x65, 0x01, 0x00, 0x06,
            0x41, 0x2E, 0x6A, 0x61, 0x76, 0x61, 0x0C, 0x00, 0x04, 0x00, 0x05,
            0x01, 0x00, 0x01, 0x41, 0x01, 0x00, 0x10, 0x6A, 0x61, 0x76, 0x61,
            0x2F, 0x6C, 0x61, 0x6E, 0x67, 0x2F, 0x4F, 0x62, 0x6A, 0x65, 0x63,
            0x74, 0x00, 0x21, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x04, 0x00, 0x05, 0x00, 0x01, 0x00,
            0x06, 0x00, 0x00, 0x00, 0x1D, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x05, 0x2A, 0xB7, 0x00, 0x01, 0xB1, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x07, 0x00, 0x00, 0x00, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x02, 0x00, 0x09,
        ];
        BYTES.to_vec()
    }

    #[test]
    fn round_trip_identity() {
        let bytes = sample_class_a();
        let cf = ClassFile::parse(&bytes).unwrap();
        let back = cf.serialize();
        assert_eq!(back, bytes, "round-trip must be byte-identical");
    }

    #[test]
    fn finds_utf8_entries() {
        let bytes = sample_class_a();
        let mut cf = ClassFile::parse(&bytes).unwrap();
        let utf8s: Vec<Vec<u8>> = cf.utf8_entries_mut().map(|b| b.clone()).collect();
        assert!(utf8s.contains(&b"java/lang/Object".to_vec()));
        assert!(utf8s.contains(&b"A".to_vec()));
    }

    #[test]
    fn enumerates_members_and_references() {
        let cf = ClassFile::parse(&sample_class_a()).unwrap();
        let m = cf.members().unwrap();
        assert_eq!(m.class_access, 0x0021);
        assert!(m.fields.is_empty());
        assert_eq!(m.methods.len(), 1);
        assert_eq!(m.methods[0].name, b"<init>");
        assert_eq!(m.methods[0].descriptor, b"()V");
        assert_eq!(m.methods[0].access_flags, 0x0001);
        assert_eq!(cf.referenced_member_names(), vec![b"<init>".as_slice()]);
    }

    #[test]
    fn rewrites_class_name() {
        let mut bytes = sample_class_a();
        let mut cf = ClassFile::parse(&bytes).unwrap();
        for u in cf.utf8_entries_mut() {
            if u == b"java/lang/Object" {
                *u = b"j/l/Object".to_vec();
            }
        }
        let out = cf.serialize();
        assert_ne!(out, bytes);
        // Re-parse and confirm structure still valid.
        let cf2 = ClassFile::parse(&out).unwrap();
        let names: Vec<&[u8]> = cf2
            .entries
            .iter()
            .filter_map(|e| match e {
                CpEntry::Utf8(b) => Some(b.as_slice()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&b"j/l/Object".as_ref()));
        assert!(!names.contains(&b"java/lang/Object".as_ref()));
        let _ = &mut bytes; // suppress unused_mut
    }
}
