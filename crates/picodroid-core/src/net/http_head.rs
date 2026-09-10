// SPDX-License-Identifier: GPL-3.0-only
//! Pure byte-level helpers for the HTTP/1.1 request and response head.
//!
//! Split out of [`super::http_connection`] so the parsing rules carry host
//! unit tests: `net` is `cfg(not(test))` (it reaches the network HAL), so
//! anything tested from inside it compiles but never runs. This module has no
//! HAL, JVM, or `super::` dependencies and is re-exposed as a test shim in
//! `lib.rs` — see the shim comment there before adding anything non-pure.

/// Offset of the first byte *after* the `\r\n\r\n` that ends a message head,
/// searching from `from`.
pub fn find_header_end(buf: &[u8], from: usize) -> Option<usize> {
    let needle = b"\r\n\r\n";
    if buf.len() < 4 {
        return None;
    }
    let mut i = from;
    while i + 4 <= buf.len() {
        if &buf[i..i + 4] == needle {
            return Some(i + 4);
        }
        i += 1;
    }
    None
}

/// Drop a trailing `\r`, so a `\n`-split line compares as its logical content.
pub fn strip_cr(line: &[u8]) -> &[u8] {
    if let Some((&b'\r', rest)) = line.split_last() {
        rest
    } else {
        line
    }
}

/// The status line of a response head (`HTTP/1.1 200 OK`), `\r` stripped.
pub fn status_line_of(head: &[u8]) -> &[u8] {
    strip_cr(head.split(|&b| b == b'\n').next().unwrap_or(&[]))
}

/// The header lines of a response head, in wire order — status line dropped,
/// `\r` stripped, blank lines removed.
pub fn header_lines_of(head: &[u8]) -> impl Iterator<Item = &[u8]> {
    head.split(|&b| b == b'\n')
        .skip(1)
        .map(strip_cr)
        .filter(|l| !l.is_empty())
}

/// The reason phrase of a status line (`HTTP/1.1 404 Not Found` → `Not
/// Found`), or `None` when the line has no third field. Internal spaces are
/// preserved.
pub fn reason_phrase(status_line: &[u8]) -> Option<&[u8]> {
    let mut parts = status_line.splitn(3, |&b| b == b' ');
    parts.next()?;
    parts.next()?;
    parts.next()
}

/// Case-insensitive match of a `name:` prefix. `name` must already be
/// lowercase. The colon is required, so `X-Multi-Extra` does not match
/// `x-multi`.
pub fn header_matches(line: &[u8], name: &[u8]) -> bool {
    if line.len() < name.len() + 1 {
        return false;
    }
    for i in 0..name.len() {
        if line[i].to_ascii_lowercase() != name[i] {
            return false;
        }
    }
    line[name.len()] == b':'
}

/// The value of a `Name: value` line, with leading spaces/tabs trimmed.
pub fn header_value(line: &[u8]) -> Option<&[u8]> {
    let colon = line.iter().position(|&b| b == b':')?;
    let mut v = &line[colon + 1..];
    while let Some((&first, rest)) = v.split_first() {
        if first == b' ' || first == b'\t' {
            v = rest;
        } else {
            break;
        }
    }
    Some(v)
}

/// The name of a `Name: value` line (everything before the colon).
pub fn header_name(line: &[u8]) -> &[u8] {
    let colon = line.iter().position(|&b| b == b':').unwrap_or(line.len());
    &line[..colon]
}

/// Parse an all-digit ASCII decimal. Returns -1 for empty or non-digit input.
pub fn parse_decimal(s: &[u8]) -> i64 {
    if s.is_empty() {
        return -1;
    }
    let mut acc: i64 = 0;
    for &b in s {
        if !b.is_ascii_digit() {
            return -1;
        }
        acc = acc.saturating_mul(10) + (b - b'0') as i64;
    }
    acc
}

/// The response's status line was not `HTTP/x.y CODE …`. The caller turns
/// this into a `ProtocolException` naming the offending line.
#[derive(Debug, PartialEq, Eq)]
pub struct MalformedStatusLine;

fn parse_status_code(line: &[u8]) -> Result<i32, MalformedStatusLine> {
    // "HTTP/1.1 200 OK" — the code sits between the first and second space.
    let first_sp = line
        .iter()
        .position(|&b| b == b' ')
        .ok_or(MalformedStatusLine)?;
    let rest = &line[first_sp + 1..];
    let second_sp = rest.iter().position(|&b| b == b' ').unwrap_or(rest.len());
    let code = parse_decimal(&rest[..second_sp]);
    if code < 0 {
        return Err(MalformedStatusLine);
    }
    Ok(code as i32)
}

