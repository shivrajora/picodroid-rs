// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.HttpURLConnection / HttpInputStream
//! / HttpOutputStream.
//!
//! The Java class is a thin shim; all protocol logic lives here.  An
//! `HttpConn` is allocated on connect and freed on disconnect, with its
//! pointer round-tripped through [`super::http_table`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::ffi::c_void;

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use crate::hal::types::{NetError, NetErrorKind};

use super::fields;
use super::helpers::{
    throw_io_exception, throw_named_exception, throw_net_exception, throw_socket_closed, NetOpCtx,
};
use super::http_head::{
    find_header_end, header_lines_of, header_matches, header_name, header_value, parse_head_bytes,
    reason_phrase, status_line_of, write_bytes, write_usize,
};
use super::http_table;

const RX_BUF_SIZE: usize = 1024;
const TX_BUF_SIZE: usize = 512;
const IO_CHUNK: usize = 256;

/// Per-connection state.  Boxed; the raw pointer is stored in the Java
/// `handle` field via [`http_table`].
struct HttpConn {
    socket: *mut c_void,
    headers_parsed: bool,
    status_code: i32,
    content_length: i64, // -1 if absent
    body_remaining: i64, // i64::MAX if Content-Length unknown (read-til-EOF)
    /// `Transfer-Encoding: chunked`: body reads run through `chunk` and the
    /// wire framing (size lines, CRLFs, trailers) never reaches the app.
    /// Without this the read-til-EOF fallback handed hex size lines and the
    /// 0\r\n\r\n terminator to the caller as body bytes (bugbash F7).
    chunked: bool,
    chunk: crate::net::http_head::ChunkDecoder,
    /// Bytes the header parser read past `\r\n\r\n` — handed to the first
    /// body reads before any new `tcp_recv`.
    rx_buf: [u8; RX_BUF_SIZE],
    rx_head: u16,
    rx_tail: u16,
    /// Length of the response head (through the `\r\n\r\n`) once parsed.
    /// Body reads never write back into `rx_buf[..head_len]` — they drain
    /// from `rx_head` and then read straight into the caller's array — so
    /// the head stays intact and the header accessors re-scan it in place
    /// instead of allocating a parsed table.
    head_len: u16,
}

impl HttpConn {
    fn new(socket: *mut c_void) -> Self {
        Self {
            socket,
            headers_parsed: false,
            status_code: -1,
            content_length: -1,
            body_remaining: i64::MAX,
            chunked: false,
            chunk: crate::net::http_head::ChunkDecoder::new(),
            rx_buf: [0; RX_BUF_SIZE],
            rx_head: 0,
            rx_tail: 0,
            head_len: 0,
        }
    }

