// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.protobuf`'s natives: micropb over a Java `byte[]` that holds
//! whatever an app received, driven by the `CodedInputStream` calls Java
//! can make in any order.

use pd_protobuf::{read_at, write_at, DecodeError, HeapIoError, ReadOp, WriteOp};
use pico_jvm::array_heap::{ArrayHeap, ATYPE_BYTE};
use pico_jvm::types::Value;

use crate::Bytes;

/// What the wire bytes at `pos..limit` hold for `op`, by a decoder that is
/// not micropb: the value as the Java side sees it and the bytes consumed,
/// or `None` when the stream ends inside the value or a varint runs past
/// ten bytes.
fn reference(bytes: &[u8], op: ReadOp) -> Option<(Option<Value>, usize)> {
    fn varint(bytes: &[u8]) -> Option<(u64, usize)> {
        let mut v = 0u64;
        for (i, &b) in bytes.iter().enumerate().take(10) {
            if i < 9 {
                v |= u64::from(b & 0x7F) << (7 * i);
            } else {
                v |= u64::from(b & 0x7F) << 63;
            }
            if b & 0x80 == 0 {
                return Some((v, i + 1));
            }
        }
        None
    }
    fn fixed(bytes: &[u8], n: usize) -> Option<&[u8]> {
        bytes.get(..n)
    }
    match op {
        ReadOp::Tag => {
            let (v, n) = varint(bytes)?;
            Some((Some(Value::Int(v as u32 as i32)), n))
        }
        ReadOp::Varint32 => {
            let (v, n) = varint(bytes)?;
            Some((Some(Value::Int(v as u32 as i32)), n))
        }
        ReadOp::Varint64 => {
            let (v, n) = varint(bytes)?;
            Some((Some(Value::Long(v as i64)), n))
        }
        ReadOp::Fixed32 => {
            let b = fixed(bytes, 4)?;
            Some((
                Some(Value::Int(u32::from_le_bytes(b.try_into().unwrap()) as i32)),
                4,
            ))
        }
        ReadOp::Fixed64 => {
            let b = fixed(bytes, 8)?;
            Some((
                Some(Value::Long(u64::from_le_bytes(b.try_into().unwrap()) as i64)),
                8,
            ))
        }
        ReadOp::Double => {
            let b = fixed(bytes, 8)?;
            Some((
                Some(Value::Double(f64::from_le_bytes(b.try_into().unwrap()))),
                8,
            ))
        }
        ReadOp::Skip { wire_type } => match wire_type {
            0 => varint(bytes).map(|(_, n)| (None, n)),
            1 => fixed(bytes, 8).map(|_| (None, 8)),
            5 => fixed(bytes, 4).map(|_| (None, 4)),
            2 => {
                // The length is a varint32: javalite and micropb both keep
                // the low 32 bits of a wider one.
                let (len, n) = varint(bytes)?;
                let len = (len as u32) as usize;
                fixed(&bytes[n..], len).map(|_| (None, n + len))
            }
            _ => None,
        },
    }
}

/// Bytes `v` takes as `op` (`computeXxxSizeNoTag`).
fn size_of(op: WriteOp, v: u64) -> usize {
    match op {
        WriteOp::Varint32 => varint_len(u64::from(v as u32)),
        WriteOp::Varint64 => varint_len(v),
        WriteOp::Fixed32 => 4,
        WriteOp::Fixed64 | WriteOp::Double => 8,
    }
}

fn varint_len(mut v: u64) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

