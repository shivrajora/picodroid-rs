// SPDX-License-Identifier: GPL-3.0-only
//! Strict RFC 8259 parser into the node pool.
//!
//! Android's `JSONTokener` is lenient (single quotes, unquoted keys,
//! comments, hex literals); this one is not — a document that is not JSON
//! fails with an Android-style message (`Expected ':' after key at
//! character 12`). Numbers follow Android's typing: no fraction or exponent
//! → `Integer` when it fits, else `Long`, else `Double`. Escapes decode to
//! UTF-8 (surrogate pairs combined, a lone surrogate becomes U+FFFD) and
//! bytes ≥ 0x80 pass through, so a Java string round-trips untouched.
//!
//! A failed parse frees every node it allocated; a successful one hands
//! back a root that is not yet bound to any wrapper — the caller binds it
//! in the same native call, before any Java allocation can run a prune.

use alloc::{format, string::String, vec::Vec};

use super::{
    pool::{Pool, PoolError},
    Node, NodeIdx, MAX_DEPTH,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

/// Parse `text`. `want_array`: `Some(false)` requires an object root,
/// `Some(true)` an array root, `None` accepts any value.
pub fn parse(
    pool: &mut Pool,
    text: &[u8],
    want_array: Option<bool>,
) -> Result<NodeIdx, ParseError> {
    let mut p = Parser {
        s: text,
        pos: 0,
        pool,
        allocated: Vec::new(),
    };
    match p.document(want_array) {
        Ok(root) => Ok(root),
        Err(e) => {
            for &n in &p.allocated {
                p.pool.free_node(n);
            }
            Err(e)
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    pool: &'a mut Pool,
    allocated: Vec<NodeIdx>,
}

impl Parser<'_> {
    fn document(&mut self, want_array: Option<bool>) -> Result<NodeIdx, ParseError> {
        self.skip_ws();
        match (want_array, self.peek()) {
            (None, _) | (Some(false), Some(b'{')) | (Some(true), Some(b'[')) => {}
            (Some(false), _) => return self.err("A JSONObject text must begin with '{'"),
            (Some(true), _) => return self.err("A JSONArray text must begin with '['"),
        }
        let root = self.value(0)?;
        self.skip_ws();
        if self.pos != self.s.len() {
            return self.err("Unexpected trailing characters");
        }
        Ok(root)
    }

    fn err<T>(&self, msg: &str) -> Result<T, ParseError> {
        Err(ParseError(format!("{msg} at character {}", self.pos)))
    }

    fn alloc(&mut self, node: Node) -> Result<NodeIdx, ParseError> {
        match self.pool.alloc(node) {
            Ok(i) => {
                self.allocated.push(i);
                Ok(i)
            }
            Err(_) => self.err("JSON pool exhausted"),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<NodeIdx, ParseError> {
        if depth >= MAX_DEPTH {
            return self.err("Too deeply nested");
        }
        self.skip_ws();
        match self.peek() {
            None => self.err("Unexpected end of input"),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => {
                let s = self.string()?;
                self.alloc(Node::Str(s))
            }
            Some(b't') => self.literal(b"true", Node::Bool(true)),
            Some(b'f') => self.literal(b"false", Node::Bool(false)),
            Some(b'n') => self.literal(b"null", Node::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => self.err("Unexpected character"),
        }
    }

    fn literal(&mut self, word: &[u8], node: Node) -> Result<NodeIdx, ParseError> {
        if self.s[self.pos..].starts_with(word) {
            self.pos += word.len();
            self.alloc(node)
        } else {
            self.err("Unexpected literal")
        }
    }

    fn object(&mut self, depth: usize) -> Result<NodeIdx, ParseError> {
        self.pos += 1; // '{'
        let obj = self.alloc(Node::Object(Vec::new()))?;
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(obj);
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return self.err("Expected a quoted key");
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return self.err("Expected ':' after key");
            }
            self.pos += 1;
            let child = self.value(depth + 1)?;
            match self.pool.object_put(obj, &key, child) {
                Ok(()) => {}
                Err(PoolError::Exhausted) => return self.err("JSON pool exhausted"),
                Err(_) => return self.err("Invalid object"),
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(obj);
                }
                _ => return self.err("Expected ',' or '}'"),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<NodeIdx, ParseError> {
        self.pos += 1; // '['
        let arr = self.alloc(Node::Array(Vec::new()))?;
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(arr);
        }
        loop {
            let child = self.value(depth + 1)?;
            match self.pool.array_set(arr, None, child) {
                Ok(()) => {}
                Err(PoolError::Exhausted) => return self.err("JSON pool exhausted"),
                Err(_) => return self.err("Invalid array"),
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(arr);
                }
                _ => return self.err("Expected ',' or ']'"),
            }
        }
    }

    /// Consume a quoted string (the opening quote is at `pos`) into UTF-8.
    fn string(&mut self) -> Result<Vec<u8>, ParseError> {
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return self.err("Unterminated string");
            };
            self.pos += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return self.err("Unterminated escape sequence");
                    };
                    self.pos += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let cp = self.unicode_escape()?;
                            let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return self.err("Illegal escape"),
                    }
                }
                _ => out.push(c),
            }
        }
    }

    /// The code point of a `\u` escape whose `u` was just consumed: a
    /// surrogate pair is combined, a lone surrogate becomes U+FFFD.
    fn unicode_escape(&mut self) -> Result<u32, ParseError> {
        let hi = self.hex4()?;
        if (0xD800..0xDC00).contains(&hi) {
            if self.s[self.pos..].starts_with(b"\\u") {
                let save = self.pos;
                self.pos += 2;
                let lo = self.hex4()?;
                if (0xDC00..0xE000).contains(&lo) {
                    return Ok(0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00));
                }
                self.pos = save;
            }
            return Ok(0xFFFD);
        }
        if (0xDC00..0xE000).contains(&hi) {
            return Ok(0xFFFD);
        }
        Ok(hi)
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let mut v = 0u32;
        for _ in 0..4 {
            let Some(d) = self.peek().and_then(|c| (c as char).to_digit(16)) else {
                return self.err("Illegal \\u escape");
            };
            v = (v << 4) | d;
            self.pos += 1;
        }
        Ok(v)
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn number(&mut self) -> Result<NodeIdx, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.digits() == 0 {
            return self.err("Expected a digit");
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if self.digits() == 0 {
                return self.err("Expected a digit after '.'");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return self.err("Expected a digit in the exponent");
            }
        }
        // ASCII by construction.
        let text = core::str::from_utf8(&self.s[start..self.pos]).unwrap_or("");
        let node = if !is_float {
            match text.parse::<i64>() {
                Ok(v) if v >= i32::MIN as i64 && v <= i32::MAX as i64 => Node::Int(v as i32),
                Ok(v) => Node::Long(v),
                Err(_) => match text.parse::<f64>() {
                    Ok(d) if d.is_finite() => Node::Double(d),
                    _ => return self.err("Invalid number"),
                },
            }
        } else {
            match text.parse::<f64>() {
                Ok(d) if d.is_finite() => Node::Double(d),
                _ => return self.err("Invalid number"),
            }
        };
        self.alloc(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::K_NULL;

    const OPEN_METEO: &[u8] = "{\"latitude\":37.56252,\"longitude\":-122.307274,\
        \"generationtime_ms\":0.1697540283203125,\"utc_offset_seconds\":0,\"timezone\":\"GMT\",\
        \"timezone_abbreviation\":\"GMT\",\"elevation\":15.0,\"current_units\":{\"time\":\"iso8601\",\
        \"interval\":\"seconds\",\"temperature_2m\":\"°C\",\"weather_code\":\"wmo code\"},\
        \"current\":{\"time\":\"2026-09-04T06:00\",\"interval\":900,\"temperature_2m\":17.3,\
        \"weather_code\":3}}"
        .as_bytes();

    fn get<'a>(pool: &'a Pool, obj: NodeIdx, key: &str) -> &'a Node {
        pool.get(pool.object_get(obj, key.as_bytes()).expect(key))
            .unwrap()
    }

    #[test]
    fn parses_the_open_meteo_reply() {
        let mut pool = Pool::new();
        let root = parse(&mut pool, OPEN_METEO, Some(false)).unwrap();
        assert_eq!(get(&pool, root, "latitude"), &Node::Double(37.56252));
        assert_eq!(get(&pool, root, "utc_offset_seconds"), &Node::Int(0));
        assert_eq!(get(&pool, root, "elevation"), &Node::Double(15.0));
        let units = pool.object_get(root, b"current_units").unwrap();
        assert_eq!(
            get(&pool, units, "temperature_2m"),
            &Node::Str("°C".as_bytes().to_vec())
        );
        let current = pool.object_get(root, b"current").unwrap();
        assert_eq!(get(&pool, current, "temperature_2m"), &Node::Double(17.3));
        assert_eq!(get(&pool, current, "weather_code"), &Node::Int(3));
        assert_eq!(get(&pool, current, "interval"), &Node::Int(900));
        assert_eq!(pool.key_at(root, 0), Some(&b"latitude"[..]));
        assert_eq!(pool.length(root), 9);
    }

    #[test]
    fn number_typing_follows_android() {
        let mut pool = Pool::new();
        let arr = parse(
            &mut pool,
            b"[2147483647, 2147483648, -2147483649, 9223372036854775807, 9223372036854775808, 1.0, 1e3, -0, 5E-1]",
            Some(true),
        )
        .unwrap();
        let items: Vec<Node> = (0..9)
            .map(|i| pool.get(pool.array_get(arr, i).unwrap()).unwrap().clone())
            .collect();
        assert_eq!(items[0], Node::Int(i32::MAX));
        assert_eq!(items[1], Node::Long(2147483648));
        assert_eq!(items[2], Node::Long(-2147483649));
        assert_eq!(items[3], Node::Long(i64::MAX));
        assert_eq!(items[4], Node::Double(9223372036854775808.0));
        assert_eq!(items[5], Node::Double(1.0));
        assert_eq!(items[6], Node::Double(1000.0));
        assert_eq!(items[7], Node::Int(0));
        assert_eq!(items[8], Node::Double(0.5));
    }

    #[test]
    fn escapes_decode_to_utf8() {
        let mut pool = Pool::new();
        let arr = parse(
            &mut pool,
            r#"["A\n\t\"\\\/\b\f\r", "°", "😀", "\ud83d", "\udc00x", "café raw °"]"#.as_bytes(),
            None,
        )
        .unwrap();
        let s = |i: usize| match pool.get(pool.array_get(arr, i).unwrap()).unwrap() {
            Node::Str(s) => s.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(s(0), b"A\n\t\"\\/\x08\x0c\r".to_vec());
        assert_eq!(s(1), "°".as_bytes().to_vec());
        assert_eq!(s(2), "😀".as_bytes().to_vec());
        assert_eq!(s(3), "\u{FFFD}".as_bytes().to_vec());
        assert_eq!(s(4), "\u{FFFD}x".as_bytes().to_vec());
        assert_eq!(s(5), "café raw °".as_bytes().to_vec());
    }

    #[test]
    fn nesting_and_literals() {
        let mut pool = Pool::new();
        let root = parse(&mut pool, b" { \"a\" : [ 1 , [ 2 , [ 3 ] ] ] , \"n\" : null , \"t\":true,\"f\":false , \"e\":{}, \"ea\":[] } ", Some(false)).unwrap();
        let a = pool.object_get(root, b"a").unwrap();
        let inner = pool.array_get(a, 1).unwrap();
        let innermost = pool.array_get(inner, 1).unwrap();
        assert_eq!(
            pool.get(pool.array_get(innermost, 0).unwrap()),
            Some(&Node::Int(3))
        );
        assert_eq!(pool.kind(pool.object_get(root, b"n").unwrap()), K_NULL);
        assert_eq!(get(&pool, root, "t"), &Node::Bool(true));
        assert_eq!(get(&pool, root, "f"), &Node::Bool(false));
        assert_eq!(pool.length(pool.object_get(root, b"e").unwrap()), 0);
        assert_eq!(pool.length(pool.object_get(root, b"ea").unwrap()), 0);
    }

    #[test]
    fn duplicate_keys_replace_in_place() {
        let mut pool = Pool::new();
        let root = parse(&mut pool, br#"{"a":1,"b":2,"a":3}"#, Some(false)).unwrap();
        assert_eq!(pool.length(root), 2);
        assert_eq!(pool.key_at(root, 0), Some(&b"a"[..]));
        assert_eq!(get(&pool, root, "a"), &Node::Int(3));
    }

    #[test]
    fn errors_name_the_position_and_free_everything() {
        let mut pool = Pool::new();
        let cases: &[(&[u8], Option<bool>, &str)] = &[
            (b"{", Some(false), "Expected a quoted key at character 1"),
            (
                b"[1]",
                Some(false),
                "A JSONObject text must begin with '{' at character 0",
            ),
            (
                b"{}",
                Some(true),
                "A JSONArray text must begin with '[' at character 0",
            ),
            (
                br#"{"a":1,}"#,
                Some(false),
                "Expected a quoted key at character 7",
            ),
            (
                br#"{"a" 1}"#,
                Some(false),
                "Expected ':' after key at character 5",
            ),
            (b"[1 2]", Some(true), "Expected ',' or ']' at character 3"),
            (
                b"[1] x",
                Some(true),
                "Unexpected trailing characters at character 4",
            ),
            (b"[tru]", Some(true), "Unexpected literal at character 1"),
            (
                b"[1.]",
                Some(true),
                "Expected a digit after '.' at character 3",
            ),
            (b"[-]", Some(true), "Expected a digit at character 2"),
            (
                b"[1e]",
                Some(true),
                "Expected a digit in the exponent at character 3",
            ),
            (
                br#"["abc"#,
                Some(true),
                "Unterminated string at character 5",
            ),
            (br#"["\x"]"#, Some(true), "Illegal escape at character 4"),
            (
                br#"["\u12"]"#,
                Some(true),
                "Illegal \\u escape at character 6",
            ),
            (b"", None, "Unexpected end of input at character 0"),
            (b"'a'", None, "Unexpected character at character 0"),
        ];
        for (text, want, msg) in cases {
            let err = parse(&mut pool, text, *want).unwrap_err();
            assert_eq!(
                err.0,
                *msg,
                "input {:?}",
                core::str::from_utf8(text).unwrap()
            );
            assert_eq!(
                pool.node_count(),
                0,
                "leak after {:?}",
                core::str::from_utf8(text).unwrap()
            );
        }
    }

    #[test]
    fn depth_cap_and_pool_cap() {
        let mut pool = Pool::new();
        let deep: Vec<u8> = core::iter::repeat_n(b'[', MAX_DEPTH + 2).collect();
        let err = parse(&mut pool, &deep, None).unwrap_err();
        assert!(err.0.starts_with("Too deeply nested"), "{}", err.0);
        assert_eq!(pool.node_count(), 0);
        let ok: Vec<u8> = core::iter::repeat_n(b'[', MAX_DEPTH)
            .chain(core::iter::repeat_n(b']', MAX_DEPTH))
            .collect();
        parse(&mut pool, &ok, None).unwrap();
        let mut huge = Vec::from(&b"["[..]);
        for i in 0..3000 {
            if i > 0 {
                huge.push(b',');
            }
            huge.push(b'1');
        }
        huge.push(b']');
        let before = pool.node_count();
        let err = parse(&mut pool, &huge, None).unwrap_err();
        assert!(err.0.starts_with("JSON pool exhausted"), "{}", err.0);
        assert_eq!(pool.node_count(), before);
    }
}