    /// The retained response head, empty until it has been parsed.
    fn head(&self) -> &[u8] {
        &self.rx_buf[..self.head_len as usize]
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn as_int(v: Option<&Value>) -> Result<i32, JvmError> {
    match v {
        Some(Value::Int(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_ref(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::Reference(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_array(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ArrayRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

fn as_obj(v: Option<&Value>) -> Result<u16, JvmError> {
    match v {
        Some(Value::ObjectRef(i)) => Ok(*i),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Get the Box'd `HttpConn` behind a handle.  Returns `None` if the handle
/// is stale or was already freed.
fn conn_mut(handle: i32) -> Option<&'static mut HttpConn> {
    let ptr = http_table::lookup(handle) as *mut HttpConn;
    if ptr.is_null() {
        return None;
    }
    // SAFETY: Pointer was produced by Box::into_raw in native_connect and
    // hasn't yet been freed (freed only by native_disconnect).
    Some(unsafe { &mut *ptr })
}

fn handle_from_obj(args: &[Value], objects: &ObjectHeap, field: usize) -> Result<i32, JvmError> {
    let idx = as_obj(args.first())?;
    match objects.get_field(idx, field) {
        Some(Value::Int(h)) => Ok(h),
        _ => Err(JvmError::InvalidReference),
    }
}

// ── HttpURLConnection.nativeConnect (static) ─────────────────────────────────

/// Java signature: `nativeConnect(String host, int port, String path,
/// String method, int bodyLength) -> int`.
///
/// Resolves the host, opens a TCP socket, and sends the request line +
/// minimal headers.  Returns the new handle.  Failures surface as the typed
/// `java.net` taxonomy: `UnknownHostException` when resolution fails,
/// `ConnectException`/`SocketTimeoutException` from the TCP connect, and
/// `SocketException`/`IOException` from the request send.
pub fn native_connect(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let host_idx = as_ref(args.first())?;
    let port = as_int(args.get(1))? as u16;
    let path_idx = as_ref(args.get(2))?;
    let method_idx = as_ref(args.get(3))?;
    let body_length = as_int(args.get(4))?;
    // 0 = infinite, per the Android URLConnection contract; the Java side
    // rejects negatives.
    let connect_timeout_ms = as_int(args.get(5))? as u32;
    let read_timeout_ms = as_int(args.get(6))? as u32;
    // Caller-supplied request headers, already formatted as `K: V\r\n` lines
    // by the Java side (which owns ordering, replace-vs-add, and rejecting
    // CR/LF injection). Empty when the app set none.
    let extra_headers_idx = as_ref(args.get(7))?;

    // Owned copies: the throw helpers below need `&mut strings` while these
    // would otherwise still be borrowed from the table.
    let host: String = strings
        .resolve(host_idx)
        .ok_or(JvmError::InvalidReference)?
        .into();
    let path: String = strings
        .resolve(path_idx)
        .ok_or(JvmError::InvalidReference)?
        .into();
    let method: String = strings
        .resolve(method_idx)
        .ok_or(JvmError::InvalidReference)?
        .into();
    let extra_headers: String = strings
        .resolve(extra_headers_idx)
        .ok_or(JvmError::InvalidReference)?
        .into();

    // Resolve hostname → packed IPv4.
    let addr = crate::hal::net::dns_resolve(&host)
        .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Dns(&host)))?;

    // Open and connect TCP socket.
    let sock = crate::hal::net::tcp_socket().map_err(|e| {
        throw_io_exception(
            objects,
            strings,
            &format!("socket create failed (err {})", e.raw),
        )
    })?;
    // A receive timeout set *before* connect doubles as the connect
    // deadline: `FreeRTOS_connect` blocks on the socket's RCVTIMEO
    // (FreeRTOS_Sockets.c:3819), and the sim HAL implements the same
    // pre-connect contract.
    if connect_timeout_ms > 0 {
        crate::hal::net::set_recv_timeout(sock, connect_timeout_ms);
    }
    if let Err(e) = crate::hal::net::tcp_connect(sock, addr, port) {
        crate::hal::net::close(sock);
        return Err(throw_net_exception(objects, strings, e, NetOpCtx::Connect));
    }
    // From here on, RCVTIMEO is the read timeout. "No read timeout" cannot
    // be expressed as 0 once a connect timeout was set — 0 means
    // non-blocking on the device stack — so restore an effectively-infinite
    // window instead.
    if read_timeout_ms > 0 {
        crate::hal::net::set_recv_timeout(sock, read_timeout_ms);
    } else if connect_timeout_ms > 0 {
        crate::hal::net::set_recv_timeout(sock, u32::MAX);
    }

    // Build the request head in a stack buffer and send it.  For HTTP/1.1
    // we're required to send Host; Connection: close keeps our cleanup
    // single-path (no keep-alive reuse).
    let mut head = HeadBuf::new();
    head.push(method.as_bytes());
    head.push(b" ");
    head.push(path.as_bytes());
    head.push(b" HTTP/1.1\r\nHost: ");
    head.push(host.as_bytes());
    if port != 80 {
        head.push(b":");
        head.push_usize(port as usize);
    }
    head.push(b"\r\nConnection: close\r\n");
    if body_length >= 0 {
        head.push(b"Content-Length: ");
        head.push_usize(body_length as usize);
        head.push(b"\r\n");
    }
    head.push(extra_headers.as_bytes());
    head.push(b"\r\n");

    if head.overflow {
        crate::hal::net::close(sock);
        return Err(throw_io_exception(
            objects,
            strings,
            "request header too large",
        ));
    }

    if let Err(e) = send_all(sock, &head.buf[..head.pos]) {
        crate::hal::net::close(sock);
        return Err(throw_net_exception(objects, strings, e, NetOpCtx::Send));
    }

    let boxed = Box::new(HttpConn::new(sock));
    let raw = Box::into_raw(boxed);
    let handle = http_table::register(raw as *mut c_void);
    if handle == 0 {
        // SAFETY: `raw` came from Box::into_raw above and was never shared.
        let conn = unsafe { Box::from_raw(raw) };
        crate::hal::net::close(conn.socket);
        return Err(throw_io_exception(
            objects,
            strings,
            "too many open connections",
        ));
    }
    Ok(Some(Value::Int(handle)))
}

/// Fixed-capacity request-head builder.  `overflow` latches when any piece
/// fails to fit, so a truncated (garbled) head is never sent — the old
/// `pos > TX_BUF_SIZE` check could not fire because `write_bytes` already
/// refuses the write that would overflow.
struct HeadBuf {
    buf: [u8; TX_BUF_SIZE],
    pos: usize,
    overflow: bool,
}

impl HeadBuf {
    fn new() -> Self {
        Self {
            buf: [0; TX_BUF_SIZE],
            pos: 0,
            overflow: false,
        }
    }

    fn push(&mut self, src: &[u8]) {
        let n = write_bytes(&mut self.buf, self.pos, src);
        if n != src.len() {
            self.overflow = true;
        }
        self.pos += n;
    }

    fn push_usize(&mut self, val: usize) {
        let n = write_usize(&mut self.buf, self.pos, val);
        if n == 0 {
            self.overflow = true;
        }
        self.pos += n;
    }
}

// ── HttpURLConnection.nativeReadResponseCode (static) ────────────────────────

pub fn native_read_response_code(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let handle = as_int(args.first())?;
    let conn = match conn_mut(handle) {
        Some(c) => c,
        None => return Err(throw_socket_closed(objects, strings)),
    };
    if !conn.headers_parsed {
        parse_response_head(conn, objects, strings)?;
    }
    Ok(Some(Value::Int(conn.status_code)))
}

// ── HttpURLConnection.nativeContentLength (static) ───────────────────────────

pub fn native_content_length(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let handle = as_int(args.first())?;
    let conn = match conn_mut(handle) {
        Some(c) => c,
        None => return Err(throw_socket_closed(objects, strings)),
    };
    // content_length is stored as i64; Java return type is int, so clamp.
    let len = if conn.content_length < 0 || conn.content_length > i32::MAX as i64 {
        -1
    } else {
        conn.content_length as i32
    };
    Ok(Some(Value::Int(len)))
}

// ── HttpURLConnection response-header accessors (static) ─────────────────────

/// Intern `bytes` and return it as a Java String reference.
fn interned(strings: &mut StringTable, bytes: &[u8]) -> Result<Option<Value>, JvmError> {
    let r = strings.intern_dyn(bytes).ok_or(JvmError::StackOverflow)?;
    Ok(Some(Value::Reference(r)))
}

/// Resolve a connection whose response head has been parsed. `Ok(None)` means
/// the head is not available (not yet read), which every accessor reports to
/// Java as `null` rather than an exception — matching Android, where the
/// getters are non-throwing.
fn conn_with_head(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<&'static mut HttpConn>, JvmError> {
    let handle = as_int(args.first())?;
    let conn = match conn_mut(handle) {
        Some(c) => c,
        None => return Err(throw_socket_closed(objects, strings)),
    };
    if !conn.headers_parsed {
        return Ok(None);
    }
    Ok(Some(conn))
}

/// `getHeaderField(String name)` — the value of the last header with that
/// name (case-insensitive), or null if absent.
pub fn native_header_field(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let name_idx = as_ref(args.get(1))?;
    let name: String = strings
        .resolve(name_idx)
        .ok_or(JvmError::InvalidReference)?
        .to_ascii_lowercase();
    let Some(conn) = conn_with_head(args, objects, strings)? else {
        return Ok(Some(Value::Null));
    };
    // Last match wins, as on Android.
    let found = header_lines_of(conn.head())
        .filter(|l| header_matches(l, name.as_bytes()))
        .filter_map(header_value)
        .last();
    match found {
        Some(v) => interned(strings, v),
        None => Ok(Some(Value::Null)),
    }
}

/// `getHeaderField(int n)` / `getHeaderFieldKey(int n)`. Index 0 is the status
/// line: its value is the whole line and its key is null, per Android. Indices
/// past the last header return null.
pub fn native_header_field_at(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let n = as_int(args.get(1))?;
    let want_key = as_int(args.get(2))? != 0;
    let Some(conn) = conn_with_head(args, objects, strings)? else {
        return Ok(Some(Value::Null));
    };
    if n < 0 {
        return Ok(Some(Value::Null));
    }
    if n == 0 {
        return if want_key {
            Ok(Some(Value::Null))
        } else {
            let line = status_line_of(conn.head());
            interned(strings, line)
        };
    }
    let Some(line) = header_lines_of(conn.head()).nth(n as usize - 1) else {
        return Ok(Some(Value::Null));
    };
    if want_key {
        interned(strings, header_name(line))
    } else {
        match header_value(line) {
            Some(v) => interned(strings, v),
            None => Ok(Some(Value::Null)),
        }
    }
}

/// `getResponseMessage()` — the reason phrase from the status line
/// (`HTTP/1.1 404 Not Found` → `Not Found`), or null if unavailable.
pub fn native_response_message(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<Option<Value>, JvmError> {
    let Some(conn) = conn_with_head(args, objects, strings)? else {
        return Ok(Some(Value::Null));
    };
    match reason_phrase(status_line_of(conn.head())) {
        Some(reason) => interned(strings, reason),
        None => Ok(Some(Value::Null)),
    }
}

// ── HttpURLConnection.nativeDisconnect (static) ──────────────────────────────

pub fn native_disconnect(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = as_int(args.first())?;
    let ptr = http_table::lookup(handle) as *mut HttpConn;
    if ptr.is_null() {
        return Ok(None);
    }
    // SAFETY: Pointer was produced by Box::into_raw in native_connect.
    let conn = unsafe { Box::from_raw(ptr) };
    crate::hal::net::close(conn.socket);
    http_table::remove(handle);
    drop(conn);
    Ok(None)
}

// ── HttpOutputStream.write (instance) ────────────────────────────────────────

pub fn native_output_write(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = handle_from_obj(args, objects, fields::http_output_stream::HANDLE)?;
    let arr_idx = as_array(args.get(1))?;
    let off = as_int(args.get(2))? as usize;
    let len = as_int(args.get(3))? as usize;

    let conn = match conn_mut(handle) {
        Some(c) => c,
        None => return Err(throw_socket_closed(objects, strings)),
    };

    // Stream in chunks — we copy from JVM byte[] into a stack buffer, then
    // hand it to the HAL.  Matches the idiom in socket.rs::send_native.
    let mut sent_total = 0usize;
    while sent_total < len {
        let chunk = core::cmp::min(IO_CHUNK, len - sent_total);
        let mut buf = [0u8; IO_CHUNK];
        for (i, b) in buf.iter_mut().enumerate().take(chunk) {
            *b = arrays
                .load(arr_idx, off + sent_total + i)
                .ok_or(JvmError::ArrayIndexOutOfBounds)? as i8 as u8;
        }
        match crate::hal::net::tcp_send(conn.socket, &buf[..chunk]) {
            // A blocking send that makes no progress means the peer is gone.
            Ok(0) => {
                let e = NetError::new(NetErrorKind::Closed, 0);
                return Err(throw_net_exception(objects, strings, e, NetOpCtx::Send));
            }
            Ok(n) => sent_total += n,
            Err(e) => return Err(throw_net_exception(objects, strings, e, NetOpCtx::Send)),
        }
    }
    Ok(None)
}

// ── HttpInputStream.read (instance) ──────────────────────────────────────────

pub fn native_input_read(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = handle_from_obj(args, objects, fields::http_input_stream::HANDLE)?;
    let arr_idx = as_array(args.get(1))?;
    let off = as_int(args.get(2))? as usize;
    let len = as_int(args.get(3))? as usize;

    if len == 0 {
        return Ok(Some(Value::Int(0)));
    }

    let conn = match conn_mut(handle) {
        Some(c) => c,
        None => return Err(throw_socket_closed(objects, strings)),
    };
    if !conn.headers_parsed {
        parse_response_head(conn, objects, strings)?;
    }
    if conn.body_remaining == 0 {
        return Ok(Some(Value::Int(-1)));
    }
    if conn.chunked {
        return chunked_read(conn, objects, strings, arrays, arr_idx, off, len);
    }

    // 1) Drain any bytes the header parser over-read.
    let stashed = (conn.rx_tail - conn.rx_head) as usize;
    if stashed > 0 {
        let take = core::cmp::min(stashed, len);
        let take = core::cmp::min(take, conn.body_remaining as usize);
        for i in 0..take {
            let b = conn.rx_buf[conn.rx_head as usize + i];
            arrays
                .store(arr_idx, off + i, b as i8 as i32)
                .ok_or(JvmError::InvalidReference)?;
        }
        conn.rx_head += take as u16;
        if conn.body_remaining != i64::MAX {
            conn.body_remaining -= take as i64;
        }
        return Ok(Some(Value::Int(take as i32)));
    }

    // 2) Fresh tcp_recv straight into a stack buffer, then mirror to the
    //    JVM array.  Cap by body_remaining if Content-Length is known.
    let mut buf = [0u8; IO_CHUNK];
    let want = core::cmp::min(len, IO_CHUNK);
    let want = if conn.body_remaining == i64::MAX {
        want
    } else {
        core::cmp::min(want, conn.body_remaining as usize)
    };
    match crate::hal::net::tcp_recv(conn.socket, &mut buf[..want]) {
        // `Ok(0)` is orderly EOF on every platform (the device HAL remaps
        // FreeRTOS's inverted encoding) — so `-1` here really means
        // end-of-stream, and a stalled-but-alive server now throws
        // `SocketTimeoutException` instead of reading as a clean EOF.
        Ok(0) => {
            conn.body_remaining = 0;
            Ok(Some(Value::Int(-1)))
        }
        Ok(n) => {
            for (i, &b) in buf.iter().enumerate().take(n) {
                arrays
                    .store(arr_idx, off + i, b as i8 as i32)
                    .ok_or(JvmError::InvalidReference)?;
            }
            if conn.body_remaining != i64::MAX {
                conn.body_remaining -= n as i64;
            }
            Ok(Some(Value::Int(n as i32)))
        }
        Err(e) => Err(throw_net_exception(objects, strings, e, NetOpCtx::Recv)),
    }
}

/// Body read for a `Transfer-Encoding: chunked` response: run every wire
/// byte (stashed head-overrun first, then fresh `tcp_recv`) through the
/// connection's [`ChunkDecoder`], storing only payload bytes. Returns as
/// soon as at least one payload byte landed, `-1` at the terminal chunk.
fn chunked_read(
    conn: &mut HttpConn,
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    arrays: &mut ArrayHeap,
    arr_idx: u16,
    off: usize,
    len: usize,
) -> Result<Option<Value>, JvmError> {
    use crate::net::http_head::ChunkEvent;

    let mut produced = 0usize;
    loop {
        // Drain what the buffer holds through the decoder.
        while produced < len && conn.rx_head < conn.rx_tail {
            let b = conn.rx_buf[conn.rx_head as usize];
            conn.rx_head += 1;
            match conn.chunk.push(b) {
                ChunkEvent::Byte(x) => {
                    arrays
                        .store(arr_idx, off + produced, x as i8 as i32)
                        .ok_or(JvmError::InvalidReference)?;
                    produced += 1;
                }
                ChunkEvent::None => {}
                ChunkEvent::Done => {
                    conn.body_remaining = 0;
                    return Ok(Some(Value::Int(if produced > 0 {
                        produced as i32
                    } else {
                        -1
                    })));
                }
                ChunkEvent::Bad => {
                    return Err(throw_named_exception(
                        objects,
                        strings,
                        "java/net/ProtocolException",
                        "malformed chunked framing",
                    ));
                }
            }
        }
        if produced > 0 {
            return Ok(Some(Value::Int(produced as i32)));
        }
        // Buffer empty and nothing produced yet: pull more wire bytes. The
        // head is parsed, so the whole rx_buf can be recycled.
        conn.rx_head = conn.head_len;
        conn.rx_tail = conn.head_len;
        let start = conn.rx_head as usize;
        let n = crate::hal::net::tcp_recv(conn.socket, &mut conn.rx_buf[start..])
            .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Recv))?;
        if n == 0 {
            return Err(throw_named_exception(
                objects,
                strings,
                "java/net/ProtocolException",
                "unexpected end of stream inside chunked body",
            ));
        }
        conn.rx_tail += n as u16;
    }
}

// ── plumbing ─────────────────────────────────────────────────────────────────

/// Read from the socket until `\r\n\r\n`, then parse the status line and
/// any `Content-Length` header.  Any bytes past the header terminator are
/// left in `conn.rx_buf[rx_head..rx_tail]` for subsequent body reads.
///
/// Malformed *server* data (truncated head, unparseable status line) is a
/// `java/net/ProtocolException`, not a JVM fault; transport failures map
/// through the usual `Recv` taxonomy (`SocketTimeoutException` on expiry).
fn parse_response_head(
    conn: &mut HttpConn,
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<(), JvmError> {
    let mut scan_from = 0usize;
    loop {
        if conn.rx_tail as usize >= RX_BUF_SIZE {
            return Err(throw_io_exception(
                objects,
                strings,
                "response headers too large",
            ));
        }
        let space = &mut conn.rx_buf[conn.rx_tail as usize..];
        let n = crate::hal::net::tcp_recv(conn.socket, space)
            .map_err(|e| throw_net_exception(objects, strings, e, NetOpCtx::Recv))?;
        if n == 0 {
            return Err(throw_named_exception(
                objects,
                strings,
                "java/net/ProtocolException",
                "unexpected end of stream",
            ));
        }
        conn.rx_tail += n as u16;

        // Scan for CRLFCRLF; keep at most the last 3 bytes of prior scan
        // as context for the boundary.
        let start = scan_from.saturating_sub(3);
        let end = conn.rx_tail as usize;
        if let Some(body_off) = find_header_end(&conn.rx_buf[..end], start) {
            let head = &conn.rx_buf[..body_off];
            let (status, content_length) = match parse_head_bytes(head) {
                Ok(v) => v,
                Err(_) => {
                    let line = status_line_of(head);
                    let msg = format!("unexpected status line: {}", String::from_utf8_lossy(line));
                    return Err(throw_named_exception(
                        objects,
                        strings,
                        "java/net/ProtocolException",
                        &msg,
                    ));
                }
            };
            conn.status_code = status;
            conn.chunked = crate::net::http_head::is_chunked(head);
            // A chunked response has no usable Content-Length (RFC 9112
            // forbids combining them; the framing wins).
            conn.content_length = if conn.chunked { -1 } else { content_length };
            if !conn.chunked && content_length >= 0 {
                conn.body_remaining = content_length;
            }
            conn.rx_head = body_off as u16;
            conn.head_len = body_off as u16;
            conn.headers_parsed = true;
            return Ok(());
        }
        scan_from = end;
    }
}

fn send_all(sock: *mut c_void, mut buf: &[u8]) -> Result<(), NetError> {
    while !buf.is_empty() {
        let n = crate::hal::net::tcp_send(sock, buf)?;
        if n == 0 {
            // A blocking send that makes no progress means the peer is gone.
            return Err(NetError::new(NetErrorKind::Closed, 0));
        }
        buf = &buf[n..];
    }
    Ok(())
}
