// SPDX-License-Identifier: GPL-3.0-only
//! The receive side of one HTTP/1.1 connection: a fixed buffer that reads
//! the response head as bytes arrive, keeps it for the header accessors,
//! and hands the body out — stashed over-read first, then straight from the
//! socket, through the chunk decoder when the response is chunked.
//!
//! Generic over the socket: the caller passes `recv`, a closure that fills
//! a slice and says how many bytes landed (`Ok(0)` is orderly EOF). The
//! buffer size `N` and the read size `CHUNK` are const parameters so the
//! device's shape ([`RX_BUF_SIZE`], [`IO_CHUNK`]) and a small one for tests
//! and fuzzing compile to the same code. No I/O, no allocation, no JVM:
//! picodroid-core's `net::http_connection` turns each [`RxError`] into the
//! Java exception it always threw.

use crate::{find_header_end, is_chunked, parse_head_bytes, ChunkDecoder, ChunkEvent};

/// The device's receive buffer: the response head must fit in it.
pub const RX_BUF_SIZE: usize = 1024;
/// How much one body read pulls off the socket at most.
pub const IO_CHUNK: usize = 256;

/// Why a read could not proceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxError<E> {
    /// The socket failed; the caller's error, untouched.
    Transport(E),
    /// The head does not fit the buffer — or, for a chunked response,
    /// leaves less than one `CHUNK` of window for the refills behind it.
    HeadTooLarge,
    /// EOF before the head ended.
    UnexpectedEof,
    /// The status line does not parse. [`RxBuffer::head`] holds the head
    /// as received, for the message.
    BadStatusLine,
    /// Malformed chunked framing.
    BadChunk,
    /// EOF inside a chunked body.
    EofInChunk,
}

/// Per-connection receive state, `N` bytes of buffer and `CHUNK`-sized
/// socket reads.
pub struct RxBuffer<const N: usize, const CHUNK: usize> {
    buf: [u8; N],
    /// Next unread byte of the stash.
    head: u16,
    /// End of the bytes received so far.
    tail: u16,
    /// Length of the response head (through the `\r\n\r\n`). Body reads never
    /// write back into `buf[..head_len]`, so the head stays intact for the
    /// header accessors, which re-scan it in place.
    head_len: u16,
    parsed: bool,
    status_code: i32,
    /// `-1` if absent (or chunked: the framing wins).
    content_length: i64,
    /// `i64::MAX` while unknown (read till EOF); `0` at the end of the body.
    body_remaining: i64,
    chunked: bool,
    chunk: ChunkDecoder,
    /// The chunk decoder met malformed framing. The bytes decoded before it
    /// were handed out; every read from now on is [`RxError::BadChunk`].
    bad_chunk: bool,
}

impl<const N: usize, const CHUNK: usize> Default for RxBuffer<N, CHUNK> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize, const CHUNK: usize> RxBuffer<N, CHUNK> {
    pub fn new() -> Self {
        const {
            assert!(N <= u16::MAX as usize, "offsets are u16");
            assert!(CHUNK >= 1 && CHUNK <= N, "a chunk must fit the buffer");
        }
        Self {
            buf: [0; N],
            head: 0,
            tail: 0,
            head_len: 0,
            parsed: false,
            status_code: -1,
            content_length: -1,
            body_remaining: i64::MAX,
            chunked: false,
            chunk: ChunkDecoder::new(),
            bad_chunk: false,
        }
    }

    /// Whether [`read_head`](Self::read_head) has succeeded.
    pub fn head_parsed(&self) -> bool {
        self.parsed
    }

    /// The response head through its `\r\n\r\n`; empty until found. After
    /// [`RxError::BadStatusLine`] it holds the head that failed to parse.
    pub fn head(&self) -> &[u8] {
        &self.buf[..usize::from(self.head_len)]
    }

    pub fn status_code(&self) -> i32 {
        self.status_code
    }

    /// `Content-Length`, or `-1` when absent or chunked.
    pub fn content_length(&self) -> i64 {
        self.content_length
    }

    pub fn is_chunked(&self) -> bool {
        self.chunked
    }

    /// Bytes of body still to come: `i64::MAX` while unknown, `0` at the end.
    pub fn body_remaining(&self) -> i64 {
        self.body_remaining
    }

    /// `(head, tail, head_len)` of the stash, for tests and the fuzz target.
    pub fn window(&self) -> (usize, usize, usize) {
        (
            usize::from(self.head),
            usize::from(self.tail),
            usize::from(self.head_len),
        )
    }