/// Parse a response head into `(status_code, content_length)`;
/// `content_length` is -1 when the header is absent.
pub fn parse_head_bytes(head: &[u8]) -> Result<(i32, i64), MalformedStatusLine> {
    let status_code = parse_status_code(status_line_of(head))?;
    let mut content_length: i64 = -1;
    for line in header_lines_of(head) {
        if header_matches(line, b"content-length") {
            if let Some(value) = header_value(line) {
                let v = parse_decimal(value);
                if v >= 0 {
                    content_length = v;
                }
            }
        }
    }
    Ok((status_code, content_length))
}

/// True when the head carries `Transfer-Encoding: chunked` (last token —
/// per RFC 9112 chunked must be the final encoding).
pub fn is_chunked(head: &[u8]) -> bool {
    for line in header_lines_of(head) {
        if header_matches(line, b"transfer-encoding") {
            if let Some(value) = header_value(line) {
                // The value may be a list ("gzip, chunked"); match the last
                // comma-separated token, case-insensitively.
                let last = value.rsplit(|&b| b == b',').next().unwrap_or(value);
                let last: alloc::vec::Vec<u8> = last
                    .iter()
                    .filter(|b| !b.is_ascii_whitespace())
                    .map(|b| b.to_ascii_lowercase())
                    .collect();
                return last == b"chunked";
            }
        }
    }
    false
}

/// Incremental `Transfer-Encoding: chunked` decoder (RFC 9112 §7.1). Fed one
/// wire byte at a time — the transport hands the connection arbitrary
/// fragments, so every boundary (a size line, a CRLF, a trailer) can be
/// split anywhere; per-byte state is the simplest resumable form and the
/// stream is already copied byte-wise into the JVM array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkState {
    /// Reading the hex size line (possibly inside a `;ext` suffix).
    Size {
        val: u64,
        seen_digit: bool,
        in_ext: bool,
    },
    /// Size line seen `\r`, expecting `\n`.
    SizeLf {
        val: u64,
    },
    /// Inside chunk data with this many bytes left.
    Data {
        left: u64,
    },
    /// Chunk data done, expecting `\r` then `\n`.
    DataCr,
    DataLf,
    /// After the last (size 0) chunk: consuming trailer lines. `at_line_start`
    /// with an immediate CRLF ends the body.
    Trailer {
        at_line_start: bool,
        saw_cr: bool,
    },
    Done,
    Bad,
}

pub struct ChunkDecoder {
    state: ChunkState,
}

/// What one wire byte produced.
#[derive(Debug, PartialEq, Eq)]
pub enum ChunkEvent {
    /// A body payload byte.
    Byte(u8),
    /// Framing consumed; nothing for the caller.
    None,
    /// The terminal chunk and trailers are fully consumed: end of body.
    Done,
    /// Malformed framing.
    Bad,
}

impl Default for ChunkDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkDecoder {
    pub const fn new() -> Self {
        Self {
            state: ChunkState::Size {
                val: 0,
                seen_digit: false,
                in_ext: false,
            },
        }
    }

    pub fn done(&self) -> bool {
        self.state == ChunkState::Done
    }

    pub fn push(&mut self, b: u8) -> ChunkEvent {
        use ChunkState::*;
        match self.state {
            Size {
                mut val,
                mut seen_digit,
                mut in_ext,
            } => {
                match b {
                    b'\r' if seen_digit => {
                        self.state = SizeLf { val };
                        return ChunkEvent::None;
                    }
                    b';' => in_ext = true,
                    _ if in_ext => {}
                    b'0'..=b'9' => {
                        val = val.saturating_mul(16) + (b - b'0') as u64;
                        seen_digit = true;
                    }
                    b'a'..=b'f' => {
                        val = val.saturating_mul(16) + (b - b'a' + 10) as u64;
                        seen_digit = true;
                    }
                    b'A'..=b'F' => {
                        val = val.saturating_mul(16) + (b - b'A' + 10) as u64;
                        seen_digit = true;
                    }
                    _ => {
                        self.state = Bad;
                        return ChunkEvent::Bad;
                    }
                }
                self.state = Size {
                    val,
                    seen_digit,
                    in_ext,
                };
                ChunkEvent::None
            }
            SizeLf { val } => {
                if b != b'\n' {
                    self.state = Bad;
                    return ChunkEvent::Bad;
                }
                if val == 0 {
                    self.state = Trailer {
                        at_line_start: true,
                        saw_cr: false,
                    };
                } else {
                    self.state = Data { left: val };
                }
                ChunkEvent::None
            }
            Data { left } => {
                let left = left - 1;
                self.state = if left == 0 { DataCr } else { Data { left } };
                ChunkEvent::Byte(b)
            }
            DataCr => {
                if b != b'\r' {
                    self.state = Bad;
                    return ChunkEvent::Bad;
                }
                self.state = DataLf;
                ChunkEvent::None
            }
            DataLf => {
                if b != b'\n' {
                    self.state = Bad;
                    return ChunkEvent::Bad;
                }
                self.state = Size {
                    val: 0,
                    seen_digit: false,
                    in_ext: false,
                };
                ChunkEvent::None
            }
            Trailer {
                at_line_start,
                saw_cr,
            } => match (b, at_line_start, saw_cr) {
                (b'\r', true, false) => {
                    self.state = Trailer {
                        at_line_start: true,
                        saw_cr: true,
                    };
                    ChunkEvent::None
                }
                (b'\n', true, true) => {
                    self.state = Done;
                    ChunkEvent::Done
                }
                (b'\n', _, _) => {
                    // End of a trailer field line.
                    self.state = Trailer {
                        at_line_start: true,
                        saw_cr: false,
                    };
                    ChunkEvent::None
                }
                _ => {
                    self.state = Trailer {
                        at_line_start: false,
                        saw_cr: false,
                    };
                    ChunkEvent::None
                }
            },
            Done => ChunkEvent::Done,
            Bad => ChunkEvent::Bad,
        }
    }
}

