// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    array_heap::{ATYPE_BYTE, ATYPE_CHAR},
    types::{JvmError, Value},
};

use super::NativeContext;
use super::INIT;
use crate::names::{c, d, mh};

/// A Java `char` as the bytes a String holds for it. ASCII is itself,
/// except that C0 controls other than `\n`, `\t` and `\r` become a space
/// (the log and display sinks cannot render them; a newline must stay a
/// newline for `StringBuilder` line-joining). Everything from U+0080 up is
/// its UTF-8 encoding: the string table checks UTF-8 on every resolve, so
/// the lone low byte these sites used to write made the whole string read
/// back as empty (`"23" + '°' + "C"` had length 0). A lone surrogate has no
/// UTF-8 form and becomes `?`, as `String(byte[])` already does for a byte
/// it cannot place.
pub(crate) fn char_bytes(c: i32, out: &mut [u8; 3]) -> &[u8] {
    let cp = (c as u32) & 0xFFFF;
    let len = if cp < 0x80 {
        out[0] = match cp as u8 {
            b'\n' | b'\t' | b'\r' => cp as u8,
            0..=0x1f => b' ',
            b => b,
        };
        1
    } else if cp < 0x800 {
        out[0] = 0xC0 | ((cp >> 6) as u8);
        out[1] = 0x80 | ((cp & 0x3F) as u8);
        2
    } else if (0xD800..=0xDFFF).contains(&cp) {
        out[0] = b'?';
        1
    } else {
        out[0] = 0xE0 | ((cp >> 12) as u8);
        out[1] = 0x80 | (((cp >> 6) & 0x3F) as u8);
        out[2] = 0x80 | ((cp & 0x3F) as u8);
        3
    };
    &out[..len]
}

/// Make `bytes` valid UTF-8 in place: every byte of a malformed sequence
/// becomes `?`, valid sequences (ASCII included) stay as they are.
pub(crate) fn replace_invalid_utf8(bytes: &mut [u8]) {
    let mut at = 0;
    while at < bytes.len() {
        match core::str::from_utf8(&bytes[at..]) {
            Ok(_) => return,
            Err(e) => {
                let bad = at + e.valid_up_to();
                // `error_len` is `None` for a sequence cut short by the end.
                let bad_len = e.error_len().unwrap_or(bytes.len() - bad);
                for b in &mut bytes[bad..bad + bad_len] {
                    *b = b'?';
                }
                at = bad + bad_len;
            }
        }
    }
}