    /// Receive until `\r\n\r\n`, then parse the status line and
    /// `Content-Length`. Bytes past the terminator stay stashed for the
    /// body reads. Idempotent once parsed.
    ///
    /// The search resumes three bytes before the previous end so a
    /// terminator split across two reads is still seen.
    pub fn read_head<E, F>(&mut self, mut recv: F) -> Result<(), RxError<E>>
    where
        F: FnMut(&mut [u8]) -> Result<usize, E>,
    {
        if self.parsed {
            return Ok(());
        }
        let mut scan_from = 0usize;
        loop {
            let tail = usize::from(self.tail);
            if tail >= N {
                return Err(RxError::HeadTooLarge);
            }
            let n = recv(&mut self.buf[tail..]).map_err(RxError::Transport)?;
            if n == 0 {
                return Err(RxError::UnexpectedEof);
            }
            let n = n.min(N - tail);
            self.tail += n as u16;
            let start = scan_from.saturating_sub(3);
            let end = usize::from(self.tail);
            if let Some(body_off) = find_header_end(&self.buf[..end], start) {
                let head = &self.buf[..body_off];
                let parsed = parse_head_bytes(head);
                let chunked = is_chunked(head);
                // Keep the head for the error message either way.
                self.head_len = body_off as u16;
                let (status, content_length) = parsed.map_err(|_| RxError::BadStatusLine)?;
                // A chunked body refills into `buf[head_len..]` for the whole
                // response (the head stays for the header accessors), so a
                // head that leaves less than one chunk of window would make
                // every refill a zero-length read — "unexpected end of
                // stream inside chunked body" on a well-formed response.
                // Refuse it as a head too large, the same answer a head over
                // the buffer gets.
                if chunked && body_off > N - CHUNK {
                    return Err(RxError::HeadTooLarge);
                }
                self.status_code = status;
                self.chunked = chunked;
                // A chunked response has no usable Content-Length (RFC 9112
                // forbids combining them; the framing wins).
                self.content_length = if chunked { -1 } else { content_length };
                if !chunked && content_length >= 0 {
                    self.body_remaining = content_length;
                }
                self.head = body_off as u16;
                self.parsed = true;
                return Ok(());
            }
            scan_from = end;
        }
    }

    /// One body read into `out`: the stash first, then one `recv`; a chunked
    /// body runs through the decoder and refills `buf[head_len..]`. Reads
    /// the head first if it has not been. `Ok(0)` is the end of the body
    /// (Java's `-1`); an empty `out` is also `Ok(0)`.
    pub fn read_body<E, F>(&mut self, out: &mut [u8], mut recv: F) -> Result<usize, RxError<E>>
    where
        F: FnMut(&mut [u8]) -> Result<usize, E>,
    {
        if out.is_empty() {
            return Ok(0);
        }
        if !self.parsed {
            self.read_head(&mut recv)?;
        }
        if self.body_remaining == 0 {
            return Ok(0);
        }
        if self.bad_chunk {
            return Err(RxError::BadChunk);
        }
        if self.chunked {
            return self.chunked_read(out, recv);
        }

        // 1) Drain any bytes the head parser over-read.
        let stashed = usize::from(self.tail - self.head);
        if stashed > 0 {
            let mut take = stashed.min(out.len());
            if self.body_remaining != i64::MAX {
                take = take.min(self.body_remaining as usize);
            }
            let from = usize::from(self.head);
            out[..take].copy_from_slice(&self.buf[from..from + take]);
            self.head += take as u16;
            if self.body_remaining != i64::MAX {
                self.body_remaining -= take as i64;
            }
            return Ok(take);
        }

        // 2) A fresh read straight into the caller's buffer, capped by
        //    `body_remaining` when Content-Length is known.
        let mut want = out.len().min(CHUNK);
        if self.body_remaining != i64::MAX {
            want = want.min(self.body_remaining as usize);
        }
        match recv(&mut out[..want]) {
            // `Ok(0)` is orderly EOF: the end of a read-till-EOF body, or a
            // Content-Length body that ended early.
            Ok(0) => {
                self.body_remaining = 0;
                Ok(0)
            }
            Ok(n) => {
                let n = n.min(want);
                if self.body_remaining != i64::MAX {
                    self.body_remaining -= n as i64;
                }
                Ok(n)
            }
            Err(e) => Err(RxError::Transport(e)),
        }
    }

