// SPDX-License-Identifier: GPL-3.0-only
//! Simulator network HAL — wraps host OS sockets (std::net).
//!
//! Provides the same public API as hal::rp::net so that Java Socket
//! native methods work identically on the simulator and real hardware.

// Sockets are opaque handles: shared net code receives one from `tcp_socket`
// and hands it straight back, never dereferencing it. The pointer is only
// ever resolved here, where it was created. Marking these `unsafe` would push
// the keyword onto every caller for a contract they cannot violate — the same
// reasoning `hal::facade::net` carries for the declarations these implement.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub use crate::hal::types::{NetError, NetErrorKind};

/// Classify a host `std::io::Error` into the shared semantic kind.
///
/// `raw` keeps the positive host errno for the `(err N)` message suffix;
/// `-1` when the error carries no OS code (e.g. a pure timeout synthesized
/// by std).
fn map_io_err(e: std::io::Error) -> NetError {
    use std::io::ErrorKind as K;
    let kind = match e.kind() {
        K::ConnectionRefused => NetErrorKind::Refused,
        // Blocking reads past an SO_RCVTIMEO-style `set_read_timeout` fail
        // with WouldBlock on Unix and TimedOut on Windows — both are the
        // deadline expiring, never spurious non-blocking here (the sim only
        // uses blocking sockets).
        K::TimedOut | K::WouldBlock => NetErrorKind::TimedOut,
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::NotConnected => {
            NetErrorKind::Closed
        }
        K::AddrInUse => NetErrorKind::AddrInUse,
        K::HostUnreachable | K::NetworkUnreachable => NetErrorKind::Unreachable,
        _ => NetErrorKind::Other,
    };
    NetError::new(kind, e.raw_os_error().unwrap_or(-1))
}

/// A handle that is not (or no longer) the kind of socket the operation
/// needs — closed, or the wrong variant.
const fn bad_handle() -> NetError {
    NetError::new(NetErrorKind::Closed, -1)
}

/// Tagged union wrapping the different std::net socket types behind
/// a single `*mut c_void` pointer for the HAL API.
///
/// The listener carries its accept timeout: `std::net::TcpListener` has no
/// native accept deadline (unlike read timeouts on streams), so
/// [`tcp_accept`] emulates one with a nonblocking poll loop.
enum SimSocket {
    TcpClient(Option<TcpStream>),
    TcpListener(TcpListener, Option<Duration>),
    Udp(UdpSocket),
}

/// Box a SimSocket and leak it as a raw pointer.
fn box_socket(s: SimSocket) -> *mut c_void {
    Box::into_raw(Box::new(s)) as *mut c_void
}

/// Recover a &mut SimSocket from a raw pointer.
///
/// # Safety
/// `ptr` must have been created by `box_socket` and not yet freed.
unsafe fn deref_socket(ptr: *mut c_void) -> &'static mut SimSocket {
    &mut *(ptr as *mut SimSocket)
}

/// Convert a packed u32 IPv4 address to std Ipv4Addr.
/// The u32 is in host byte order: MSB = first octet.
fn u32_to_ipv4(addr: u32) -> Ipv4Addr {
    Ipv4Addr::from(addr.to_be_bytes())
}

/// Run a blocking std::net call, retrying on EINTR.
///
/// The sim scheduler drives FreeRTOS ticks with SIGALRM; a signal landing
/// while a socket call blocks makes it fail with ErrorKind::Interrupted,
/// which the device HAL has no equivalent for — retry instead of
/// surfacing a spurious error to Java.
fn retry_eintr<T>(mut f: impl FnMut() -> std::io::Result<T>) -> Result<T, NetError> {
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(map_io_err(e)),
        }
    }
}

// ── TCP ──────────────────────────────────────────────────────────────────────

pub fn tcp_socket() -> Result<*mut c_void, NetError> {
    Ok(box_socket(SimSocket::TcpClient(None)))
}

pub fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(ref mut opt) => {
            let ip = u32_to_ipv4(addr);
            let stream = retry_eintr(|| TcpStream::connect(SocketAddrV4::new(ip, port)))?;
            *opt = Some(stream);
            Ok(())
        }
        _ => Err(bad_handle()),
    }
}

pub fn tcp_send(sock: *mut c_void, buf: &[u8]) -> Result<usize, NetError> {
    use std::io::Write;
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(Some(ref mut stream)) => retry_eintr(|| stream.write(buf)),
        _ => Err(bad_handle()),
    }
}

/// Blocking receive. `Ok(0)` means orderly EOF — a read-timeout expiry is
/// `Err(TimedOut)`, matching the normalized `tcp_recv` contract the device
/// HAL implements by remapping FreeRTOS's inverted encoding.
///
/// With a timeout configured, the naive EINTR-retry loop can never expire:
/// the sim scheduler's SIGALRM ticks land every millisecond, and each retry
/// restarts the kernel's SO_RCVTIMEO window from scratch. Track a deadline
/// and shrink the window across retries instead.
pub fn tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError> {
    use std::io::Read;
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(Some(ref mut stream)) => {
            let configured = stream.read_timeout().ok().flatten();
            let Some(window) = configured else {
                // No timeout: an EINTR retry loop is exactly right.
                return retry_eintr(|| stream.read(buf));
            };
            let deadline = std::time::Instant::now() + window;
            let result = loop {
                match stream.read(buf) {
                    Ok(v) => break Ok(v),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        let now = std::time::Instant::now();
                        let remaining = deadline.saturating_duration_since(now);
                        // set_read_timeout rejects a zero Duration, and a
                        // sub-millisecond window would just EINTR again.
                        if remaining < Duration::from_millis(1) {
                            break Err(NetError::new(NetErrorKind::TimedOut, -1));
                        }
                        let _ = stream.set_read_timeout(Some(remaining));
                    }
                    // WouldBlock/TimedOut only occur at window expiry →
                    // map_io_err turns them into TimedOut.
                    Err(e) => break Err(map_io_err(e)),
                }
            };
            // Restore the configured value for the next call.
            let _ = stream.set_read_timeout(Some(window));
            result
        }
        _ => Err(bad_handle()),
    }
}

