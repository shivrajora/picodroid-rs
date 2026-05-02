// SPDX-License-Identifier: GPL-3.0-only
//! Simulator network HAL — wraps host OS sockets (std::net).
//!
//! Provides the same public API as hal::rp::net so that Java Socket
//! native methods work identically on the simulator and real hardware.

use core::ffi::c_void;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

/// Error returned by network operations.
#[derive(Debug, Clone, Copy)]
pub struct NetError(pub i32);

/// Tagged union wrapping the different std::net socket types behind
/// a single `*mut c_void` pointer for the HAL API.
enum SimSocket {
    TcpClient(Option<TcpStream>),
    TcpListener(TcpListener),
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

// ── TCP ──────────────────────────────────────────────────────────────────────

pub fn tcp_socket() -> Result<*mut c_void, NetError> {
    Ok(box_socket(SimSocket::TcpClient(None)))
}

pub fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(ref mut opt) => {
            let ip = u32_to_ipv4(addr);
            let stream =
                TcpStream::connect(SocketAddrV4::new(ip, port)).map_err(|_| NetError(-1))?;
            *opt = Some(stream);
            Ok(())
        }
        _ => Err(NetError(-1)),
    }
}

pub fn tcp_send(sock: *mut c_void, buf: &[u8]) -> Result<usize, NetError> {
    use std::io::Write;
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(Some(ref mut stream)) => stream.write(buf).map_err(|_| NetError(-1)),
        _ => Err(NetError(-1)),
    }
}

pub fn tcp_recv(sock: *mut c_void, buf: &mut [u8]) -> Result<usize, NetError> {
    use std::io::Read;
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(Some(ref mut stream)) => stream.read(buf).map_err(|_| NetError(-1)),
        _ => Err(NetError(-1)),
    }
}

pub fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpClient(_) => {
            let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))
                .map_err(|_| NetError(-1))?;
            *s = SimSocket::TcpListener(listener);
            Ok(())
        }
        _ => Err(NetError(-1)),
    }
}

pub fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::TcpListener(ref listener) => {
            let (stream, _addr) = listener.accept().map_err(|_| NetError(-1))?;
            Ok(box_socket(SimSocket::TcpClient(Some(stream))))
        }
        _ => Err(NetError(-1)),
    }
}

// ── UDP ──────────────────────────────────────────────────────────────────────

pub fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError> {
    let sock = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, local_port))
        .map_err(|_| NetError(-1))?;
    Ok(box_socket(SimSocket::Udp(sock)))
}

pub fn udp_sendto(sock: *mut c_void, buf: &[u8], addr: u32, port: u16) -> Result<usize, NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(ref udp) => {
            let ip = u32_to_ipv4(addr);
            udp.send_to(buf, SocketAddrV4::new(ip, port))
                .map_err(|_| NetError(-1))
        }
        _ => Err(NetError(-1)),
    }
}

pub fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(ref udp) => {
            let (n, src) = udp.recv_from(buf).map_err(|_| NetError(-1))?;
            match src {
                std::net::SocketAddr::V4(v4) => {
                    let octets = v4.ip().octets();
                    let addr = ((octets[0] as u32) << 24)
                        | ((octets[1] as u32) << 16)
                        | ((octets[2] as u32) << 8)
                        | (octets[3] as u32);
                    Ok((n, addr, v4.port()))
                }
                _ => Err(NetError(-1)),
            }
        }
        _ => Err(NetError(-1)),
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
/// handles either.  Returns the first IPv4 result; any IPv6-only
/// hostname yields `NetError(-1)`.
pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
    // Port 0 is fine — we only care about the address.
    let addrs = (hostname, 0u16)
        .to_socket_addrs()
        .map_err(|_| NetError(-1))?;
    for a in addrs {
        if let std::net::SocketAddr::V4(v4) = a {
            let o = v4.ip().octets();
            return Ok(((o[0] as u32) << 24)
                | ((o[1] as u32) << 16)
                | ((o[2] as u32) << 8)
                | (o[3] as u32));
        }
    }
    Err(NetError(-1))
}
