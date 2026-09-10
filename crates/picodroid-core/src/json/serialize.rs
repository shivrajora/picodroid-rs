// SPDX-License-Identifier: GPL-3.0-only
//! `toString()` / `toString(indent)` and `quote()` over the node pool,
//! spelled the way Android's `JSONStringer` spells them: `"`, `\` and `/`
//! escaped, control characters as `\uXXXX`, an indented document with one
//! entry per line and `"key": value`. Doubles print as integers when they
//! are integral (`numberToString`), otherwise in Rust's shortest form — a
//! very large or very small magnitude takes `1e21` where Java writes
//! `1.0E21`.

use alloc::{format, string::String, vec::Vec};

use super::{pool::Pool, Node, NodeIdx, MAX_DEPTH};

/// Serialize the tree at `root`; `None` when it is nested past
/// [`MAX_DEPTH`] or `root` is dead.
pub fn to_bytes(pool: &Pool, root: NodeIdx, indent: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    write(pool, root, indent, 0, &mut out).ok()?;
    Some(out)
}

fn write(
    pool: &Pool,
    node: NodeIdx,
    indent: usize,
    depth: usize,
    out: &mut Vec<u8>,
) -> Result<(), ()> {
    if depth >= MAX_DEPTH {
        return Err(());
    }
    match pool.get(node).ok_or(())? {
        Node::Null => out.extend_from_slice(b"null"),
        Node::Bool(true) => out.extend_from_slice(b"true"),
        Node::Bool(false) => out.extend_from_slice(b"false"),
        Node::Int(v) => out.extend_from_slice(format!("{v}").as_bytes()),
        Node::Long(v) => out.extend_from_slice(format!("{v}").as_bytes()),
        Node::Double(d) => out.extend_from_slice(double_to_string(*d).as_bytes()),
        Node::Str(s) => quote_into(s, out),
        Node::Object(entries) => {
            out.push(b'{');
            for (i, (key, child)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                newline(out, indent, depth + 1);
                quote_into(key, out);
                out.push(b':');
                if indent > 0 {
                    out.push(b' ');
                }
                write(pool, *child, indent, depth + 1, out)?;
            }
            if !entries.is_empty() {
                newline(out, indent, depth);
            }
            out.push(b'}');
        }
        Node::Array(items) => {
            out.push(b'[');
            for (i, child) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                newline(out, indent, depth + 1);
                write(pool, *child, indent, depth + 1, out)?;
            }
            if !items.is_empty() {
                newline(out, indent, depth);
            }
            out.push(b']');
        }
    }
    Ok(())
}

fn newline(out: &mut Vec<u8>, indent: usize, depth: usize) {
    if indent > 0 {
        out.push(b'\n');
        for _ in 0..indent * depth {
            out.push(b' ');
        }
    }
}

/// Android's `JSONObject.numberToString` for a double: integral values
/// print without a fraction.
pub fn double_to_string(d: f64) -> String {
    // `no_std`: no `fract`/`abs` on f64, so spell both out.
    let magnitude = if d < 0.0 { -d } else { d };
    if magnitude < 9.0e18 && (d as i64) as f64 == d {
        format!("{}", d as i64)
    } else if !(1e-4..1e16).contains(&magnitude) {
        format!("{d:e}")
    } else {
        format!("{d}")
    }
}

/// Append `s` as a quoted JSON string, escaped as `JSONStringer` does.
pub fn quote_into(s: &[u8], out: &mut Vec<u8>) {
    out.push(b'"');
    for &c in s {
        match c {
            b'"' | b'\\' | b'/' => {
                out.push(b'\\');
                out.push(c);
            }
            b'\t' => out.extend_from_slice(b"\\t"),
            0x08 => out.extend_from_slice(b"\\b"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            0x0c => out.extend_from_slice(b"\\f"),
            c if c < 0x20 => out.extend_from_slice(format!("\\u{c:04x}").as_bytes()),
            _ => out.push(c),
        }
    }
    out.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse::parse;

    fn round_trip(text: &str) -> String {
        let mut pool = Pool::new();
        let root = parse(&mut pool, text.as_bytes(), None).unwrap();
        String::from_utf8(to_bytes(&pool, root, 0).unwrap()).unwrap()
    }

    #[test]
    fn compact_output_round_trips() {
        let text = r#"{"a":1,"b":[true,false,null,"x\"y\\z\/w\n",-2.5,10000000000],"c":{},"d":[],"e":"°"}"#;
        assert_eq!(round_trip(text), text);
        assert_eq!(
            round_trip("[1.0, 17.3, 1e3, 2.5e-7, 1e22]"),
            "[1,17.3,1000,2.5e-7,1e22]"
        );
    }

    #[test]
    fn indented_output_matches_android_layout() {
        let mut pool = Pool::new();
        let root = parse(&mut pool, br#"{"a":1,"b":[1,{"c":"d"}],"e":{}}"#, None).unwrap();
        let s = String::from_utf8(to_bytes(&pool, root, 2).unwrap()).unwrap();
        assert_eq!(
            s,
            "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    {\n      \"c\": \"d\"\n    }\n  ],\n  \"e\": {}\n}"
        );
    }

    #[test]
    fn quote_escapes_like_jsonstringer() {
        let mut out = Vec::new();
        quote_into(b"a\"b\\c/d\te\x08f\ng\rh\x0ci\x01j", &mut out);
        assert_eq!(out, br#""a\"b\\c\/d\te\bf\ng\rh\fi\u0001j""#);
    }

    #[test]
    fn depth_cap_yields_none() {
        let mut pool = Pool::new();
        let ok: Vec<u8> = core::iter::repeat_n(b'[', MAX_DEPTH)
            .chain(core::iter::repeat_n(b']', MAX_DEPTH))
            .collect();
        let root = parse(&mut pool, &ok, None).unwrap();
        assert!(to_bytes(&pool, root, 0).is_some());
        // Wrap the deepest legal document once more by hand.
        let outer = pool.alloc(Node::Array(alloc::vec![root])).unwrap();
        assert!(to_bytes(&pool, outer, 0).is_none());
        assert!(to_bytes(&pool, 999, 0).is_none());
    }
}