fn same_bits(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Double(x), Value::Double(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

fn byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> Option<u16> {
    let idx = arrays.alloc(ATYPE_BYTE, bytes.len() as u16)?;
    for (i, &b) in bytes.iter().enumerate() {
        arrays.store(idx, i, b as i8 as i32)?;
    }
    Some(idx)
}

const READ_OPS: [ReadOp; 6] = [
    ReadOp::Tag,
    ReadOp::Varint32,
    ReadOp::Varint64,
    ReadOp::Fixed32,
    ReadOp::Fixed64,
    ReadOp::Double,
];

const WRITE_OPS: [WriteOp; 5] = [
    WriteOp::Varint32,
    WriteOp::Varint64,
    WriteOp::Fixed32,
    WriteOp::Fixed64,
    WriteOp::Double,
];

/// Input: an op count, a byte array (what the app received), then the
/// script: reads of every kind, skips of every wire type, `pushLimit` /
/// `popLimit` / `readRawBytes` as Java keeps the cursor, a read with a
/// hostile cursor, and write-then-read round trips into a second array.
///
/// Every read agrees with a reference decoder on the value and on the bytes
/// consumed, never passes the limit, and leaves the cursor in place when it
/// fails. A write lands exactly `computeXxxSizeNoTag` bytes or stops at the
/// limit with `Full` and the bytes past it untouched. The heap's integrity
/// check holds at the end.
pub fn protobuf(data: &[u8]) {
    let mut b = Bytes::new(data);
    let n_ops = b.u8() as usize % 64;
    let wire = b.take(256);
    let mut arrays = ArrayHeap::new();
    let Some(buf) = byte_array(&mut arrays, wire) else {
        return;
    };
    let mut pos = 0usize;
    let mut limit = wire.len();
    let mut limits: Vec<usize> = Vec::new();

    for _ in 0..n_ops {
        match b.u8() % 12 {
            op @ 0..=5 => {
                let op = READ_OPS[op as usize];
                let expect = reference(&wire[pos..limit], op);
                match read_at(&arrays, buf, pos, limit, op) {
                    Ok((v, end)) => {
                        let (ev, n) = expect.unwrap_or_else(|| {
                            panic!(
                                "{op:?} at {pos}..{limit} read {v:?}; the reference says truncated"
                            )
                        });
                        assert_eq!(end, pos + n, "{op:?} consumed {} bytes", end - pos);
                        assert!(end <= limit);
                        // A zero tag comes back as 0: `CodedInputStream.readTag`
                        // is what throws on a zero field number.
                        assert!(
                            same_bits(&v.unwrap(), &ev.unwrap()),
                            "{op:?} read {v:?}, reference {ev:?}"
                        );
                        pos = end;
                    }
                    Err(DecodeError::ZeroField) => {
                        assert_eq!(op, ReadOp::Tag);
                        assert_eq!(expect.map(|(v, _)| v), Some(Some(Value::Int(0))));
                    }
                    Err(DecodeError::UnexpectedEof | DecodeError::VarIntLimit) => {
                        assert!(expect.is_none(), "{op:?} refused what the reference reads");
                    }
                    Err(e) => panic!("{op:?}: {e:?}"),
                }
            }
            6 => {
                let wire_type = b.u8() & 7;
                let op = ReadOp::Skip { wire_type };
                let expect = reference(&wire[pos..limit], op);
                match read_at(&arrays, buf, pos, limit, op) {
                    Ok((v, end)) => {
                        assert_eq!(v, None);
                        let (_, n) = expect.unwrap_or_else(|| {
                            panic!(
                                "skipped a truncated wire type {wire_type} at {pos}..{limit}: {:?}",
                                &wire[pos..limit]
                            )
                        });
                        assert_eq!(end, pos + n, "skip of wire type {wire_type}");
                        pos = end;
                    }
                    Err(DecodeError::Deprecation) => assert!(matches!(wire_type, 3 | 4)),
                    Err(DecodeError::UnknownWireType) => assert!(matches!(wire_type, 6 | 7)),
                    Err(DecodeError::UnexpectedEof | DecodeError::VarIntLimit) => {
                        assert!(
                            expect.is_none(),
                            "skip refused what the reference steps over"
                        );
                    }
                    Err(e) => panic!("skip {wire_type}: {e:?}"),
                }
            }
            7 => {
                // pushLimit: Java refuses a limit past the current one.
                let n = b.u16() as usize;
                if pos + n <= limit {
                    limits.push(limit);
                    limit = pos + n;
                }
            }
            8 => {
                if let Some(old) = limits.pop() {
                    limit = old;
                }
            }
            9 => {
                // readRawBytes: a Java-side move within the limit.
                let n = b.u8() as usize;
                if n <= limit - pos {
                    pos += n;
                }
            }
            10 => {
                // A hostile cursor: any array index, any position, any limit.
                let hb = b.u16();
                let hp = b.u16() as usize;
                let hl = b.u16() as usize;
                let op = READ_OPS[b.u8() as usize % READ_OPS.len()];
                if let Ok((_, end)) = read_at(&arrays, hb, hp, hl, op) {
                    let len = arrays.length(hb).map_or(0, usize::from);
                    assert!(end <= hl.min(len), "read past the array or the limit");
                    assert!(end >= hp.min(hl.min(len)));
                }
            }
            _ => {
                // Write, then read what was written.
                let op = WRITE_OPS[b.u8() as usize % WRITE_OPS.len()];
                let v = b.u64();
                let cap = b.below(16);
                let start = b.below(cap + 1);
                let Some(out) = byte_array(&mut arrays, &[0x55u8; 16][..cap]) else {
                    continue;
                };
                let expected = size_of(op, v);
                let (end, r) = write_at(&mut arrays, out, start, cap, v, op);
                assert!(
                    start <= end && end <= cap,
                    "{op:?}: {start}..{end} of {cap}"
                );
                match r {
                    Ok(()) => {
                        assert_eq!(end - start, expected, "{op:?} size");
                        let read = match op {
                            WriteOp::Varint32 => ReadOp::Varint32,
                            WriteOp::Varint64 => ReadOp::Varint64,
                            WriteOp::Fixed32 => ReadOp::Fixed32,
                            WriteOp::Fixed64 => ReadOp::Fixed64,
                            WriteOp::Double => ReadOp::Double,
                        };
                        let (back, at) =
                            read_at(&arrays, out, start, cap, read).unwrap_or_else(|e| {
                                panic!("{op:?} wrote bytes that do not read: {e:?}")
                            });
                        assert_eq!(at, end);
                        let want = match op {
                            WriteOp::Varint32 | WriteOp::Fixed32 => Value::Int(v as u32 as i32),
                            WriteOp::Varint64 | WriteOp::Fixed64 => Value::Long(v as i64),
                            WriteOp::Double => Value::Double(f64::from_bits(v)),
                        };
                        assert!(same_bits(&back.unwrap(), &want), "{op:?} round trip");
                    }
                    Err(HeapIoError::Full) => {
                        assert!(expected > cap - start, "{op:?}: Full with room");
                        for i in end..cap {
                            assert_eq!(arrays.load(out, i), Some(0x55), "wrote past the limit");
                        }
                    }
                    Err(HeapIoError::Invalid) => panic!("{op:?}: a byte[] turned invalid"),
                }
                arrays.free(out);
            }
        }
        assert!(pos <= limit && limit <= wire.len());
    }
    // The wire array is the one allocation left: every write buffer was freed.
    assert!(arrays.is_live(buf));
    assert_eq!(arrays.length(buf).map(usize::from), Some(wire.len()));
}
