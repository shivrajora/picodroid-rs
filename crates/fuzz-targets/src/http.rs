// SPDX-License-Identifier: GPL-3.0-only
//! The HTTP/1.1 response head and the chunked body, as a server — or
//! anything between the device and it — sends them.

use http_head::{
    find_header_end, header_lines_of, header_name, header_value, is_chunked, parse_head_bytes,
    reason_phrase, status_line_of, ChunkDecoder, ChunkEvent,
};

use crate::Bytes;

/// Input: a few split points, then the bytes off the socket.
///
/// The connection finds the end of the head as bytes arrive, resuming each
/// search three bytes before the old end so a `\r\n\r\n` split across two
/// reads is still seen. Fed in any pieces, it must find the end exactly
/// where one search over the whole buffer does.
pub fn http_head(data: &[u8]) {
    let mut b = Bytes::new(data);
    let splits = [b.u8() as usize, b.u8() as usize, b.u8() as usize];
    let buf = b.rest();

    let whole = find_header_end(buf, 0);
    let mut seen = None;
    let mut end = 0usize;
    for step in splits.iter().chain(core::iter::repeat(&64)) {
        if seen.is_some() || end >= buf.len() {
            break;
        }
        let start = end.saturating_sub(3);
        end = (end + step.max(&1)).min(buf.len());
        seen = find_header_end(&buf[..end], start);
    }
    assert_eq!(seen, whole, "incremental and one-shot searches disagree");

    let head = &buf[..whole.unwrap_or(buf.len())];
    if let Ok((status, content_length)) = parse_head_bytes(head) {
        assert!((0..=999).contains(&status), "status {status}");
        assert!(content_length >= -1);
    }
    let _ = (is_chunked(head), reason_phrase(status_line_of(head)));
    for line in header_lines_of(head) {
        let _ = (header_name(line), header_value(line));
    }
}

/// Input: a selector byte; then either raw wire bytes or a body to frame.
///
/// Raw: the decoder never yields more body than it consumed, and says
/// `Bad` or `Done` at most once, after which it yields nothing.
/// Framed: a body cut into input-chosen chunks by a reference encoder
/// decodes back to itself, ending in `Done`.
pub fn http_chunked(data: &[u8]) {
    let mut b = Bytes::new(data);
    if b.u8() & 1 == 0 {
        let wire = b.rest();
        let mut d = ChunkDecoder::new();
        let mut body = 0usize;
        let mut ended = false;
        for (i, &byte) in wire.iter().enumerate() {
            match d.push(byte) {
                ChunkEvent::Byte(_) => {
                    assert!(!ended, "body after the end");
                    body += 1;
                    assert!(body <= i + 1);
                }
                ChunkEvent::Done | ChunkEvent::Bad => ended = true,
                ChunkEvent::None => {}
            }
        }
        return;
    }
    let sizes: Vec<usize> = (0..b.below(8)).map(|_| b.below(300)).collect();
    let body = b.rest();
    let mut wire = Vec::new();
    let mut at = 0;
    for size in sizes.iter().copied().chain(core::iter::once(usize::MAX)) {
        let n = size.min(body.len() - at);
        if n == 0 {
            if at < body.len() {
                continue;
            }
            break;
        }
        wire.extend_from_slice(format!("{n:x}\r\n").as_bytes());
        wire.extend_from_slice(&body[at..at + n]);
        wire.extend_from_slice(b"\r\n");
        at += n;
    }
    wire.extend_from_slice(b"0\r\n\r\n");

    let mut d = ChunkDecoder::new();
    let mut out = Vec::new();
    for &byte in &wire {
        match d.push(byte) {
            ChunkEvent::Byte(x) => out.push(x),
            ChunkEvent::None => {}
            ChunkEvent::Done => break,
            ChunkEvent::Bad => panic!("valid chunked framing refused: {wire:?}"),
        }
    }
    assert!(d.done(), "the terminal chunk did not end the body");
    assert_eq!(out, body);
}