    /// Body read for a `Transfer-Encoding: chunked` response: run every wire
    /// byte (stashed head-overrun first, then fresh reads) through the chunk
    /// decoder, storing only payload bytes. Returns as soon as at least one
    /// payload byte landed, `Ok(0)` at the terminal chunk.
    fn chunked_read<E, F>(&mut self, out: &mut [u8], mut recv: F) -> Result<usize, RxError<E>>
    where
        F: FnMut(&mut [u8]) -> Result<usize, E>,
    {
        let mut produced = 0usize;
        loop {
            // Drain what the buffer holds through the decoder.
            while produced < out.len() && self.head < self.tail {
                let b = self.buf[usize::from(self.head)];
                self.head += 1;
                match self.chunk.push(b) {
                    ChunkEvent::Byte(x) => {
                        out[produced] = x;
                        produced += 1;
                    }
                    ChunkEvent::None => {}
                    ChunkEvent::Done => {
                        self.body_remaining = 0;
                        return Ok(produced);
                    }
                    ChunkEvent::Bad => {
                        // The payload decoded so far is good; hand it out
                        // and fail the next read, so nothing before the
                        // malformation is lost.
                        self.bad_chunk = true;
                        if produced > 0 {
                            return Ok(produced);
                        }
                        return Err(RxError::BadChunk);
                    }
                }
            }
            if produced > 0 {
                return Ok(produced);
            }
            // Buffer empty and nothing produced yet: pull more wire bytes.
            // The head is parsed, so everything behind it can be recycled.
            self.head = self.head_len;
            self.tail = self.head_len;
            let start = usize::from(self.head_len);
            let n = recv(&mut self.buf[start..]).map_err(RxError::Transport)?;
            if n == 0 {
                return Err(RxError::EofInChunk);
            }
            self.tail += n.min(N - start) as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// A socket whose bytes are `wire`, handed out `step` at a time.
    struct Feed<'a> {
        wire: &'a [u8],
        pos: usize,
        step: usize,
    }

    impl Feed<'_> {
        fn recv(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
            let n = self.step.min(buf.len()).min(self.wire.len() - self.pos);
            buf[..n].copy_from_slice(&self.wire[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    fn body<const N: usize, const C: usize>(wire: &[u8], step: usize, read: usize) -> Vec<u8> {
        let mut rx = RxBuffer::<N, C>::new();
        let mut feed = Feed { wire, pos: 0, step };
        let mut out = Vec::new();
        let mut buf = alloc::vec![0u8; read];
        loop {
            let n = rx.read_body(&mut buf, |b| feed.recv(b)).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        out
    }

    const LENGTH: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello world";
    const CHUNKED: &[u8] =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5\r\npedia\r\n0\r\nX: y\r\n\r\n";

    #[test]
    fn the_head_is_found_at_any_split() {
        let off = find_header_end(LENGTH, 0).unwrap();
        for step in 1..=LENGTH.len() {
            let mut rx = RxBuffer::<256, 32>::new();
            let mut feed = Feed {
                wire: LENGTH,
                pos: 0,
                step,
            };
            rx.read_head(|b| feed.recv(b)).unwrap();
            assert_eq!(rx.head(), &LENGTH[..off], "step {step}");
            assert_eq!(rx.status_code(), 200);
            assert_eq!(rx.content_length(), 11);
            let (h, t, hl) = rx.window();
            assert_eq!(hl, off);
            assert_eq!(h, off);
            assert!(t >= h && t <= LENGTH.len());
        }
    }

    #[test]
    fn a_content_length_body_drains_the_stash_then_reads() {
        for step in [1, 7, 64] {
            for read in [1, 3, 32] {
                assert_eq!(body::<256, 32>(LENGTH, step, read), b"hello world");
            }
        }
        // Read-till-EOF with no length.
        let eof = b"HTTP/1.0 200 OK\r\n\r\nbytes until close";
        assert_eq!(body::<128, 16>(eof, 5, 4), b"bytes until close");
        // No body at all.
        assert_eq!(
            body::<128, 16>(b"HTTP/1.1 204 No Content\r\n\r\n", 9, 8),
            b""
        );
    }

    #[test]
    fn a_chunked_body_refills_behind_the_head() {
        for step in [1, 3, 16, 200] {
            for read in [1, 2, 64] {
                assert_eq!(
                    body::<96, 32>(CHUNKED, step, read),
                    b"Wikipedia",
                    "{step}/{read}"
                );
            }
        }
        // After the end, more reads say end, and the window stayed behind
        // the head.
        let mut rx = RxBuffer::<96, 32>::new();
        let mut feed = Feed {
            wire: CHUNKED,
            pos: 0,
            step: 8,
        };
        let mut buf = [0u8; 16];
        while rx.read_body(&mut buf, |b| feed.recv(b)).unwrap() != 0 {}
        assert_eq!(rx.read_body(&mut buf, |b| feed.recv(b)), Ok(0));
        let (h, _, hl) = rx.window();
        assert!(h >= hl);
    }

    #[test]
    fn heads_that_do_not_fit_are_refused_and_stay_refused() {
        let mut big = alloc::vec::Vec::from(&b"HTTP/1.1 200 OK\r\n"[..]);
        while big.len() < 300 {
            big.extend_from_slice(b"X-Pad: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
        }
        big.extend_from_slice(b"\r\n");
        let mut rx = RxBuffer::<128, 32>::new();
        let mut feed = Feed {
            wire: &big,
            pos: 0,
            step: 50,
        };
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::HeadTooLarge));
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::HeadTooLarge));
        assert!(!rx.head_parsed());

        // A head of exactly N bytes with no body fits.
        let mut exact = alloc::vec::Vec::from(&b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n"[..]);
        let pad = 128 - exact.len() - 4 - 2;
        exact.extend_from_slice(b"X:");
        exact.extend(core::iter::repeat_n(b'a', pad));
        exact.extend_from_slice(b"\r\n\r\n");
        assert_eq!(exact.len(), 128);
        let mut rx = RxBuffer::<128, 32>::new();
        let mut feed = Feed {
            wire: &exact,
            pos: 0,
            step: 128,
        };
        assert_eq!(rx.read_head(|b| feed.recv(b)), Ok(()));
        assert_eq!(rx.head(), &exact[..]);

        // A chunked head over N - CHUNK is refused: its refills would never
        // have room.
        let mut chunked =
            alloc::vec::Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n"[..]);
        while chunked.len() < 100 {
            chunked.extend_from_slice(b"X-Pad: aaaaaaaa\r\n");
        }
        chunked.extend_from_slice(b"\r\n0\r\n\r\n");
        let mut rx = RxBuffer::<128, 32>::new();
        let mut feed = Feed {
            wire: &chunked,
            pos: 0,
            step: 128,
        };
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::HeadTooLarge));
    }

    #[test]
    fn eof_and_bad_lines_are_named() {
        let mut rx = RxBuffer::<64, 16>::new();
        let mut feed = Feed {
            wire: b"HTTP/1.1 200",
            pos: 0,
            step: 64,
        };
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::UnexpectedEof));
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::UnexpectedEof));

        let mut rx = RxBuffer::<64, 16>::new();
        let mut feed = Feed {
            wire: b"garbage\r\n\r\nbody",
            pos: 0,
            step: 64,
        };
        assert_eq!(rx.read_head(|b| feed.recv(b)), Err(RxError::BadStatusLine));
        assert_eq!(rx.head(), b"garbage\r\n\r\n");
        assert!(!rx.head_parsed());

        let mut rx = RxBuffer::<96, 32>::new();
        let mut feed = Feed {
            wire: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWi",
            pos: 0,
            step: 96,
        };
        let mut buf = [0u8; 8];
        assert_eq!(rx.read_body(&mut buf, |b| feed.recv(b)), Ok(2));
        assert_eq!(
            rx.read_body(&mut buf, |b| feed.recv(b)),
            Err(RxError::EofInChunk)
        );

        let mut rx = RxBuffer::<96, 32>::new();
        let mut feed = Feed {
            wire: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
            pos: 0,
            step: 96,
        };
        assert_eq!(
            rx.read_body(&mut buf, |b| feed.recv(b)),
            Err(RxError::BadChunk)
        );
        assert_eq!(
            rx.read_body(&mut buf, |b| feed.recv(b)),
            Err(RxError::BadChunk)
        );

        // Payload decoded before the malformation reaches the caller; the
        // next read fails.
        let mut rx = RxBuffer::<96, 32>::new();
        let mut feed = Feed {
            wire: b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n;ext=a\r\n0\r\n\r\n",
            pos: 0,
            step: 96,
        };
        let mut big = [0u8; 32];
        assert_eq!(rx.read_body(&mut big, |b| feed.recv(b)), Ok(4));
        assert_eq!(&big[..4], b";ext");
        assert_eq!(
            rx.read_body(&mut big, |b| feed.recv(b)),
            Err(RxError::BadChunk)
        );

        let mut rx = RxBuffer::<64, 16>::new();
        assert_eq!(
            rx.read_head(|_: &mut [u8]| Err::<usize, u8>(7)),
            Err(RxError::Transport(7))
        );
    }
}
