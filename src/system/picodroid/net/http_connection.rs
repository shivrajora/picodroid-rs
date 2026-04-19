//! Native implementations for picodroid.net.HttpUrlConnection / HttpInputStream
//! / HttpOutputStream.
//!
//! The Java class is a thin shim; all protocol logic lives here.  An
//! `HttpConn` is allocated on connect and freed on disconnect, with its
//! pointer round-tripped through [`super::http_table`].

use alloc::boxed::Box;
use core::ffi::c_void;

use pico_jvm::array_heap::ArrayHeap;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
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
    /// Bytes the header parser read past `\r\n\r\n` — handed to the first
    /// body reads before any new `tcp_recv`.
    rx_buf: [u8; RX_BUF_SIZE],
    rx_head: u16,
    rx_tail: u16,
}

impl HttpConn {
    fn new(socket: *mut c_void) -> Self {
        Self {
            socket,
            headers_parsed: false,
            status_code: -1,
            content_length: -1,
            body_remaining: i64::MAX,
            rx_buf: [0; RX_BUF_SIZE],
            rx_head: 0,
            rx_tail: 0,
        }
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

// ── HttpUrlConnection.nativeConnect (static) ─────────────────────────────────

/// Java signature: `nativeConnect(String host, int port, String path,
/// String method, int bodyLength) -> int`.
///
/// Resolves the host, opens a TCP socket, and sends the request line +
/// minimal headers.  Returns the new handle.
pub fn native_connect(args: &[Value], strings: &StringTable) -> Result<Option<Value>, JvmError> {
    let host_idx = as_ref(args.first())?;
    let port = as_int(args.get(1))? as u16;
    let path_idx = as_ref(args.get(2))?;
    let method_idx = as_ref(args.get(3))?;
    let body_length = as_int(args.get(4))?;

    let host = strings
        .resolve(host_idx)
        .ok_or(JvmError::InvalidReference)?;
    let path = strings
        .resolve(path_idx)
        .ok_or(JvmError::InvalidReference)?;
    let method = strings
        .resolve(method_idx)
        .ok_or(JvmError::InvalidReference)?;

    // Resolve hostname → packed IPv4.
    let addr = crate::hal::net::dns_resolve(host).map_err(|_| JvmError::InvalidReference)?;

    // Open and connect TCP socket.
    let sock = crate::hal::net::tcp_socket().map_err(|_| JvmError::InvalidReference)?;
    if let Err(_) = crate::hal::net::tcp_connect(sock, addr, port) {
        crate::hal::net::close(sock);
        return Err(JvmError::InvalidReference);
    }

    // Build the request head in a stack buffer and send it.  For HTTP/1.1
    // we're required to send Host; Connection: close keeps our cleanup
    // single-path (no keep-alive reuse).
    let mut buf = [0u8; TX_BUF_SIZE];
    let mut pos = 0usize;
    pos += write_bytes(&mut buf, pos, method.as_bytes());
    pos += write_bytes(&mut buf, pos, b" ");
    pos += write_bytes(&mut buf, pos, path.as_bytes());
    pos += write_bytes(&mut buf, pos, b" HTTP/1.1\r\nHost: ");
    pos += write_bytes(&mut buf, pos, host.as_bytes());
    if !(port == 80) {
        pos += write_bytes(&mut buf, pos, b":");
        pos += write_usize(&mut buf, pos, port as usize);
    }
    pos += write_bytes(&mut buf, pos, b"\r\nConnection: close\r\n");
    if body_length >= 0 {
        pos += write_bytes(&mut buf, pos, b"Content-Length: ");
        pos += write_usize(&mut buf, pos, body_length as usize);
        pos += write_bytes(&mut buf, pos, b"\r\n");
    }
    pos += write_bytes(&mut buf, pos, b"\r\n");

    if pos > TX_BUF_SIZE {
        crate::hal::net::close(sock);
        return Err(JvmError::InvalidReference);
    }

    if send_all(sock, &buf[..pos]).is_err() {
        crate::hal::net::close(sock);
        return Err(JvmError::InvalidReference);
    }

    let boxed = Box::new(HttpConn::new(sock));
    let raw = Box::into_raw(boxed);
    let handle = http_table::register(raw as *mut c_void);
    Ok(Some(Value::Int(handle)))
}

// ── HttpUrlConnection.nativeReadResponseCode (static) ────────────────────────

pub fn native_read_response_code(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = as_int(args.first())?;
    let conn = conn_mut(handle).ok_or(JvmError::InvalidReference)?;
    if !conn.headers_parsed {
        parse_response_head(conn)?;
    }
    Ok(Some(Value::Int(conn.status_code)))
}

// ── HttpUrlConnection.nativeContentLength (static) ───────────────────────────

pub fn native_content_length(args: &[Value]) -> Result<Option<Value>, JvmError> {
    let handle = as_int(args.first())?;
    let conn = conn_mut(handle).ok_or(JvmError::InvalidReference)?;
    // content_length is stored as i64; Java return type is int, so clamp.
    let len = if conn.content_length < 0 || conn.content_length > i32::MAX as i64 {
        -1
    } else {
        conn.content_length as i32
    };
    Ok(Some(Value::Int(len)))
}

// ── HttpUrlConnection.nativeDisconnect (static) ──────────────────────────────

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
    objects: &ObjectHeap,
    arrays: &ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = handle_from_obj(args, objects, fields::http_output_stream::HANDLE)?;
    let arr_idx = as_array(args.get(1))?;
    let off = as_int(args.get(2))? as usize;
    let len = as_int(args.get(3))? as usize;

    let conn = conn_mut(handle).ok_or(JvmError::InvalidReference)?;

    // Stream in chunks — we copy from JVM byte[] into a stack buffer, then
    // hand it to the HAL.  Matches the idiom in socket.rs::send_native.
    let mut sent_total = 0usize;
    while sent_total < len {
        let chunk = core::cmp::min(IO_CHUNK, len - sent_total);
        let mut buf = [0u8; IO_CHUNK];
        for i in 0..chunk {
            buf[i] = arrays
                .load(arr_idx, off + sent_total + i)
                .ok_or(JvmError::ArrayIndexOutOfBounds)? as i8 as u8;
        }
        match crate::hal::net::tcp_send(conn.socket, &buf[..chunk]) {
            Ok(0) => return Err(JvmError::InvalidReference),
            Ok(n) => sent_total += n,
            Err(_) => return Err(JvmError::InvalidReference),
        }
    }
    Ok(None)
}

// ── HttpInputStream.read (instance) ──────────────────────────────────────────

pub fn native_input_read(
    args: &[Value],
    objects: &ObjectHeap,
    arrays: &mut ArrayHeap,
) -> Result<Option<Value>, JvmError> {
    let handle = handle_from_obj(args, objects, fields::http_input_stream::HANDLE)?;
    let arr_idx = as_array(args.get(1))?;
    let off = as_int(args.get(2))? as usize;
    let len = as_int(args.get(3))? as usize;

    if len == 0 {
        return Ok(Some(Value::Int(0)));
    }

    let conn = conn_mut(handle).ok_or(JvmError::InvalidReference)?;
    if !conn.headers_parsed {
        parse_response_head(conn)?;
    }
    if conn.body_remaining == 0 {
        return Ok(Some(Value::Int(-1)));
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
        Ok(0) => {
            conn.body_remaining = 0;
            Ok(Some(Value::Int(-1)))
        }
        Ok(n) => {
            for i in 0..n {
                arrays
                    .store(arr_idx, off + i, buf[i] as i8 as i32)
                    .ok_or(JvmError::InvalidReference)?;
            }
            if conn.body_remaining != i64::MAX {
                conn.body_remaining -= n as i64;
            }
            Ok(Some(Value::Int(n as i32)))
        }
        Err(_) => Ok(Some(Value::Int(-1))),
    }
}

// ── plumbing ─────────────────────────────────────────────────────────────────

/// Read from the socket until `\r\n\r\n`, then parse the status line and
/// any `Content-Length` header.  Any bytes past the header terminator are
/// left in `conn.rx_buf[rx_head..rx_tail]` for subsequent body reads.
fn parse_response_head(conn: &mut HttpConn) -> Result<(), JvmError> {
    let mut scan_from = 0usize;
    loop {
        if conn.rx_tail as usize >= RX_BUF_SIZE {
            // Headers too long to fit.
            return Err(JvmError::InvalidReference);
        }
        let space = &mut conn.rx_buf[conn.rx_tail as usize..];
        let n = crate::hal::net::tcp_recv(conn.socket, space)
            .map_err(|_| JvmError::InvalidReference)?;
        if n == 0 {
            return Err(JvmError::InvalidReference);
        }
        conn.rx_tail += n as u16;

        // Scan for CRLFCRLF; keep at most the last 3 bytes of prior scan
        // as context for the boundary.
        let start = scan_from.saturating_sub(3);
        let end = conn.rx_tail as usize;
        if let Some(body_off) = find_header_end(&conn.rx_buf[..end], start) {
            let (status, content_length) = parse_head_bytes(&conn.rx_buf[..body_off])?;
            conn.status_code = status;
            conn.content_length = content_length;
            if content_length >= 0 {
                conn.body_remaining = content_length;
            }
            conn.rx_head = body_off as u16;
            conn.headers_parsed = true;
            return Ok(());
        }
        scan_from = end;
    }
}

fn find_header_end(buf: &[u8], from: usize) -> Option<usize> {
    // Returns the offset of the first byte *after* \r\n\r\n.
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

/// Parse the response head bytes.  Returns `(status_code, content_length)`;
/// `content_length` is -1 if the header was absent.
fn parse_head_bytes(head: &[u8]) -> Result<(i32, i64), JvmError> {
    // head ends with \r\n\r\n — split on \r\n.
    let mut lines = head.split(|&b| b == b'\n');
    // First line: HTTP/1.x SPC CODE SPC REASON
    let status_line = lines.next().ok_or(JvmError::InvalidReference)?;
    let status_line = strip_cr(status_line);
    let status_code = parse_status_code(status_line)?;

    let mut content_length: i64 = -1;
    for line in lines {
        let line = strip_cr(line);
        if line.is_empty() {
            continue;
        }
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

fn strip_cr(line: &[u8]) -> &[u8] {
    if let Some((&b'\r', rest)) = line.split_last().map(|(l, r)| (l, r)) {
        rest
    } else {
        line
    }
}

fn parse_status_code(line: &[u8]) -> Result<i32, JvmError> {
    // "HTTP/1.1 200 OK" — find the first and second space.
    let first_sp = line
        .iter()
        .position(|&b| b == b' ')
        .ok_or(JvmError::InvalidReference)?;
    let rest = &line[first_sp + 1..];
    let second_sp = rest.iter().position(|&b| b == b' ').unwrap_or(rest.len());
    let code = parse_decimal(&rest[..second_sp]);
    if code < 0 {
        return Err(JvmError::InvalidReference);
    }
    Ok(code as i32)
}

/// Case-insensitive match of the `name:` prefix.
fn header_matches(line: &[u8], name: &[u8]) -> bool {
    if line.len() < name.len() + 1 {
        return false;
    }
    for i in 0..name.len() {
        let a = line[i].to_ascii_lowercase();
        if a != name[i] {
            return false;
        }
    }
    line[name.len()] == b':'
}

fn header_value(line: &[u8]) -> Option<&[u8]> {
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

fn parse_decimal(s: &[u8]) -> i64 {
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

fn send_all(sock: *mut c_void, mut buf: &[u8]) -> Result<(), JvmError> {
    while !buf.is_empty() {
        let n = crate::hal::net::tcp_send(sock, buf).map_err(|_| JvmError::InvalidReference)?;
        if n == 0 {
            return Err(JvmError::InvalidReference);
        }
        buf = &buf[n..];
    }
    Ok(())
}

fn write_bytes(buf: &mut [u8], pos: usize, src: &[u8]) -> usize {
    if pos + src.len() > buf.len() {
        return 0;
    }
    buf[pos..pos + src.len()].copy_from_slice(src);
    src.len()
}

fn write_usize(buf: &mut [u8], pos: usize, mut val: usize) -> usize {
    // No leading zero suppression needed — we format a plain decimal.
    if val == 0 {
        if pos < buf.len() {
            buf[pos] = b'0';
            return 1;
        }
        return 0;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while val > 0 {
        tmp[n] = b'0' + (val % 10) as u8;
        val /= 10;
        n += 1;
    }
    if pos + n > buf.len() {
        return 0;
    }
    for i in 0..n {
        buf[pos + i] = tmp[n - 1 - i];
    }
    n
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_200_with_content_length() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 42\r\nConnection: close\r\n\r\n";
        let (status, content_length) = parse_head_bytes(head).unwrap();
        assert_eq!(status, 200);
        assert_eq!(content_length, 42);
    }

    #[test]
    fn parses_404_without_content_length() {
        let head = b"HTTP/1.1 404 Not Found\r\n\r\n";
        let (status, content_length) = parse_head_bytes(head).unwrap();
        assert_eq!(status, 404);
        assert_eq!(content_length, -1);
    }

    #[test]
    fn content_length_is_case_insensitive() {
        let head = b"HTTP/1.1 200 OK\r\ncontent-length: 7\r\n\r\n";
        let (_, content_length) = parse_head_bytes(head).unwrap();
        assert_eq!(content_length, 7);
    }

    #[test]
    fn find_header_end_locates_crlfcrlf() {
        let buf = b"GET /\r\nHost: x\r\n\r\nBODY";
        assert_eq!(find_header_end(buf, 0), Some(buf.len() - 4));
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
}
