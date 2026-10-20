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
use super::http_head::conn::{RxBuffer, RxError, IO_CHUNK, RX_BUF_SIZE};
use super::http_head::{
    header_lines_of, header_matches, header_name, header_value, reason_phrase, status_line_of,
    write_bytes, write_usize,
};
use super::http_table;
use crate::shrink_names::c;

const TX_BUF_SIZE: usize = 512;

/// The receive side at the device's shape (`http_head::conn`): the head
/// parse, the stash and the chunked refill live there, where the host
/// tests and fuzzes them; this file turns its errors into exceptions.
type Rx = RxBuffer<RX_BUF_SIZE, IO_CHUNK>;

/// What an `HttpConn` reads and writes: the HAL socket, or on a `has_tls`
/// board a TLS session over it. One shape for the request and response
/// code either way.
enum Transport {
    Plain(*mut c_void),
    #[cfg(has_tls)]
    Tls(super::tls::TlsStream),
}

impl Transport {
    fn send(&mut self, data: &[u8]) -> Result<usize, NetError> {
        match self {
            Transport::Plain(sock) => crate::hal::net::tcp_send(*sock, data),
            #[cfg(has_tls)]
            Transport::Tls(tls) => tls.send(data),
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> Result<usize, NetError> {
        match self {
            Transport::Plain(sock) => crate::hal::net::tcp_recv(*sock, buf),
            #[cfg(has_tls)]
            Transport::Tls(tls) => tls.recv(buf),
        }
    }

    /// `close_notify` first on a TLS session, then the socket's graceful close.
    fn close(self) {
        match self {
            Transport::Plain(sock) => crate::hal::net::close(sock),
            #[cfg(has_tls)]
            Transport::Tls(tls) => crate::hal::net::close(tls.close()),
        }
    }
}

/// Per-connection state.  Boxed; the raw pointer is stored in the Java
/// `handle` field via [`http_table`].
struct HttpConn {
    transport: Transport,
    /// The response: head, stash, chunk decoder, body countdown. The head
    /// stays in the buffer for the header accessors, which re-scan it in
    /// place instead of allocating a parsed table.
    rx: Rx,
}

impl HttpConn {
    fn new(transport: Transport) -> Self {
        Self {
            transport,
            rx: Rx::new(),
        }
    }

    /// The retained response head, empty until it has been parsed.
    fn head(&self) -> &[u8] {
        self.rx.head()
    }
}

/// An [`RxError`] as the exception the connection always threw for it.
fn rx_error(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    rx: &Rx,
    e: RxError<NetError>,
) -> JvmError {
    match e {
        RxError::Transport(e) => throw_net_exception(objects, strings, e, NetOpCtx::Recv),
        RxError::HeadTooLarge => throw_io_exception(objects, strings, "response headers too large"),
        RxError::UnexpectedEof => throw_named_exception(
            objects,
            strings,
            c::java_net_ProtocolException,
            "unexpected end of stream",
        ),
        RxError::BadStatusLine => {
            let line = status_line_of(rx.head());
            let msg = format!("unexpected status line: {}", String::from_utf8_lossy(line));
            throw_named_exception(objects, strings, c::java_net_ProtocolException, &msg)
        }
        RxError::BadChunk => throw_named_exception(
            objects,
            strings,
            c::java_net_ProtocolException,
            "malformed chunked framing",
        ),
        RxError::EofInChunk => throw_named_exception(
            objects,
            strings,
            c::java_net_ProtocolException,
            "unexpected end of stream inside chunked body",
        ),
    }
}

/// Read the response head off the socket if it has not been read.
///
/// Malformed *server* data (truncated head, unparseable status line) is a
/// `java/net/ProtocolException`, not a JVM fault; transport failures map
/// through the usual `Recv` taxonomy (`SocketTimeoutException` on expiry).
fn read_head(
    conn: &mut HttpConn,
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
) -> Result<(), JvmError> {
    let HttpConn { transport, rx } = conn;
    rx.read_head(|buf| transport.recv(buf))
        .map_err(|e| rx_error(objects, strings, rx, e))
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
/// String method, int bodyLength, int connectTimeoutMs, int readTimeoutMs,
/// String extraHeaders, boolean tls) -> int`.
///
/// Resolves the host, opens a TCP socket — and, for an `https` URL, runs
/// the TLS handshake over it — then sends the request line + minimal
/// headers.  Returns the new handle.  Failures surface as the typed
/// `java.net` taxonomy: `UnknownHostException` when resolution fails,
/// `ConnectException`/`SocketTimeoutException` from the TCP connect,
/// `javax.net.ssl.SSLHandshakeException` from the handshake, and
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
    // `https`: the Java side derives it from the URL's scheme.
    let tls = as_int(args.get(8))? != 0;

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

    // The TLS handshake, on a board that has one. Its record reads run
    // under the read timeout set just above.
    #[cfg(has_tls)]
    let mut transport = if tls {
        match super::tls::TlsStream::open(sock, &host) {
            Ok(stream) => Transport::Tls(stream),
            Err(fail) => {
                crate::hal::net::close(sock);
                return Err(super::tls::throw_tls_fail(objects, strings, fail, &host));
            }
        }
    } else {
        Transport::Plain(sock)
    };
    #[cfg(not(has_tls))]
    let mut transport = {
        if tls {
            crate::hal::net::close(sock);
            return Err(throw_named_exception(
                objects,
                strings,
                c::java_lang_UnsupportedOperationException,
                "HTTPS is not supported on this board",
            ));
        }
        Transport::Plain(sock)
    };

    // Build the request head in a stack buffer and send it.  For HTTP/1.1
    // we're required to send Host; Connection: close keeps our cleanup
    // single-path (no keep-alive reuse).
    let mut head = HeadBuf::new();
    head.push(method.as_bytes());
    head.push(b" ");
    head.push(path.as_bytes());
    head.push(b" HTTP/1.1\r\nHost: ");
    head.push(host.as_bytes());
    let default_port = if tls { 443 } else { 80 };
    if port != default_port {
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
        transport.close();
        return Err(throw_io_exception(
            objects,
            strings,
            "request header too large",
        ));
    }

    if let Err(e) = send_all(&mut transport, &head.buf[..head.pos]) {
        transport.close();
        return Err(throw_net_exception(objects, strings, e, NetOpCtx::Send));
    }

    // A fallible box: on a tight board the arena can be down to fragments
    // here, after the TLS handshake's transient (2026-09-27,
    // pico_enviro_mon_w: less than this 1 KB was left and `Box::new`
    // HardFaulted the device). An IOException keeps a fail-soft caller alive.
    let layout = core::alloc::Layout::new::<HttpConn>();
    // SAFETY: `HttpConn` is not zero-sized.
    let raw = unsafe { alloc::alloc::alloc(layout) as *mut HttpConn };
    if raw.is_null() {
        transport.close();
        return Err(throw_io_exception(
            objects,
            strings,
            "out of memory for the connection",
        ));
    }
    // SAFETY: `raw` is a fresh allocation with `HttpConn`'s layout, so a
    // later `Box::from_raw` (disconnect, or the error path below) frees it
    // the way `Box::new` would have.
    unsafe { raw.write(HttpConn::new(transport)) };
    let handle = http_table::register(raw as *mut c_void);
    if handle == 0 {
        // SAFETY: `raw` came from Box::into_raw above and was never shared.
        let conn = unsafe { Box::from_raw(raw) };
        conn.transport.close();
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
    read_head(conn, objects, strings)?;
    Ok(Some(Value::Int(conn.rx.status_code())))
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
    let content_length = conn.rx.content_length();
    let len = if content_length < 0 || content_length > i32::MAX as i64 {
        -1
    } else {
        content_length as i32
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
    if !conn.rx.head_parsed() {
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
    conn.transport.close();
    http_table::remove(handle);
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
        match conn.transport.send(&buf[..chunk]) {
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
    // One read into a stack buffer, then mirror it to the JVM array: the
    // stash, or one `recv` of at most IO_CHUNK, through the chunk decoder on
    // a chunked response. `Ok(0)` is the end of the body: orderly EOF on
    // every platform (the device HAL remaps FreeRTOS's inverted encoding),
    // so `-1` here really means end-of-stream, and a stalled-but-alive
    // server throws `SocketTimeoutException` instead of reading as EOF.
    let mut buf = [0u8; IO_CHUNK];
    let want = core::cmp::min(len, IO_CHUNK);
    let HttpConn { transport, rx } = conn;
    match rx.read_body(&mut buf[..want], |b| transport.recv(b)) {
        Ok(0) => Ok(Some(Value::Int(-1))),
        Ok(n) => {
            for (i, &b) in buf.iter().enumerate().take(n) {
                arrays
                    .store(arr_idx, off + i, b as i8 as i32)
                    .ok_or(JvmError::InvalidReference)?;
            }
            Ok(Some(Value::Int(n as i32)))
        }
        Err(e) => Err(rx_error(objects, strings, rx, e)),
    }
}

// ── plumbing ─────────────────────────────────────────────────────────────────

fn send_all(transport: &mut Transport, mut buf: &[u8]) -> Result<(), NetError> {
    while !buf.is_empty() {
        let n = transport.send(buf)?;
        if n == 0 {
            // A blocking send that makes no progress means the peer is gone.
            return Err(NetError::new(NetErrorKind::Closed, 0));
        }
        buf = &buf[n..];
    }
    Ok(())
}
