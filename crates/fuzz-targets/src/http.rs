// SPDX-License-Identifier: GPL-3.0-only
//! The HTTP/1.1 response head and the chunked body, as a server — or
//! anything between the device and it — sends them.

use http_head::conn::{RxBuffer, RxError, IO_CHUNK, RX_BUF_SIZE};
use http_head::{
    find_header_end, header_lines_of, header_matches, header_name, header_value, is_chunked,
    parse_decimal, parse_head_bytes, reason_phrase, status_line_of, ChunkDecoder, ChunkEvent,
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
    let parsed = parse_head_bytes(head);
    if let Ok((status, content_length)) = parsed {
        assert!((0..=999).contains(&status), "status {status}");
        assert!(content_length >= -1);
    }
    let _ = (is_chunked(head), reason_phrase(status_line_of(head)));
    // `getHeaderField(name)` matches by name, case-insensitively: a line
    // matches its own lowercased name iff it has a colon, and the last
    // `Content-Length` line is the length the status parse reported.
    let mut last_length: Option<i64> = None;
    for line in header_lines_of(head) {
        let name = header_name(line);
        let lower = name.to_ascii_lowercase();
        let has_colon = line.contains(&b':');
        assert_eq!(
            header_matches(line, &lower),
            has_colon,
            "header_matches on {:?}",
            String::from_utf8_lossy(line)
        );
        if has_colon {
            assert!(!header_matches(line, b"\x00never a header name"));
        }
        if header_matches(line, b"content-length") {
            last_length = header_value(line).map(parse_decimal);
        }
        let _ = header_value(line);
    }
    if let (Ok((_, content_length)), Some(last)) = (parsed, last_length) {
        if last >= 0 {
            assert_eq!(content_length, last, "Content-Length read two ways");
        }
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

/// What one whole-stream decode says a connection must deliver.
#[derive(Debug, PartialEq, Eq)]
enum HeadVerdict {
    Ok { off: usize, chunked: bool },
    Err(RxError<()>),
}

#[derive(Debug)]
struct Oracle {
    head: HeadVerdict,
    /// The body bytes the app gets, in order.
    body: Vec<u8>,
    /// The error the final read must return, if the body does not end
    /// cleanly.
    tail: Option<RxError<()>>,
}

fn oracle(wire: &[u8], n: usize, chunk: usize) -> Oracle {
    let Some(off) = find_header_end(wire, 0) else {
        let e = if wire.len() >= n {
            RxError::HeadTooLarge
        } else {
            RxError::UnexpectedEof
        };
        return Oracle {
            head: HeadVerdict::Err(e),
            body: Vec::new(),
            tail: None,
        };
    };
    if off > n {
        return Oracle {
            head: HeadVerdict::Err(RxError::HeadTooLarge),
            body: Vec::new(),
            tail: None,
        };
    }
    let head = &wire[..off];
    let Ok((_, content_length)) = parse_head_bytes(head) else {
        return Oracle {
            head: HeadVerdict::Err(RxError::BadStatusLine),
            body: Vec::new(),
            tail: None,
        };
    };
    let chunked = is_chunked(head);
    if chunked && off > n - chunk {
        return Oracle {
            head: HeadVerdict::Err(RxError::HeadTooLarge),
            body: Vec::new(),
            tail: None,
        };
    }
    let rest = &wire[off..];
    let (body, tail) = if chunked {
        let mut d = ChunkDecoder::new();
        let mut body = Vec::new();
        let mut tail = Some(RxError::EofInChunk);
        for &b in rest {
            match d.push(b) {
                ChunkEvent::Byte(x) => body.push(x),
                ChunkEvent::None => {}
                ChunkEvent::Done => {
                    tail = None;
                    break;
                }
                ChunkEvent::Bad => {
                    tail = Some(RxError::BadChunk);
                    break;
                }
            }
        }
        (body, tail)
    } else if content_length >= 0 {
        let take = (content_length as usize).min(rest.len());
        // A short Content-Length body ends cleanly today: EOF is EOF.
        (rest[..take].to_vec(), None)
    } else {
        (rest.to_vec(), None)
    };
    Oracle {
        head: HeadVerdict::Ok { off, chunked },
        body,
        tail,
    }
}

/// Drive one `RxBuffer` shape over `wire`, chopped by `socket_sizes` and
/// read by `app_sizes`, and compare with the oracle.
fn run_conn<const N: usize, const CHUNK: usize>(
    wire: &[u8],
    socket_sizes: &[u8],
    app_sizes: &[u8],
    fail_at: Option<usize>,
) {
    let want = oracle(wire, N, CHUNK);
    let mut rx = RxBuffer::<N, CHUNK>::new();
    let mut pos = 0usize;
    let mut reads = 0usize;
    let failed = core::cell::Cell::new(false);
    // The injected failure is armed for the main sequence only; the retries
    // afterwards see a healthy socket.
    let fail_at = core::cell::Cell::new(fail_at);
    let mut recv = |buf: &mut [u8]| -> Result<usize, ()> {
        reads += 1;
        if fail_at.get() == Some(reads) {
            failed.set(true);
            return Err(());
        }
        let step = socket_sizes
            .get((reads - 1) % socket_sizes.len().max(1))
            .map_or(64, |&s| usize::from(s) + 1);
        let n = step.min(buf.len()).min(wire.len() - pos);
        buf[..n].copy_from_slice(&wire[pos..pos + n]);
        pos += n;
        Ok(n)
    };
    let check_window = |rx: &RxBuffer<N, CHUNK>| {
        let (h, t, hl) = rx.window();
        assert!(h <= t && t <= N, "window {h}..{t} of {N}");
        assert!(hl <= t, "head_len {hl} past tail {t}");
        if rx.head_parsed() {
            assert!(h >= hl, "stash head {h} inside the head {hl}");
        }
    };

    let head = rx.read_head(&mut recv);
    check_window(&rx);
    if failed.get() {
        assert_eq!(head, Err(RxError::Transport(())));
        // Afterwards only the invariants hold; the stream is gone.
        let mut buf = [0u8; 8];
        let _ = rx.read_body(&mut buf, &mut recv);
        check_window(&rx);
        return;
    }
    match (&want.head, head) {
        (HeadVerdict::Err(e), Err(got)) => {
            assert_eq!(&got, e, "head verdict");
            fail_at.set(None);
            // Asked again: a stream that ended says so again. After a bad
            // status line, or a chunked head refused for leaving no window,
            // the buffer keeps reading (the Java side has thrown; a retry is
            // its choice), so it may now hit the end or the cap instead;
            // never `Ok`.
            let again = rx.read_head(&mut recv);
            let mut buf = [0u8; 8];
            let body = rx.read_body(&mut buf, &mut recv);
            if *e != RxError::UnexpectedEof {
                assert!(
                    matches!(
                        again,
                        Err(RxError::BadStatusLine
                            | RxError::UnexpectedEof
                            | RxError::HeadTooLarge)
                    ),
                    "{again:?}"
                );
                assert!(body.is_err(), "{body:?}");
            } else {
                assert_eq!(again, Err(*e));
                assert_eq!(body, Err(*e));
            }
            check_window(&rx);
            return;
        }
        (HeadVerdict::Ok { off, chunked }, Ok(())) => {
            assert_eq!(rx.head(), &wire[..*off]);
            assert_eq!(rx.is_chunked(), *chunked);
            let (status, cl) = parse_head_bytes(rx.head()).unwrap();
            assert_eq!(rx.status_code(), status);
            assert_eq!(rx.content_length(), if *chunked { -1 } else { cl });
            let (_, _, hl) = rx.window();
            assert_eq!(hl, *off);
        }
        (w, got) => panic!("oracle says {w:?}, the buffer said {got:?}"),
    }

    let mut delivered = Vec::new();
    let mut calls = 0usize;
    let mut final_err = None;
    loop {
        calls += 1;
        assert!(
            calls <= wire.len() + 4,
            "no progress: {calls} reads for {} wire bytes",
            wire.len()
        );
        let size = app_sizes
            .get((calls - 1) % app_sizes.len().max(1))
            .map_or(32, |&s| usize::from(s) + 1);
        let mut buf = vec![0u8; size];
        match rx.read_body(&mut buf, &mut recv) {
            Ok(0) => break,
            Ok(n) => {
                assert!(n <= size);
                delivered.extend_from_slice(&buf[..n]);
            }
            Err(e) => {
                final_err = Some(e);
                break;
            }
        }
        check_window(&rx);
        if failed.get() {
            break;
        }
    }
    check_window(&rx);
    if failed.get() {
        assert_eq!(final_err, Some(RxError::Transport(())));
        assert!(
            want.body.starts_with(&delivered),
            "delivered bytes are not a prefix of the body"
        );
        return;
    }
    assert_eq!(final_err, want.tail, "how the body ended");
    assert_eq!(
        delivered, want.body,
        "body differs (chunks {socket_sizes:?}, reads {app_sizes:?})"
    );
    // Past the end: the same answer again, nothing more delivered.
    fail_at.set(None);
    let mut buf = [0u8; 8];
    for _ in 0..2 {
        let again = rx.read_body(&mut buf, &mut recv);
        match want.tail {
            None => assert_eq!(again, Ok(0)),
            Some(e) => assert_eq!(again, Err(e)),
        }
    }
    assert_eq!(rx.read_head(&mut recv), Ok(()));
}

/// Input: a flags byte (bit 0: the device's buffer shape, else a small one
/// whose edges are a few bytes away; bit 1: the socket fails once, at the
/// read the next byte names), up to eight socket read sizes, up to eight
/// app read sizes, then the bytes off the socket.
///
/// However the stream is chopped and however the app reads, the connection
/// delivers exactly the body one whole-stream decode yields, says the same
/// about a head that does not fit or a body that breaks, and its window
/// indices never leave the buffer.
pub fn http_conn(data: &[u8]) {
    let mut b = Bytes::new(data);
    let flags = b.u8();
    let fail_at = (flags & 2 != 0).then(|| usize::from(b.u8()) + 1);
    let socket_sizes = b.take(8).to_vec();
    let app_sizes = b.take(8).to_vec();
    let wire = b.rest();
    if flags & 1 == 1 {
        run_conn::<RX_BUF_SIZE, IO_CHUNK>(wire, &socket_sizes, &app_sizes, fail_at);
    } else {
        run_conn::<96, 32>(wire, &socket_sizes, &app_sizes, fail_at);
    }
}
