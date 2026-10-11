// SPDX-License-Identifier: GPL-3.0-only
//! Pack-time verification of a method's bytecode: every instruction's
//! operands lie inside the code, every branch and switch target does too,
//! and switch tables are well-formed. The interpreter indexes `code`
//! directly on the strength of this (a truncated `wide`, a `tableswitch`
//! with `low = i32::MIN` or a negative `npairs` used to panic it — a reset
//! on device), so the check runs where the link table is derived: once per
//! class at pack time, again when a PAPK is installed, never at run time.
//!
//! What is not checked: that targets land on instruction boundaries (a
//! bitmap of starts per method is more than an install should allocate),
//! the operand stack and local-variable discipline, and constant-pool
//! operand kinds (the pool is walked separately). None of those can read
//! outside `code`.

use crate::error::LinkError;

/// Variable-length instructions, resolved by hand below.
const SPECIAL: u8 = 0xFF;

/// Operand bytes following each opcode, [`SPECIAL`] for the variable ones,
/// `None` for an opcode a class file may not contain.
fn operand_len(op: u8) -> Option<u8> {
    Some(match op {
        0x00..=0x0f => 0,       // nop, aconst_null, iconst_*, lconst_*, fconst_*, dconst_*
        0x10 => 1,              // bipush
        0x11 => 2,              // sipush
        0x12 => 1,              // ldc
        0x13 | 0x14 => 2,       // ldc_w, ldc2_w
        0x15..=0x19 => 1,       // iload..aload
        0x1a..=0x35 => 0,       // xload_<n>, xaload
        0x36..=0x3a => 1,       // istore..astore
        0x3b..=0x83 => 0,       // xstore_<n>, xastore, stack ops, arithmetic
        0x84 => 2,              // iinc
        0x85..=0x98 => 0,       // conversions, comparisons
        0x99..=0xa8 => 2,       // if<cond>, if_icmp<cond>, if_acmp<cond>, goto, jsr
        0xa9 => 1,              // ret
        0xaa | 0xab => SPECIAL, // tableswitch, lookupswitch
        0xac..=0xb1 => 0,       // returns
        0xb2..=0xb8 => 2,       // getstatic..invokestatic
        0xb9 | 0xba => 4,       // invokeinterface, invokedynamic
        0xbb => 2,              // new
        0xbc => 1,              // newarray
        0xbd => 2,              // anewarray
        0xbe | 0xbf => 0,       // arraylength, athrow
        0xc0 | 0xc1 => 2,       // checkcast, instanceof
        0xc2 | 0xc3 => 0,       // monitorenter, monitorexit
        0xc4 => SPECIAL,        // wide
        0xc5 => 3,              // multianewarray
        0xc6 | 0xc7 => 2,       // ifnull, ifnonnull
        0xc8 | 0xc9 => 4,       // goto_w, jsr_w
        // breakpoint, impdep1/2 and the unassigned range never appear in a
        // class file (JVMS §6.2).
        _ => return None,
    })
}

fn be16(code: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes([*code.get(at)?, *code.get(at + 1)?]))
}

fn be32(code: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_be_bytes([
        *code.get(at)?,
        *code.get(at + 1)?,
        *code.get(at + 2)?,
        *code.get(at + 3)?,
    ]))
}

/// A branch from the instruction at `pc` by `offset` must land inside the
/// code.
fn check_target(code: &[u8], pc: usize, offset: i64) -> Result<(), LinkError> {
    let target = pc as i64 + offset;
    if target < 0 || target >= code.len() as i64 {
        return Err(LinkError::BadBytecode { pc: pc as u16 });
    }
    Ok(())
}

