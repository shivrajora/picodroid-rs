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

            // dup_x2 —
            //   Form1 (all cat-1):    .., v3, v2, v1 → .., v1, v3, v2, v1
            //   Form2 (v2 is cat-2):  .., v2, v1     → .., v1, v2, v1
            0x5b => {
                let v1 = frame.pop()?;
                let v2 = frame.pop()?;
                if is_cat2(&v2) {
                    frame.push(v1)?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                } else {
                    let v3 = frame.pop()?;
                    frame.push(v1)?;
                    frame.push(v3)?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                }
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

            // dup2_x1 —
            //   Form1 (all cat-1):    .., v3, v2, v1 → .., v2, v1, v3, v2, v1
            //   Form2 (v1 is cat-2):  .., v2, v1     → .., v1, v2, v1
            0x5d => {
                let v1 = frame.pop()?;
                if is_cat2(&v1) {
                    let v2 = frame.pop()?;
                    frame.push(v1)?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                } else {
                    let v2 = frame.pop()?;
                    let v3 = frame.pop()?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                    frame.push(v3)?;
                    frame.push(v2)?;
                    frame.push(v1)?;
                }
            }

            // dup2_x2 — four shapes depending on categories of top two values.
            0x5e => {
                let v1 = frame.pop()?;
                let v2 = frame.pop()?;
                match (is_cat2(&v1), is_cat2(&v2)) {
                    // Form4: v1 cat-2, v2 cat-2 — .., v2, v1 → .., v1, v2, v1
                    (true, true) => {
                        frame.push(v1)?;
                        frame.push(v2)?;
                        frame.push(v1)?;
                    }
                    // Form2: v1 cat-2, v2 cat-1 — .., v3, v2, v1 → .., v1, v3, v2, v1
                    (true, false) => {
                        let v3 = frame.pop()?;
                        frame.push(v1)?;
                        frame.push(v3)?;
                        frame.push(v2)?;
                        frame.push(v1)?;
                    }
                    // Form3: v1 cat-1, v2 cat-1, v3 cat-2 — v3, v2, v1 → v2, v1, v3, v2, v1
                    // Form1: all cat-1 — v4, v3, v2, v1 → v2, v1, v4, v3, v2, v1
                    (false, _) => {
                        let v3 = frame.pop()?;
                        if is_cat2(&v3) {
                            frame.push(v2)?;
                            frame.push(v1)?;
                            frame.push(v3)?;
                            frame.push(v2)?;
                            frame.push(v1)?;
                        } else {
                            let v4 = frame.pop()?;
                            frame.push(v2)?;
                            frame.push(v1)?;
                            frame.push(v4)?;
                            frame.push(v3)?;
                            frame.push(v2)?;
                            frame.push(v1)?;
                        }
                    }
                }
            }

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
