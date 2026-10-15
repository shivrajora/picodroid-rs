// SPDX-License-Identifier: GPL-3.0-only
//! The interpreter on bytecode the pack-time checker accepts. That checker
//! bounds operands and branch targets only — no stack or local typing, no
//! instruction boundaries — so the interpreter's own run-time checks are
//! what stands between an unsigned PAPK and the device. This target runs
//! arbitrary checked bytecode and holds them to it: every outcome is a
//! return or a Java error, never a Rust panic or an ASan report.

use class_link::build::{build_section, simple_class};
use pico_jvm::types::{JvmError, Value};
use pico_jvm::{Jvm, NativeContext, NativeMethodHandler, SharedJvmHeap};

/// No platform natives: everything falls through to the built-in handler.
struct NoNatives;

impl NativeMethodHandler for NoNatives {
    fn dispatch(
        &mut self,
        _class_name: &str,
        _method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        None
    }
}

/// Input: the bytecode of `static int m()` (`max_stack` 4, `max_locals` 4)
/// in a class with no other members.
pub fn interp(data: &[u8]) {
    let code = &data[..data.len().min(512)];
    // The interpreter has no instruction budget, and Java may loop; only
    // code that provably ends is run, so a hang is a finding.
    if !terminates(code) {
        return;
    }
    let class = simple_class(b"F", b"java/lang/Object", Some(code));
    // What the packer refuses never reaches a device.
    if build_section(&[&class]).is_err() {
        return;
    }
    // `load_class` wants `'static` bytes, as flash is. One small leak per
    // input; scripts/fuzz.sh runs this target with leak detection off.
    let class: &'static [u8] = Box::leak(class.into_boxed_slice());
    // A board's heap, not the host's: see `budget`.
    crate::budget::with_budget(HEAP_BUDGET, || {
        let mut jvm = Jvm::new();
        if jvm.load_class(class).is_err() {
            return;
        }
        let mut heap = SharedJvmHeap::new();
        let _ = jvm.invoke_static("F", "m", &mut heap, &mut NoNatives);
    });
}

/// More than any board gives the JVM (host structures are wider), small
/// enough that allocating until refused takes milliseconds.
const HEAP_BUDGET: usize = 1 << 20;

/// Whether `code` certainly ends: decoded straight through, every
/// instruction is whole, every branch goes forward to an instruction
/// boundary, and there is no subroutine (`jsr`, `ret`). Execution then only
/// ever moves forward through those instructions, and the class has no
/// method refs to recurse through and no exception table to loop through.
fn terminates(code: &[u8]) -> bool {
    let mut starts = vec![false; code.len() + 1];
    let mut targets = Vec::new();
    let mut pc = 0usize;
    while pc < code.len() {
        starts[pc] = true;
        let op = code[pc];
        let first_target = targets.len();
        let be16 = |at: usize| -> Option<i32> {
            Some(i16::from_be_bytes([*code.get(at)?, *code.get(at + 1)?]) as i32)
        };
        let be32 = |at: usize| -> Option<i32> {
            Some(i32::from_be_bytes([
                *code.get(at)?,
                *code.get(at + 1)?,
                *code.get(at + 2)?,
                *code.get(at + 3)?,
            ]))
        };
        let len = match op {
            0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xbc => 2,
            0x11 | 0x13 | 0x14 | 0x84 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => 3,
            0x99..=0xa7 | 0xc6 | 0xc7 => {
                let Some(off) = be16(pc + 1) else {
                    return false;
                };
                targets.push(pc as i64 + off as i64);
                3
            }
            0xc8 => {
                let Some(off) = be32(pc + 1) else {
                    return false;
                };
                targets.push(pc as i64 + off as i64);
                5
            }
            0xb9 | 0xba => 5,
            0xc5 => 4,
            0xc4 => match code.get(pc + 1) {
                Some(0x84) => 6,
                Some(0x15..=0x19 | 0x36..=0x3a) => 4,
                _ => return false,
            },
            0xaa | 0xab => {
                let base = (pc + 4) & !3;
                let Some(default) = be32(base) else {
                    return false;
                };
                targets.push(pc as i64 + default as i64);
                let (count, entry, first) = if op == 0xaa {
                    let (Some(lo), Some(hi)) = (be32(base + 4), be32(base + 8)) else {
                        return false;
                    };
                    if hi < lo {
                        return false;
                    }
                    ((hi as i64 - lo as i64 + 1) as usize, 4, base + 12)
                } else {
                    let Some(n) = be32(base + 4) else {
                        return false;
                    };
                    if n < 0 {
                        return false;
                    }
                    (n as usize, 8, base + 12)
                };
                if count > code.len() {
                    return false;
                }
                for i in 0..count {
                    let Some(off) = be32(first + i * entry) else {
                        return false;
                    };
                    targets.push(pc as i64 + off as i64);
                }
                first + count * entry - pc - if op == 0xaa { 0 } else { 4 }
            }
            // jsr, ret, jsr_w: subroutines can return backwards.
            0xa8 | 0xa9 | 0xc9 => return false,
            _ => 1,
        };
        if targets[first_target..].iter().any(|&t| t <= pc as i64) {
            return false;
        }
        pc += len;
    }
    pc == code.len()
        && targets
            .iter()
            .all(|&t| (t as usize) < code.len() && starts[t as usize])
}

#[cfg(test)]
mod tests {
    use super::terminates;

    #[test]
    fn forward_code_terminates_and_loops_do_not() {
        assert!(terminates(&[0x03, 0xAC])); // iconst_0; ireturn
        assert!(terminates(&[0xA7, 0x00, 0x03, 0x03, 0xAC])); // goto +3
        assert!(!terminates(&[0xA7, 0x00, 0x00])); // goto 0
        assert!(!terminates(&[0xA7, 0x00, 0x02, 0x03, 0xAC])); // into an operand
        assert!(!terminates(&[0x10])); // truncated bipush
    }
}