pub(crate) fn dispatch(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match method {
        // ── String — identity ────────────────────────────────────────
        // `String.toString()` returns `this`. Covers an invokevirtual that
        // resolves to the String class directly (the Object-typed receiver
        // path is handled by `dispatch_object`).
        mh::toString => match ctx.args.first() {
            Some(Value::Reference(idx)) => Some(Ok(Some(Value::Reference(*idx)))),
            _ => Some(Err(JvmError::InvalidReference)),
        },

        // ── String — byte[] constructors ─────────────────────────────
        // `new String(byte[])` / `new String(byte[], int, int)`. The receiver
        // on the operand stack is the placeholder ObjectHeap object from
        // `op_new` (String has no class file, so javac's `new; dup;
        // invokespecial` shape reaches native dispatch); this arm interns the
        // bytes and returns the string Reference, and `finalize_invoke`
        // swaps every stack/locals occurrence of the placeholder for it —
        // see the `<init>` special case there. The bytes are the string's
        // bytes (the table is UTF-8 and `charAt`/`length` count bytes):
        // a valid UTF-8 sequence is kept as it is, so `new String(utf8)`
        // round-trips, and each byte of an invalid one becomes `?` (the
        // table refuses what is not UTF-8).
        INIT if ctx.descriptor == "([B)V" || ctx.descriptor == "([BII)V" => {
            let Some(Value::ArrayRef(arr)) = ctx.args.get(1) else {
                return Some(Err(JvmError::InvalidReference));
            };
            let total = match ctx.arrays.length(*arr) {
                Some(l) => l as usize,
                None => return Some(Err(JvmError::InvalidReference)),
            };
            let (off, len) = if ctx.descriptor == "([BII)V" {
                match (ctx.args.get(2), ctx.args.get(3)) {
                    (Some(Value::Int(o)), Some(Value::Int(l))) if *o >= 0 && *l >= 0 => {
                        (*o as usize, *l as usize)
                    }
                    (Some(Value::Int(_)), Some(Value::Int(_))) => {
                        return Some(Err(JvmError::ArrayIndexOutOfBounds))
                    }
                    _ => return Some(Err(JvmError::InvalidReference)),
                }
            } else {
                (0, total)
            };
            if off > total || len > total - off {
                return Some(Err(JvmError::ArrayIndexOutOfBounds));
            }
            let mut bytes = alloc::vec::Vec::with_capacity(len);
            for i in off..off + len {
                let v = match ctx.arrays.load(*arr, i) {
                    Some(v) => v,
                    None => return Some(Err(JvmError::InvalidReference)),
                };
                bytes.push(v as u8);
            }
            replace_invalid_utf8(&mut bytes);
            let r = ctx
                .strings
                .intern_dyn_owned(bytes)
                .ok_or(JvmError::StackOverflow);
            Some(r.map(|idx| Some(Value::Reference(idx))))
        }

        // ── String — static formatter ────────────────────────────────
        mh::format => super::string_format::format(ctx),

        // ── String — non-allocating ──────────────────────────────────
        mh::length => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                Some(Ok(Some(Value::Int(s.len() as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::charAt => {
            if let (Some(Value::Reference(idx)), Some(Value::Int(i))) =
                (ctx.args.first(), ctx.args.get(1))
            {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                match usize::try_from(*i).ok().and_then(|i| s.as_bytes().get(i)) {
                    Some(&ch) => Some(Ok(Some(Value::Int(ch as i32)))),
                    None => Some(Err(super::throw_named(
                        ctx,
                        c::java_lang_StringIndexOutOfBoundsException,
                    ))),
                }
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::isEmpty => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                Some(Ok(Some(Value::Int(s.is_empty() as i32))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::equals => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int((sa == sb) as i32))))
            }
            // equals(Object): null or any non-String is simply false.
            (Some(Value::Reference(_)), Some(_)) => Some(Ok(Some(Value::Int(0)))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::equalsIgnoreCase => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.eq_ignore_ascii_case(sb) as i32))))
            }
            (Some(Value::Reference(_)), Some(Value::Null)) => Some(Ok(Some(Value::Int(0)))),
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::startsWith => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                // startsWith(prefix, toffset): false for an offset outside
                // [0, len] — Java's contract, no exception.
                let r = match ctx.args.get(2) {
                    Some(Value::Int(off)) => usize::try_from(*off)
                        .ok()
                        .and_then(|o| sa.get(o..))
                        .is_some_and(|tail| tail.starts_with(sb)),
                    _ => sa.starts_with(sb),
                };
                Some(Ok(Some(Value::Int(r as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::endsWith => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.ends_with(sb) as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::contains => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                Some(Ok(Some(Value::Int(sa.contains(sb) as i32))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::indexOf => {
            // Optional third arg: fromIndex (negative clamps to 0, past the
            // end finds nothing) — Java's contract for the 2-arg overloads.
            let from = match ctx.args.get(2) {
                Some(Value::Int(f)) => (*f).max(0) as usize,
                _ => 0,
            };
            match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    // indexOf(String[, int])
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    // Java clamps fromIndex to the length first, so an
                    // empty needle past the end is found at length().
                    let from = from.min(sa.len());
                    let result = sa
                        .get(from..)
                        .and_then(|tail| tail.find(sb))
                        .map(|i| (i + from) as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                    // indexOf(char[, int]) — char passed as int; a char
                    // from U+0080 up is its UTF-8 byte sequence in the string.
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let mut buf = [0u8; 3];
                    let target = char_bytes(*ch, &mut buf);
                    let result = sa
                        .as_bytes()
                        .get(from..)
                        .and_then(|tail| tail.windows(target.len()).position(|w| w == target))
                        .map(|i| (i + from) as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        mh::lastIndexOf => {
            // Optional third arg: fromIndex — search backward starting at
            // that index (a match may start at fromIndex itself); negative
            // finds nothing, past the end searches the whole string.
            let from = match ctx.args.get(2) {
                Some(Value::Int(f)) if *f < 0 => {
                    return Some(Ok(Some(Value::Int(-1))));
                }
                Some(Value::Int(f)) => Some(*f as usize),
                _ => None,
            };
            match (ctx.args.first(), ctx.args.get(1)) {
                (Some(Value::Reference(a)), Some(Value::Int(ch))) => {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let mut buf = [0u8; 3];
                    let target = char_bytes(*ch, &mut buf);
                    let bytes = sa.as_bytes();
                    // A match may start at `from`; the window ends after it.
                    let end = from
                        .map(|f| f.saturating_add(target.len()).min(bytes.len()))
                        .unwrap_or(bytes.len());
                    let result = bytes[..end]
                        .windows(target.len())
                        .rposition(|w| w == target)
                        .map(|i| i as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                    // lastIndexOf(String[, int])
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    let end = from
                        .map(|f| f.saturating_add(sb.len()).min(sa.len()))
                        .unwrap_or(sa.len());
                    let result = sa
                        .get(..end)
                        .and_then(|head| head.rfind(sb))
                        .map(|i| i as i32)
                        .unwrap_or(-1);
                    Some(Ok(Some(Value::Int(result))))
                }
                _ => Some(Err(JvmError::InvalidReference)),
            }
        }
        mh::compareTo => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let sa = ctx.strings.resolve(*a).unwrap_or("");
                let sb = ctx.strings.resolve(*b).unwrap_or("");
                // Java's contract: the difference of the first differing
                // chars, else the length difference — `"a".compareTo("c")`
                // is -2, not -1 (QA 2026-09-13).
                let (ba, bb) = (sa.as_bytes(), sb.as_bytes());
                let result = match ba.iter().zip(bb).find(|(x, y)| x != y) {
                    Some((&x, &y)) => x as i32 - y as i32,
                    None => ba.len() as i32 - bb.len() as i32,
                };
                Some(Ok(Some(Value::Int(result))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },

        // ── String — allocating ──────────────────────────────────────
        mh::substring => match ctx.args.first() {
            Some(Value::Reference(idx)) => {
                // Copy bytes to owned storage first so the immutable borrow
                // on ctx.strings ends before we call intern_dyn (mutable).
                // A negative or out-of-range index is Java's
                // StringIndexOutOfBoundsException (as charAt throws), not
                // the array flavour: a catch of the former must match.
                let owned: Option<alloc::vec::Vec<u8>> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let bytes = s.as_bytes();
                    let start = match ctx.args.get(1) {
                        Some(Value::Int(n)) => usize::try_from(*n).ok(),
                        _ => return Some(Err(JvmError::InvalidReference)),
                    };
                    let end = match ctx.args.get(2) {
                        Some(Value::Int(n)) => usize::try_from(*n).ok(),
                        None => Some(bytes.len()),
                        _ => return Some(Err(JvmError::InvalidReference)),
                    };
                    match (start, end) {
                        (Some(start), Some(end)) if start <= end && end <= bytes.len() => {
                            Some(bytes[start..end].to_vec())
                        }
                        _ => None,
                    }
                };
                match owned {
                    None => Some(Err(super::throw_named(
                        ctx,
                        c::java_lang_StringIndexOutOfBoundsException,
                    ))),
                    Some(v) => {
                        let r = ctx
                            .strings
                            .intern_dyn_owned(v)
                            .ok_or(JvmError::StackOverflow);
                        Some(r.map(|idx| Some(Value::Reference(idx))))
                    }
                }
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::trim => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let owned: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    // Java trims every code unit <= U+0020 (NUL, VT, the
                    // C0 controls), not only the five ASCII whitespace chars.
                    s.trim_matches(|c: char| c <= '\u{20}').as_bytes().to_vec()
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(owned)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::toUpperCase => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let upper: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.bytes().map(|b| b.to_ascii_uppercase()).collect()
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(upper)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::toLowerCase => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let lower: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.bytes().map(|b| b.to_ascii_lowercase()).collect()
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(lower)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::valueOf => {
            // Static method: String.valueOf(int/long/boolean/char/float/double),
            // plus valueOf(Object) for a String or null — any other object was
            // already turned into its `toString()` by the interpreter
            // (`Executor::stringify_object_arg`).
            let result: Option<alloc::vec::Vec<u8>> = match ctx.args.first() {
                Some(Value::Reference(r)) => return Some(Ok(Some(Value::Reference(*r)))),
                Some(Value::Null) => Some(b"null".to_vec()),
                Some(Value::Int(n)) => {
                    if ctx.descriptor.starts_with("(Z)") {
                        Some(if *n != 0 {
                            b"true".to_vec()
                        } else {
                            b"false".to_vec()
                        })
                    } else if ctx.descriptor.starts_with("(C)") {
                        let mut buf = [0u8; 3];
                        Some(char_bytes(*n, &mut buf).to_vec())
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
                    let bytes = crate::object_heap::double_to_str_buf(*d, &mut tmp);
                    Some(bytes.to_vec())
                }
                _ => None,
            };
            if let Some(bytes) = result {
                let r = ctx
                    .strings
                    .intern_dyn_owned(bytes)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::concat => match (ctx.args.first(), ctx.args.get(1)) {
            (Some(Value::Reference(a)), Some(Value::Reference(b))) => {
                let combined: alloc::vec::Vec<u8> = {
                    let sa = ctx.strings.resolve(*a).unwrap_or("");
                    let sb = ctx.strings.resolve(*b).unwrap_or("");
                    let mut v = alloc::vec::Vec::with_capacity(sa.len() + sb.len());
                    v.extend_from_slice(sa.as_bytes());
                    v.extend_from_slice(sb.as_bytes());
                    v
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(combined)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            }
            _ => Some(Err(JvmError::InvalidReference)),
        },
        mh::hashCode => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                // A literal's hash is in its pool row.
                if let Some(h) = ctx.strings.literal_hash(*idx) {
                    return Some(Ok(Some(Value::Int(h))));
                }
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                let mut h: i32 = 0;
                for &b in s.as_bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as i32);
                }
                Some(Ok(Some(Value::Int(h))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::toCharArray => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let bytes: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.as_bytes().to_vec()
                };
                let arr = match ctx.arrays.alloc(ATYPE_CHAR, bytes.len() as u16) {
                    Some(a) => a,
                    None => return Some(Err(JvmError::StackOverflow)),
                };
                for (i, &b) in bytes.iter().enumerate() {
                    ctx.arrays.store(arr, i, b as i32);
                }
                Some(Ok(Some(Value::ArrayRef(arr))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        // Elements are sign-extended (`as i8 as i32`) to match baload/bastore
        // byte[] slot conventions.
        mh::getBytes => {
            if let Some(Value::Reference(idx)) = ctx.args.first() {
                let bytes: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    s.as_bytes().to_vec()
                };
                let arr = match ctx.arrays.alloc(ATYPE_BYTE, bytes.len() as u16) {
                    Some(a) => a,
                    None => return Some(Err(JvmError::StackOverflow)),
                };
                for (i, &b) in bytes.iter().enumerate() {
                    ctx.arrays.store(arr, i, b as i8 as i32);
                }
                Some(Ok(Some(Value::ArrayRef(arr))))
            } else {
                Some(Err(JvmError::InvalidReference))
            }
        }
        mh::replace => {
            // Two overloads: replace(char, char) and replace(CharSequence, CharSequence)
            if ctx.descriptor.starts_with("(CC)") {
                // replace(char oldChar, char newChar)
                let (Some(Value::Reference(idx)), Some(Value::Int(old)), Some(Value::Int(new))) =
                    (ctx.args.first(), ctx.args.get(1), ctx.args.get(2))
                else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let replaced: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let (mut ob, mut nb) = ([0u8; 3], [0u8; 3]);
                    let old_bytes = char_bytes(*old, &mut ob);
                    let new_bytes = char_bytes(*new, &mut nb);
                    let bytes = s.as_bytes();
                    let mut out = alloc::vec::Vec::with_capacity(bytes.len());
                    let mut i = 0;
                    while i < bytes.len() {
                        if bytes[i..].starts_with(old_bytes) {
                            out.extend_from_slice(new_bytes);
                            i += old_bytes.len();
                        } else {
                            out.push(bytes[i]);
                            i += 1;
                        }
                    }
                    out
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(replaced)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            } else {
                // replace(CharSequence target, CharSequence replacement) — we
                // accept (String, String) descriptors.
                let (
                    Some(Value::Reference(idx)),
                    Some(Value::Reference(target)),
                    Some(Value::Reference(repl)),
                ) = (ctx.args.first(), ctx.args.get(1), ctx.args.get(2))
                else {
                    return Some(Err(JvmError::InvalidReference));
                };
                let replaced: alloc::vec::Vec<u8> = {
                    let s = ctx.strings.resolve(*idx).unwrap_or("");
                    let t = ctx.strings.resolve(*target).unwrap_or("");
                    let r = ctx.strings.resolve(*repl).unwrap_or("");
                    if t.is_empty() {
                        // An empty target matches before every char and at
                        // the end: `"ab".replace("", "-")` is `-a-b-`, as in
                        // Java (QA 2026-09-13: it was a no-op).
                        let mut v =
                            alloc::vec::Vec::with_capacity(s.len() + r.len() * (s.len() + 1));
                        for b in s.bytes() {
                            v.extend_from_slice(r.as_bytes());
                            v.push(b);
                        }
                        v.extend_from_slice(r.as_bytes());
                        v
                    } else {
                        s.replace(t, r).into_bytes()
                    }
                };
                let r = ctx
                    .strings
                    .intern_dyn_owned(replaced)
                    .ok_or(JvmError::StackOverflow);
                Some(r.map(|v| Some(Value::Reference(v))))
            }
        }
        mh::split => {
            // split(String delim[, int limit]) — literal delimiter, no regex.
            // Java's limit: positive caps the number of parts (the last one
            // keeps the rest), negative keeps every part, zero (the one-arg
            // overload) drops trailing empty strings.
            let (Some(Value::Reference(idx)), Some(Value::Reference(delim_idx))) =
                (ctx.args.first(), ctx.args.get(1))
            else {
                return Some(Err(JvmError::InvalidReference));
            };
            let limit = match ctx.args.get(2) {
                Some(Value::Int(l)) => *l,
                _ => 0,
            };
            // Collect segments as owned byte vectors to avoid borrow conflicts.
            let segments: alloc::vec::Vec<alloc::vec::Vec<u8>> = {
                let s = ctx.strings.resolve(*idx).unwrap_or("");
                let delim = ctx.strings.resolve(*delim_idx).unwrap_or("");
                if delim.is_empty() {
                    alloc::vec![s.as_bytes().to_vec()]
                } else {
                    // splitn with no cap is split: one collect for both forms
                    // (the native arms' allocation sites are counted).
                    let cap = if limit > 0 {
                        limit as usize
                    } else {
                        usize::MAX
                    };
                    let mut parts: alloc::vec::Vec<alloc::vec::Vec<u8>> = s
                        .splitn(cap, delim)
                        .map(|part| part.as_bytes().to_vec())
                        .collect();
                    // Java limit-0 semantics: trailing empty strings are
                    // discarded ("a,b,," -> ["a","b"], ",," -> []). A
                    // no-match input stays [input], even when empty.
                    if limit == 0 && parts.len() > 1 {
                        while parts.last().is_some_and(|p| p.is_empty()) {
                            parts.pop();
                        }
                    }
                    parts
                }
            };
            // Intern each segment, then build a ref array.
            let mut segment_refs: alloc::vec::Vec<u16> =
                alloc::vec::Vec::with_capacity(segments.len());
            for seg in &segments {
                let r = match ctx.strings.intern_dyn(seg) {
                    Some(v) => v,
                    None => return Some(Err(JvmError::StackOverflow)),
                };
                segment_refs.push(r);
            }
            let arr = match ctx
                .arrays
                .alloc(crate::array_heap::ATYPE_REF, segments.len() as u16)
            {
                Some(a) => a,
                None => return Some(Err(JvmError::StackOverflow)),
            };
            for (i, &r) in segment_refs.iter().enumerate() {
                // Tag as Reference (not ObjectRef) so aaload returns Value::Reference.
                let tagged = ((r as u32) | crate::array_heap::REF_TAG) as i32;
                ctx.arrays.store(arr, i, tagged);
            }
            Some(Ok(Some(Value::ArrayRef(arr))))
        }
        // ── String.join ──────────────────────────────────────────────
        // `join(CharSequence, CharSequence...)` over a ref array, and
        // `join(CharSequence, Iterable)` over a builtin ArrayList (the only
        // classfile-less Iterable whose elements are reachable from here).
        // Elements are strings or null ("null", as Java prints it); any
        // other CharSequence is rejected rather than silently skipped.
        mh::join
            if ctx.descriptor == d::CharSequence_aCharSequence__String
                || ctx.descriptor == d::CharSequence_Iterable__String =>
        {
            let Some(Value::Reference(delim_idx)) = ctx.args.first() else {
                return Some(Err(JvmError::InvalidReference));
            };
            let elements: alloc::vec::Vec<Value> = match ctx.args.get(1) {
                Some(Value::ArrayRef(arr)) => {
                    let n = ctx.arrays.length(*arr).unwrap_or(0) as usize;
                    (0..n)
                        .map(|i| {
                            crate::array_heap::decode_ref(ctx.arrays.load(*arr, i).unwrap_or(0))
                        })
                        .collect()
                }
                Some(Value::ObjectRef(obj))
                    if ctx.objects.class_name(*obj) == Some(c::java_util_ArrayList) =>
                {
                    let Some(Value::Int(buf)) = ctx.objects.get_field(*obj, 0) else {
                        return Some(Err(JvmError::InvalidReference));
                    };
                    let buf = buf as u16;
                    (0..ctx.objects.list_len(buf))
                        .map(|i| ctx.objects.list_get(buf, i).unwrap_or(Value::Null))
                        .collect()
                }
                _ => return Some(Err(JvmError::InvalidReference)),
            };
            let mut out: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
            let delim = ctx
                .strings
                .resolve(*delim_idx)
                .unwrap_or("")
                .as_bytes()
                .to_vec();
            for (i, e) in elements.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(&delim);
                }
                match e {
                    Value::Reference(r) => {
                        out.extend_from_slice(ctx.strings.resolve(*r).unwrap_or("").as_bytes())
                    }
                    Value::Null => out.extend_from_slice(b"null"),
                    _ => return Some(Err(JvmError::InvalidReference)),
                }
            }
            match ctx.strings.intern_dyn(&out) {
                Some(r) => Some(Ok(Some(Value::Reference(r)))),
                None => Some(Err(JvmError::StackOverflow)),
            }
        }
        _ => None,
    }
}