/// Check one method's `code` (JVMS §4.9.1 static constraints, the part
/// the interpreter relies on). `Err(BadBytecode { pc })` names the first
/// instruction that fails.
pub fn check_code(code: &[u8]) -> Result<(), LinkError> {
    let len = code.len();
    // JVMS §4.7.3: 0 < code_length < 65536.
    if len == 0 || len > 65535 {
        return Err(LinkError::BadBytecode { pc: 0 });
    }
    let mut pc = 0usize;
    while pc < len {
        let bad = LinkError::BadBytecode { pc: pc as u16 };
        let op = code[pc];
        let operands = operand_len(op).ok_or(bad)?;
        if operands != SPECIAL {
            let next = pc + 1 + operands as usize;
            if next > len {
                return Err(bad);
            }
            match op {
                0x99..=0xa8 | 0xc6 | 0xc7 => {
                    check_target(code, pc, be16(code, pc + 1).ok_or(bad)? as i64)?;
                }
                0xc8 | 0xc9 => {
                    check_target(code, pc, be32(code, pc + 1).ok_or(bad)? as i64)?;
                }
                // invokeinterface: count != 0 and a zero fourth byte;
                // invokedynamic: two zero bytes.
                0xb9 if code[pc + 3] == 0 || code[pc + 4] != 0 => return Err(bad),
                0xba if code[pc + 3] != 0 || code[pc + 4] != 0 => return Err(bad),
                // newarray: a primitive array type code (T_BOOLEAN..T_LONG).
                0xbc if !(4..=11).contains(&code[pc + 1]) => return Err(bad),
                // multianewarray: at least one dimension.
                0xc5 if code[pc + 3] == 0 => return Err(bad),
                _ => {}
            }
            pc = next;
            continue;
        }
        match op {
            0xaa => {
                // tableswitch: 0-3 pad bytes to a 4-byte boundary, then
                // default, low, high (i32 each) and high - low + 1 offsets.
                let start = (pc + 4) & !3;
                let default = be32(code, start).ok_or(bad)?;
                let low = be32(code, start + 4).ok_or(bad)?;
                let high = be32(code, start + 8).ok_or(bad)?;
                if low > high {
                    return Err(bad);
                }
                let count = (high as i64 - low as i64 + 1) as usize;
                let table = start + 12;
                let end = count
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(table))
                    .ok_or(bad)?;
                if end > len {
                    return Err(bad);
                }
                check_target(code, pc, default as i64)?;
                for i in 0..count {
                    check_target(code, pc, be32(code, table + 4 * i).ok_or(bad)? as i64)?;
                }
                pc = end;
            }
            0xab => {
                // lookupswitch: pad, default, npairs >= 0, then npairs
                // (match, offset) pairs with the matches strictly ascending.
                let start = (pc + 4) & !3;
                let default = be32(code, start).ok_or(bad)?;
                let npairs = be32(code, start + 4).ok_or(bad)?;
                if npairs < 0 {
                    return Err(bad);
                }
                let npairs = npairs as usize;
                let table = start + 8;
                let end = npairs
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(table))
                    .ok_or(bad)?;
                if end > len {
                    return Err(bad);
                }
                check_target(code, pc, default as i64)?;
                let mut prev: Option<i32> = None;
                for i in 0..npairs {
                    let key = be32(code, table + 8 * i).ok_or(bad)?;
                    if prev.is_some_and(|p| p >= key) {
                        return Err(bad);
                    }
                    prev = Some(key);
                    check_target(code, pc, be32(code, table + 8 * i + 4).ok_or(bad)? as i64)?;
                }
                pc = end;
            }
            0xc4 => {
                // wide: a load/store/ret with a u16 index, or iinc with a
                // u16 index and an i16 constant.
                let real = *code.get(pc + 1).ok_or(bad)?;
                let next = match real {
                    0x15..=0x19 | 0x36..=0x3a | 0xa9 => pc + 4,
                    0x84 => pc + 6,
                    _ => return Err(bad),
                };
                if next > len {
                    return Err(bad);
                }
                pc = next;
            }
            _ => return Err(bad),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad_at(code: &[u8], pc: u16) {
        assert_eq!(
            check_code(code),
            Err(LinkError::BadBytecode { pc }),
            "{code:x?}"
        );
    }

    #[test]
    fn straight_line_code_passes() {
        // iconst_1; istore_1; iload_1; bipush 7; iadd; ireturn
        assert_eq!(check_code(&[0x04, 0x3C, 0x1B, 0x10, 7, 0x60, 0xAC]), Ok(()));
        // aload_0; invokevirtual #8; ireturn
        assert_eq!(check_code(&[0x2A, 0xB6, 0x00, 0x08, 0xAC]), Ok(()));
    }

    #[test]
    fn truncated_operands_are_refused() {
        bad_at(&[0x10], 0); // bipush without its byte
        bad_at(&[0x04, 0xB6, 0x00], 1); // invokevirtual with one index byte
        bad_at(&[0xC4, 0x15, 0x00], 0); // wide iload with one index byte
        bad_at(&[0xC4, 0x84, 0x00, 0x01, 0x00], 0); // wide iinc without its constant
        bad_at(&[0xC4, 0x60], 0); // wide before an opcode it does not widen
    }

    #[test]
    fn unassigned_opcodes_are_refused() {
        bad_at(&[0xCA], 0); // breakpoint
        bad_at(&[0xFE], 0); // impdep1
        bad_at(&[0x04, 0xD0], 1);
    }

    #[test]
    fn branches_must_land_inside_the_code() {
        assert_eq!(check_code(&[0xA7, 0x00, 0x03, 0x00, 0xB1]), Ok(())); // goto +3
        bad_at(&[0xA7, 0x00, 0x05, 0xB1], 0); // goto past the end
        bad_at(&[0x00, 0xA7, 0xFF, 0xFC, 0xB1], 1); // goto before the start
        bad_at(&[0xB1, 0xC8, 0x7F, 0xFF, 0xFF, 0xFF], 1); // goto_w far away
        assert_eq!(check_code(&[0x03, 0x99, 0x00, 0x03, 0xB1]), Ok(())); // ifeq +3
    }

    #[test]
    fn tableswitch_tables_are_bounded_and_ordered() {
        // iconst_0; tableswitch (pad 2) default=+13 low=0 high=1 [+13, +13]; return
        let mut code = alloc::vec![0x03, 0xAA, 0, 0];
        for w in [13i32, 0, 1, 13, 13] {
            code.extend_from_slice(&w.to_be_bytes());
        }
        code.push(0xB1);
        assert_eq!(check_code(&code), Ok(()));
        // high < low (low is at 8..12, high at 12..16)
        let mut swapped = code.clone();
        swapped[8..12].copy_from_slice(&1i32.to_be_bytes());
        swapped[12..16].copy_from_slice(&0i32.to_be_bytes());
        bad_at(&swapped, 1);
        // low = i32::MIN: the count overflows any table
        let mut huge = code.clone();
        huge[8..12].copy_from_slice(&i32::MIN.to_be_bytes());
        bad_at(&huge, 1);
        // a target outside the code
        let mut far = code.clone();
        far[20..24].copy_from_slice(&200i32.to_be_bytes());
        bad_at(&far, 1);
        // the table itself truncated
        bad_at(&code[..20], 1);
    }

    #[test]
    fn lookupswitch_tables_are_bounded_sorted_and_non_negative() {
        // iconst_0; lookupswitch (pad 2) default=+17 npairs=2 (1→+17) (5→+17); return
        let mut code = alloc::vec![0x03, 0xAB, 0, 0];
        for w in [17i32, 2, 1, 17, 5, 17] {
            code.extend_from_slice(&w.to_be_bytes());
        }
        code.push(0xB1);
        assert_eq!(check_code(&code), Ok(()));
        let mut negative = code.clone();
        negative[8..12].copy_from_slice(&(-1i32).to_be_bytes());
        bad_at(&negative, 1);
        let mut unsorted = code.clone();
        unsorted[12..16].copy_from_slice(&9i32.to_be_bytes());
        bad_at(&unsorted, 1);
        let mut far = code.clone();
        far[16..20].copy_from_slice(&(-50i32).to_be_bytes());
        bad_at(&far, 1);
        bad_at(&code[..12], 1);
    }

    #[test]
    fn invoke_and_array_operand_shapes_are_checked() {
        assert_eq!(check_code(&[0xB9, 0, 1, 1, 0, 0xB1]), Ok(()));
        bad_at(&[0xB9, 0, 1, 0, 0, 0xB1], 0); // invokeinterface count 0
        bad_at(&[0xB9, 0, 1, 1, 7, 0xB1], 0); // invokeinterface fourth byte set
        bad_at(&[0xBA, 0, 1, 0, 7, 0xB1], 0); // invokedynamic reserved bytes set
        assert_eq!(check_code(&[0x03, 0xBC, 10, 0xB0]), Ok(())); // newarray int
        bad_at(&[0x03, 0xBC, 3, 0xB0], 1); // newarray with no such type
        bad_at(&[0xC5, 0, 1, 0, 0xB0], 0); // multianewarray with 0 dimensions
    }

    #[test]
    fn empty_code_is_refused() {
        bad_at(&[], 0);
    }
}