/// Write `src` at `pos`, returning bytes written — 0 (nothing written) if it
/// would not fit.
pub fn write_bytes(buf: &mut [u8], pos: usize, src: &[u8]) -> usize {
    if pos + src.len() > buf.len() {
        return 0;
    }
    buf[pos..pos + src.len()].copy_from_slice(src);
    src.len()
}

/// Write `val` as ASCII decimal at `pos`, returning bytes written — 0 if it
/// would not fit.
pub fn write_usize(buf: &mut [u8], pos: usize, val: usize) -> usize {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    let mut v = val;
    if v == 0 {
        tmp[0] = b'0';
        n = 1;
    } else {
        while v > 0 {
            tmp[n] = b'0' + (v % 10) as u8;
            v /= 10;
            n += 1;
        }
        tmp[..n].reverse();
    }
    write_bytes(buf, pos, &tmp[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dechunk(wire: &[u8]) -> Result<alloc::vec::Vec<u8>, &'static str> {
        let mut d = ChunkDecoder::new();
        let mut out = alloc::vec::Vec::new();
        for &b in wire {
            match d.push(b) {
                ChunkEvent::Byte(x) => out.push(x),
                ChunkEvent::None => {}
                ChunkEvent::Done => return Ok(out),
                ChunkEvent::Bad => return Err("bad"),
            }
        }
        Err("truncated")
    }

    #[test]
    fn is_chunked_matches_case_insensitively_and_last_token() {
        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(is_chunked(head));
        let head = b"HTTP/1.1 200 OK\r\ntransfer-encoding:  Chunked \r\n\r\n";
        assert!(is_chunked(head));
        let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n";
        assert!(is_chunked(head));
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n";
        assert!(!is_chunked(head));
    }

    #[test]
    fn chunk_decoder_reassembles_a_body() {
        // Two chunks with a hex size, an extension, and trailers.
        let wire = b"4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\n\r\n";
        assert_eq!(dechunk(wire).unwrap(), b"Wikipedia");
        let wire = b"A\r\n0123456789\r\n0\r\nX-Trailer: v\r\n\r\n";
        assert_eq!(dechunk(wire).unwrap(), b"0123456789");
        // Empty body.
        assert_eq!(dechunk(b"0\r\n\r\n").unwrap(), b"");
    }

    #[test]
    fn chunk_decoder_rejects_garbage_sizes() {
        assert_eq!(dechunk(b"zz\r\nWiki\r\n0\r\n\r\n"), Err("bad"));
        assert_eq!(dechunk(b"4\r\nWikiXX\r\n0\r\n\r\n"), Err("bad"));
        assert_eq!(dechunk(b"4\r\nWiki\r\n0\r\n"), Err("truncated"));
    }

    const HEAD: &[u8] =
        b"HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nX-Multi: first\r\nX-Multi: second\r\n\r\n";

    fn value_of<'a>(head: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
        // `last()` mirrors the accessor: when a header repeats, Android
        // reports the last value.
        header_lines_of(head)
            .filter(|l| header_matches(l, name))
            .filter_map(header_value)
            .last()
    }

    #[test]
    fn parses_200_with_content_length() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 42\r\nConnection: close\r\n\r\n";
        assert_eq!(parse_head_bytes(head).unwrap(), (200, 42));
    }

    #[test]
    fn parses_404_without_content_length() {
        let head = b"HTTP/1.1 404 Not Found\r\n\r\n";
        assert_eq!(parse_head_bytes(head).unwrap(), (404, -1));
    }

    #[test]
    fn content_length_is_case_insensitive() {
        let head = b"HTTP/1.1 200 OK\r\ncontent-length: 7\r\n\r\n";
        assert_eq!(parse_head_bytes(head).unwrap().1, 7);
    }

    #[test]
    fn malformed_status_line_is_rejected() {
        assert!(parse_head_bytes(b"NOT-HTTP\r\n\r\n").is_err());
        assert!(parse_head_bytes(b"HTTP/1.1 abc OK\r\n\r\n").is_err());
    }

    #[test]
    fn find_header_end_locates_crlfcrlf() {
        let buf = b"GET /\r\nHost: x\r\n\r\nBODY";
        assert_eq!(find_header_end(buf, 0), Some(buf.len() - 4));
        assert_eq!(find_header_end(b"GET /\r\nHost: x\r\n", 0), None);
    }

    #[test]
    fn write_usize_formats_decimal() {
        let mut buf = [0u8; 8];
        let n = write_usize(&mut buf, 0, 1234);
        assert_eq!(&buf[..n], b"1234");
    }

    #[test]
    fn write_usize_zero() {
        let mut buf = [0u8; 4];
        let n = write_usize(&mut buf, 0, 0);
        assert_eq!(&buf[..n], b"0");
    }

    #[test]
    fn write_bytes_refuses_an_overflowing_write() {
        let mut buf = [0u8; 4];
        assert_eq!(write_bytes(&mut buf, 2, b"abcd"), 0);
        assert_eq!(buf, [0, 0, 0, 0]);
    }

    #[test]
    fn header_lines_skips_status_line_and_blanks() {
        let lines: alloc::vec::Vec<&[u8]> = header_lines_of(HEAD).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"Content-Type: text/html");
        assert_eq!(lines[2], b"X-Multi: second");
    }

    #[test]
    fn status_line_has_no_trailing_cr() {
        assert_eq!(status_line_of(HEAD), b"HTTP/1.1 404 Not Found");
    }

    #[test]
    fn reason_phrase_keeps_internal_spaces() {
        assert_eq!(reason_phrase(status_line_of(HEAD)), Some(&b"Not Found"[..]));
        assert_eq!(reason_phrase(b"HTTP/1.1 200"), None);
    }

    #[test]
    fn header_lookup_is_case_insensitive_and_trims_leading_space() {
        assert_eq!(value_of(HEAD, b"content-type"), Some(&b"text/html"[..]));
    }

    /// Android reports the last value when a header repeats.
    #[test]
    fn repeated_header_returns_last_value() {
        assert_eq!(value_of(HEAD, b"x-multi"), Some(&b"second"[..]));
    }

    #[test]
    fn absent_header_returns_none() {
        assert_eq!(value_of(HEAD, b"server"), None);
    }

    /// A prefix is not a match: `X-Multi-Extra` differs from `X-Multi`.
    #[test]
    fn header_matches_requires_the_colon() {
        assert!(header_matches(b"X-Multi: v", b"x-multi"));
        assert!(!header_matches(b"X-Multi-Extra: v", b"x-multi"));
        assert!(!header_matches(b"X-Mult: v", b"x-multi"));
    }

    #[test]
    fn header_name_stops_at_the_colon() {
        assert_eq!(header_name(b"Content-Type: text/html"), b"Content-Type");
    }

    #[test]
    fn indexed_access_walks_headers_in_wire_order() {
        let nth = |n: usize| header_lines_of(HEAD).nth(n);
        assert_eq!(nth(0), Some(&b"Content-Type: text/html"[..]));
        assert_eq!(nth(1), Some(&b"X-Multi: first"[..]));
        assert_eq!(nth(2), Some(&b"X-Multi: second"[..]));
        assert_eq!(nth(3), None);
    }

    /// A head with no headers still parses; there is simply nothing after the
    /// status line.
    #[test]
    fn head_with_no_headers_has_no_lines() {
        let head = b"HTTP/1.1 204 No Content\r\n\r\n";
        assert_eq!(header_lines_of(head).count(), 0);
        assert_eq!(status_line_of(head), b"HTTP/1.1 204 No Content");
    }

    #[test]
    fn header_value_handles_empty_and_tab_padded_values() {
        assert_eq!(header_value(b"X-Empty:"), Some(&b""[..]));
        assert_eq!(header_value(b"X-Tab:\tv"), Some(&b"v"[..]));
        assert_eq!(header_value(b"no-colon"), None);
    }

    #[test]
    fn parse_decimal_rejects_non_digits() {
        assert_eq!(parse_decimal(b"123"), 123);
        assert_eq!(parse_decimal(b""), -1);
        assert_eq!(parse_decimal(b"12a"), -1);
        assert_eq!(parse_decimal(b"-1"), -1);
    }
}