pub fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(_) => {
            let listener =
                retry_eintr(|| TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)))?;
            *s = SimSocket::TcpListener(listener, None);
            Ok(())
        }
        _ => Err(bad_handle()),
    }
}

pub fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpListener(ref listener, None) => {
            let (stream, _addr) = retry_eintr(|| listener.accept())?;
            Ok(box_socket(SimSocket::TcpClient(Some(stream))))
        }
        // With a timeout set, emulate an accept deadline by polling a
        // nonblocking listener — WouldBlock here means "nothing pending
        // yet", never the deadline itself, so it must not go through
        // `map_io_err` (which reads WouldBlock as TimedOut).
        SimSocket::TcpListener(ref listener, Some(dur)) => {
            let deadline = std::time::Instant::now() + *dur;
            listener.set_nonblocking(true).map_err(map_io_err)?;
            let result = loop {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let _ = stream.set_nonblocking(false);
                        break Ok(box_socket(SimSocket::TcpClient(Some(stream))));
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        if std::time::Instant::now() >= deadline {
                            break Err(NetError::new(NetErrorKind::TimedOut, -1));
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => break Err(map_io_err(e)),
                }
            };
            let _ = listener.set_nonblocking(false);
            result
        }
        _ => Err(bad_handle()),
    }
}

// ── UDP ──────────────────────────────────────────────────────────────────────

pub fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError> {
    let sock =
        retry_eintr(|| UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, local_port)))?;
    Ok(box_socket(SimSocket::Udp(sock)))
}

pub fn udp_sendto(sock: *mut c_void, buf: &[u8], addr: u32, port: u16) -> Result<usize, NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(ref udp) => {
            let ip = u32_to_ipv4(addr);
            retry_eintr(|| udp.send_to(buf, SocketAddrV4::new(ip, port)))
        }
        _ => Err(bad_handle()),
    }
}

pub fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(ref udp) => {
            // Same deadline discipline as tcp_recv: the sim's SIGALRM ticks
            // EINTR a blocking recv every millisecond, and a naive retry
            // restarts SO_RCVTIMEO from scratch, so a timeout never expires.
            let (n, src) = if let Some(window) = udp.read_timeout().ok().flatten() {
                let deadline = std::time::Instant::now() + window;
                let result = loop {
                    match udp.recv_from(buf) {
                        Ok(v) => break Ok(v),
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                            let now = std::time::Instant::now();
                            let remaining = deadline.saturating_duration_since(now);
                            if remaining < Duration::from_millis(1) {
                                break Err(NetError::new(NetErrorKind::TimedOut, -1));
                            }
                            let _ = udp.set_read_timeout(Some(remaining));
                        }
                        Err(e) => break Err(map_io_err(e)),
                    }
                };
                let _ = udp.set_read_timeout(Some(window));
                result?
            } else {
                retry_eintr(|| udp.recv_from(buf))?
            };
            match src {
                std::net::SocketAddr::V4(v4) => {
                    let octets = v4.ip().octets();
                    let addr = ((octets[0] as u32) << 24)
                        | ((octets[1] as u32) << 16)
                        | ((octets[2] as u32) << 8)
                        | (octets[3] as u32);
                    Ok((n, addr, v4.port()))
                }
                _ => Err(NetError::other(-1)),
            }
        }
        _ => Err(bad_handle()),
    }
}

// ── Common ───────────────────────────────────────────────────────────────────

pub fn close(sock: *mut c_void) {
    if sock.is_null() {
        return;
    }
    // Reconstitute the Box and drop it, closing the socket.
    unsafe {
        let _ = Box::from_raw(sock as *mut SimSocket);
    }
}

pub fn set_recv_timeout(sock: *mut c_void, timeout_ms: u32) {
    let s = unsafe { deref_socket(sock) };
    let dur = if timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };
    match s {
        SimSocket::TcpClient(Some(ref stream)) => {
            let _ = stream.set_read_timeout(dur);
        }
        SimSocket::Udp(ref udp) => {
            let _ = udp.set_read_timeout(dur);
        }
        SimSocket::TcpListener(_, ref mut timeout) => {
            *timeout = dur;
        }
        _ => {}
    }
}

/// Check if the network stack is up (always true in sim).
pub fn is_network_up() -> bool {
    true
}

/// Get the assigned IP address (127.0.0.1 in sim).
pub fn get_ip_address() -> u32 {
    // 127.0.0.1 in host byte order (MSB = first octet)
    0x7F000001
}

/// Resolve a hostname to a packed IPv4 address (MSB = first octet).
///
/// Accepts both DNS names and dotted-quad literals — `ToSocketAddrs`
/// handles either.  Returns the first IPv4 result; failure to resolve
/// (including an IPv6-only hostname) is `HostLookup`.
pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
    // Port 0 is fine — we only care about the address.
    let addrs = (hostname, 0u16)
        .to_socket_addrs()
        .map_err(|e| NetError::new(NetErrorKind::HostLookup, e.raw_os_error().unwrap_or(-1)))?;
    for a in addrs {
        if let std::net::SocketAddr::V4(v4) = a {
            let o = v4.ip().octets();
            return Ok(((o[0] as u32) << 24)
                | ((o[1] as u32) << 16)
                | ((o[2] as u32) << 8)
                | (o[3] as u32));
        }
    }
    Err(NetError::new(NetErrorKind::HostLookup, -1))
}
