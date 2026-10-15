// SPDX-License-Identifier: GPL-3.0-only
use super::Executor;
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

/// JVM-spec category-2 values occupy two stack slots. Picodroid stores
/// every `Value` in a single slot, so the dup2 / dup_x* family must
/// inspect the top of the stack to pick the right form.
#[inline]
fn is_cat2(v: &Value) -> bool {
    matches!(v, Value::Long(_) | Value::Double(_))
}

/// The value `k` slots below the top (`k` = 1 is the top).
fn below_top(stack: &[Value], k: usize) -> Result<&Value, JvmError> {
    stack
        .len()
        .checked_sub(k)
        .map(|i| &stack[i])
        .ok_or(JvmError::StackUnderflow)
}

/// `dup_x2` / `dup2_x1` / `dup2_x2`: every form copies the top `n` values
/// (one category-2 value, or one or two category-1 values) and inserts the
/// copy `depth` values down. The form is picked from the categories, as the
/// spec's tables do:
///
/// - `dup_x2`  Form1 all cat-1 `v3 v2 v1 → v1 v3 v2 v1` (1, 3);
///   Form2 v2 cat-2 `v2 v1 → v1 v2 v1` (1, 2)
/// - `dup2_x1` Form1 all cat-1 `v3 v2 v1 → v2 v1 v3 v2 v1` (2, 3);
///   Form2 v1 cat-2 `v2 v1 → v1 v2 v1` (1, 2)
/// - `dup2_x2` Form4 v1, v2 cat-2 `v2 v1 → v1 v2 v1` (1, 2);
///   Form2 v1 cat-2 `v3 v2 v1 → v1 v3 v2 v1` (1, 3);
///   Form3 v3 cat-2 `v3 v2 v1 → v2 v1 v3 v2 v1` (2, 3);
///   Form1 all cat-1 `v4 v3 v2 v1 → v2 v1 v4 v3 v2 v1` (2, 4)
///
/// A stack too shallow for the form is a `StackUnderflow`, as before; it is
/// now found before anything moves.
#[cold]
#[inline(never)]
fn dup_x(stack: &mut alloc::vec::Vec<Value>, opcode: u8) -> Result<(), JvmError> {
    let cat2 = |k| below_top(stack, k).map(is_cat2);
    let (n, depth) = match opcode {
        0x5b => {
            if cat2(2)? {
                (1, 2)
            } else {
                (1, 3)
            }
        }
        0x5d => {
            if cat2(1)? {
                (1, 2)
            } else {
                (2, 3)
            }
        }
        _ => match (cat2(1)?, cat2(2)?) {
            (true, true) => (1, 2),
            (true, false) => (1, 3),
            (false, _) => {
                if cat2(3)? {
                    (2, 3)
                } else {
                    (2, 4)
                }
            }
        },
    };
    let len = stack.len();
    if len < depth {
        return Err(JvmError::StackUnderflow);
    }
    // Room first, fallibly, as `Frame::push` grows: past `max_stack` the
    // inserts would otherwise grow the stack infallibly (the `interp` fuzz
    // target).
    stack.try_reserve(n).map_err(|_| JvmError::StackOverflow)?;
    let mut copy = [Value::Null; 2];
    copy[..n].copy_from_slice(&stack[len - n..]);
    for (i, v) in copy[..n].iter().enumerate() {
        stack.insert(len - depth + i, *v);
    }
    Ok(())
}

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_stack(&mut self, opcode: u8, frame: &mut Frame) -> Result<(), JvmError> {
        match opcode {
            // pop
            0x57 => {
                frame.pop()?;
            }

            // pop2
            0x58 => {
                let v1 = frame.pop()?;
                if !is_cat2(&v1) {
                    frame.pop()?;
                }
            }

            // dup
            0x59 => {
                let v = frame.pop()?;
                frame.push(v)?;
                frame.push(v)?;
            }

            // dup_x1 — .., v2, v1 → .., v1, v2, v1  (both cat-1)
            0x5a => {
                let v1 = frame.pop()?;
                let v2 = frame.pop()?;
                frame.push(v1)?;
                frame.push(v2)?;
                frame.push(v1)?;
            }

            // dup2 —
            //   Form1 (cat-1 top):  .., v2, v1 → .., v2, v1, v2, v1
            //   Form2 (cat-2 top):  .., v1     → .., v1, v1
            0x5c => {
                let v1 = frame.pop()?;
                if is_cat2(&v1) {
                    frame.push(v1)?;
                    frame.push(v1)?;
                } else {
                    let v2 = frame.pop()?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                }
            }

            // dup_x2, dup2_x1, dup2_x2: rare in javac output, so one cold
            // copy (`dup_x`) serves all their forms.
            0x5b | 0x5d | 0x5e => dup_x(&mut frame.stack, opcode)?,

            // swap — .., v2, v1 → .., v1, v2  (both cat-1 per spec)
            0x5f => {
                let v1 = frame.pop()?;
                let v2 = frame.pop()?;
                frame.push(v1)?;
                frame.push(v2)?;
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const A: Value = Value::Int(1);
    const B: Value = Value::Int(2);
    const C: Value = Value::Int(3);
    const D: Value = Value::Int(4);
    const L: Value = Value::Long(5);
    const M: Value = Value::Double(6.0);

    fn run(opcode: u8, stack: &[Value]) -> Result<Vec<Value>, JvmError> {
        let mut s = stack.to_vec();
        dup_x(&mut s, opcode).map(|()| s)
    }

    #[test]
    fn dup_x2_forms() {
        // Form1: v3 v2 v1 -> v1 v3 v2 v1; Form2 (v2 cat-2): v2 v1 -> v1 v2 v1
        assert_eq!(run(0x5b, &[D, C, B, A]).unwrap(), [D, A, C, B, A]);
        assert_eq!(run(0x5b, &[D, L, A]).unwrap(), [D, A, L, A]);
    }

    #[test]
    fn dup2_x1_forms() {
        // Form1: v3 v2 v1 -> v2 v1 v3 v2 v1; Form2 (v1 cat-2): v2 v1 -> v1 v2 v1
        assert_eq!(run(0x5d, &[D, C, B, A]).unwrap(), [D, B, A, C, B, A]);
        assert_eq!(run(0x5d, &[D, C, L]).unwrap(), [D, L, C, L]);
    }

    #[test]
    fn dup2_x2_forms() {
        // Form1: v4 v3 v2 v1 -> v2 v1 v4 v3 v2 v1
        assert_eq!(run(0x5e, &[D, C, B, A]).unwrap(), [B, A, D, C, B, A]);
        // Form2 (v1 cat-2): v3 v2 v1 -> v1 v3 v2 v1
        assert_eq!(run(0x5e, &[C, B, L]).unwrap(), [L, C, B, L]);
        // Form3 (v3 cat-2): v3 v2 v1 -> v2 v1 v3 v2 v1
        assert_eq!(run(0x5e, &[M, B, A]).unwrap(), [B, A, M, B, A]);
        // Form4 (v1, v2 cat-2): v2 v1 -> v1 v2 v1
        assert_eq!(run(0x5e, &[M, L]).unwrap(), [L, M, L]);
    }

    #[test]
    fn a_shallow_stack_underflows_untouched() {
        assert!(matches!(run(0x5b, &[B, A]), Err(JvmError::StackUnderflow)));
        assert!(matches!(run(0x5d, &[A]), Err(JvmError::StackUnderflow)));
        assert!(matches!(
            run(0x5e, &[C, B, A]),
            Err(JvmError::StackUnderflow)
        ));
        assert!(matches!(run(0x5e, &[]), Err(JvmError::StackUnderflow)));
    }
}
