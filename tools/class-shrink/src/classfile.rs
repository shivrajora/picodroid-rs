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
