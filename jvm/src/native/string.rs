use crate::types::{JvmError, Value};

use super::NativeContext;

pub(crate) fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method_name {
        // ── String — non-allocating ──────────────────────────────────
        "length" => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                Some(Ok(Some(Value::Int(s.len() as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "charAt" => {
            if let (Some(Value::Reference(idx)), Some(Value::Int(i))) =
                (ctx.args.first(), ctx.args.get(1))
            {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                let ch = s.as_bytes().get(*i as usize).copied().unwrap_or(0);
                Some(Ok(Some(Value::Int(ch as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "isEmpty" => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                Some(Ok(Some(Value::Int(s.is_empty() as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "equals" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int((sa == sb) as i32))))
            }
            (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "equalsIgnoreCase" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.eq_ignore_ascii_case(sb) as i32))))
            }
            (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "startsWith" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.starts_with(sb) as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "endsWith" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.ends_with(sb) as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "contains" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.contains(sb) as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "indexOf" => {
            match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    // indexOf(String)
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    let result = sa.find(sb).map(|i| i as i32).unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                    // indexOf(char) — char passed as int
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let target = *ch as u8;
                    let result = sa
                        .as_bytes()
                        .iter()
                        .position(|&b| b == target)
                        .map(|i| i as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        "lastIndexOf" => {
            match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let target = *ch as u8;
                    let result = sa
                        .as_bytes()
                        .iter()
                        .rposition(|&b| b == target)
                        .map(|i| i as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    // lastIndexOf(String)
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    let result = sa.rfind(sb).map(|i| i as i32).unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        "compareTo" => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                let result = match sa.cmp(sb) {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                };
                Some(Ok(Some(Value::Int(result))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },

        // ── String — allocating ──────────────────────────────────────
        "substring" => match ctx.args.first() {
            Some(Value::Reference(idx)) => {
                // Copy bytes to owned storage first so the immutable borrow
                // on ctx.strings ends before we call intern_dyn (mutable).
                let owned: Result<alloc::vec::Vec<u8>, JvmError> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let bytes = s.as_bytes();
                    let start = match ctx.args.get(1) {
                        Some(Value::Int(n)) => *n as usize,
                        _ => return Some(Err(JvmError::InvalidReference)),
                    };
                    let end = match ctx.args.get(2) {
                        Some(Value::Int(n)) => *n as usize,
                        None => bytes.len(),
                        _ => return Some(Err(JvmError::InvalidReference)),
                    };
                    if start > end || end > bytes.len() {
                        Err(JvmError::ArrayIndexOutOfBounds)
                    } else {
                        Ok(bytes[start..end].to_vec())
                    }
                };
                match owned {
                    Err(e) => Some(Err(e)),
                    Ok(v) => {
                        let r = ctx.strings.intern_dyn(&v).ok_or(JvmError::StackOverflow);
                        Some(r.map(|idx| Some(Value::Reference(idx))))
                    }
                }
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        "trim" => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let owned: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.trim_matches(|c: char| c.is_ascii_whitespace())
                        .as_bytes()
                        .to_vec()
                };
                let r = ctx
                    .strings
                    .intern_dyn(&owned)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "toUpperCase" => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let upper: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.bytes().map(|b| b.to_ascii_uppercase()).collect()
                };
                let r = ctx
                    .strings
                    .intern_dyn(&upper)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "toLowerCase" => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let lower: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.bytes().map(|b| b.to_ascii_lowercase()).collect()
                };
                let r = ctx
                    .strings
                    .intern_dyn(&lower)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        "valueOf" => {
            // Static method: String.valueOf(int/long/boolean/char/float/double)
            let result: Option<alloc::vec::Vec<u8>> = match ctx.args.first() {
                Some(Value::Int(n)) => {
                    if ctx.descriptor.starts_with("(Z)") {
                        Some(if *n != 0 {
                            b"true".to_vec()
                        } else {
                            b"false".to_vec()
                        })
                    } else if ctx.descriptor.starts_with("(C)") {
                        Some(alloc::vec![(*n as u8).max(0x20)])
                    } else {
                        let mut tmp = [0u8; 12];
                        let bytes = crate::object_heap::int_to_decimal_buf(*n, &mut tmp);
                        Some(bytes.to_vec())
                    }
                }
                Some(Value::Long(n)) => {
                    let mut tmp = [0u8; 21];
                    let bytes = crate::object_heap::long_to_decimal_buf(*n, &mut tmp);
                    Some(bytes.to_vec())
                }
                Some(Value::Float(f)) => {
                    let mut tmp = [0u8; 32];
                    let bytes = crate::object_heap::float_to_str_buf(*f, &mut tmp);
                    Some(bytes.to_vec())
                }
                Some(Value::Double(d)) => {
                    let mut tmp = [0u8; 32];
                    let bytes = crate::object_heap::float_to_str_buf(*d as f32, &mut tmp);
                    Some(bytes.to_vec())
                }
                _ => None,
            };
            if let Some(bytes) = result {
                let r = ctx
                    .strings
                    .intern_dyn(&bytes)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        _ => None,
    }
}
