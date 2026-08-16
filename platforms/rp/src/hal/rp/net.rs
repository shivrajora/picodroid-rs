// SPDX-License-Identifier: GPL-3.0-only
//! Network HAL — FreeRTOS+TCP socket wrappers.
//!
//! Thin Rust wrappers around the FreeRTOS+TCP C socket API.
//! These provide the platform-specific implementation called by the
//! Java Socket/ServerSocket/DatagramSocket native methods.

pub use picodroid_core::hal::types::{NetError, NetErrorKind};

// FreeRTOS+TCP returns negated `pdFREERTOS_ERRNO_*` codes
// (`third_party/FreeRTOS-Kernel/include/projdefs.h`). The ones this module
// classifies; everything else passes through as `NetErrorKind::Other` with
// the raw code preserved for the `(err N)` message suffix.
const ERR_EWOULDBLOCK: i32 = -11;
const ERR_EADDRINUSE: i32 = -112;
const ERR_ETIMEDOUT: i32 = -116;
const ERR_ENOTCONN: i32 = -128;

// FreeRTOS+TCP socket FFI (hand-written bindings).
extern "C" {
    fn FreeRTOS_socket(domain: i32, r#type: i32, protocol: i32) -> *mut core::ffi::c_void;

    fn FreeRTOS_bind(
        socket: *mut core::ffi::c_void,
        addr: *const FreertosSocketAddr,
        addr_len: u32,
    ) -> i32;

    fn FreeRTOS_connect(
        socket: *mut core::ffi::c_void,
        addr: *const FreertosSocketAddr,
        addr_len: u32,
    ) -> i32;

    fn FreeRTOS_listen(socket: *mut core::ffi::c_void, backlog: i32) -> i32;

    fn FreeRTOS_accept(
        socket: *mut core::ffi::c_void,
        addr: *mut FreertosSocketAddr,
        addr_len: *mut u32,
    ) -> *mut core::ffi::c_void;

    fn FreeRTOS_recv(socket: *mut core::ffi::c_void, buf: *mut u8, len: usize, flags: i32) -> i32;

    fn FreeRTOS_send(socket: *mut core::ffi::c_void, buf: *const u8, len: usize, flags: i32)
        -> i32;

    fn FreeRTOS_recvfrom(
        socket: *mut core::ffi::c_void,
        buf: *mut u8,
        len: usize,
        flags: i32,
        addr: *mut FreertosSocketAddr,
        addr_len: *mut u32,
    ) -> i32;

    fn FreeRTOS_sendto(
        socket: *mut core::ffi::c_void,
        buf: *const u8,
        len: usize,
        flags: i32,
        addr: *const FreertosSocketAddr,
        addr_len: u32,
    ) -> i32;

    fn FreeRTOS_closesocket(socket: *mut core::ffi::c_void) -> i32;

    fn FreeRTOS_setsockopt(
        socket: *mut core::ffi::c_void,
        level: i32,
        option: i32,
        value: *const core::ffi::c_void,
        option_len: u32,
    ) -> i32;

    fn FreeRTOS_GetIPAddress() -> u32;
    fn FreeRTOS_IsNetworkUp() -> i32;

    fn FreeRTOS_gethostbyname(pc_host_name: *const u8) -> u32;

    /// FreeRTOS kernel tick count — 1 ms ticks (configTICK_RATE_HZ=1000).
    /// Used to classify connect failures by elapsed time (see tcp_connect).
    fn xTaskGetTickCount() -> u32;
}

/// FreeRTOS+TCP socket address, mirroring V4's `struct freertos_sockaddr`:
/// `sin_address` is an `IP_Address_t` union (IPv4 u32 / IPv6 16 bytes) at
/// offset 8 — the union keeps its 16-byte size even with IPv6 compiled
/// out, so the struct is 24 bytes and the stack writes that much back in
/// accept/recvfrom.  An 8-byte mirror here means stack corruption.
#[repr(C)]
struct FreertosSocketAddr {
    sin_len: u8,
    sin_family: u8,
    sin_port: u16,
    sin_flowinfo: u32,
    sin_addr: u32,
    _sin_addr_pad: [u8; 12],
}

impl FreertosSocketAddr {
    const SIZE: u32 = core::mem::size_of::<FreertosSocketAddr>() as u32;

    /// Build an IPv4 address record.  `addr` uses the picodroid HAL
    /// convention (MSB = first octet); FreeRTOS+TCP stores IPv4 in
    /// network byte order within a little-endian u32, so swap here.
    fn ipv4(addr: u32, port: u16) -> Self {
        FreertosSocketAddr {
            sin_len: Self::SIZE as u8,
            sin_family: FREERTOS_AF_INET as u8,
            sin_port: htons(port),
            sin_flowinfo: 0,
            sin_addr: addr.swap_bytes(),
            _sin_addr_pad: [0; 12],
        }
    }

    fn zeroed() -> Self {
        FreertosSocketAddr {
            sin_len: 0,
            sin_family: 0,
            sin_port: 0,
            sin_flowinfo: 0,
            sin_addr: 0,
            _sin_addr_pad: [0; 12],
        }
    }
}

const _: () = assert!(core::mem::size_of::<FreertosSocketAddr>() == 24);

// Protocol family / type constants (match FreeRTOS+TCP definitions).
const FREERTOS_AF_INET: i32 = 2;
const FREERTOS_SOCK_STREAM: i32 = 1;
const FREERTOS_SOCK_DGRAM: i32 = 2;
const FREERTOS_IPPROTO_TCP: i32 = 6;
const FREERTOS_IPPROTO_UDP: i32 = 17;
const FREERTOS_SO_RCVTIMEO: i32 = 0;

/// Invalid socket sentinel — FreeRTOS+TCP's FREERTOS_INVALID_SOCKET is
/// `(Socket_t)~0U`, NOT null.  (accept additionally yields null on
/// timeout, so both must be treated as failure.)
const FREERTOS_INVALID_SOCKET: *mut core::ffi::c_void = usize::MAX as *mut core::ffi::c_void;

/// True for both failure encodings a socket-returning call can produce.
fn socket_invalid(sock: *mut core::ffi::c_void) -> bool {
    sock == FREERTOS_INVALID_SOCKET || sock.is_null()
}

/// Swap bytes for network byte order (big-endian) port number.
fn htons(val: u16) -> u16 {
    val.to_be()
}

/// Create a TCP socket.  Returns a handle (pointer cast to i32 via handle table).
pub fn tcp_socket() -> Result<*mut core::ffi::c_void, NetError> {
    let sock =
        unsafe { FreeRTOS_socket(FREERTOS_AF_INET, FREERTOS_SOCK_STREAM, FREERTOS_IPPROTO_TCP) };
    if socket_invalid(sock) {
        return Err(NetError::other(-1));
    }
    Ok(sock)
}

/// Connect a TCP socket to a remote address.
pub fn tcp_connect(sock: *mut core::ffi::c_void, addr: u32, port: u16) -> Result<(), NetError> {
    let sa = FreertosSocketAddr::ipv4(addr, port);
    let t0 = unsafe { xTaskGetTickCount() };
    let ret = unsafe { FreeRTOS_connect(sock, &sa, FreertosSocketAddr::SIZE) };
    if ret != 0 {
        let elapsed_ms = unsafe { xTaskGetTickCount() }.wrapping_sub(t0);
        // Every aborted connect returns -ENOTCONN (HW-verified 2026-08-15):
        // peer RST (via the vendored fork's wake fix), ARP-resolution
        // give-up, and SYN-retransmission exhaustion all converge on
        // eCLOSE_WAIT, and -ETIMEDOUT appears only when a finite socket
        // block time expires first. The stack's own timing ladder keeps the
        // three causes far apart, so classify by elapsed time:
        //  - RST refusal: one RTT, milliseconds on a LAN;
        //  - ARP give-up: 3 resolution polls at 500 ms
        //    (prvTCPPrepareConnect_IPV4 bumps ucRepCount per miss) ≈ 1.5 s;
        //  - SYN exhaustion: retries at +3 s/+6 s, give-up check ≥ 9 s in.
        let kind = match ret {
            ERR_ENOTCONN if elapsed_ms < 1_000 => NetErrorKind::Refused,
            ERR_ENOTCONN if elapsed_ms <= 6_000 => NetErrorKind::Unreachable,
            ERR_ENOTCONN | ERR_ETIMEDOUT => NetErrorKind::TimedOut,
            _ => NetErrorKind::Other,
        };
        return Err(NetError::new(kind, ret));
    }
    Ok(())
}

/// Receive data from a TCP socket (blocking).
///
/// FreeRTOS+TCP's encoding is inverted relative to the HAL contract:
/// `FreeRTOS_recv` returns `0` on SO_RCVTIMEO expiry and `-ENOTCONN` on
/// peer close. Normalize here so shared code sees `Ok(0)` = orderly EOF
/// and `Err(TimedOut)` = timeout on every platform. (A hard RST is also
/// `-ENOTCONN` and thus reads as EOF — Android apps treat both as
/// end-of-stream in practice, accept the approximation.)
pub fn tcp_recv(sock: *mut core::ffi::c_void, buf: &mut [u8]) -> Result<usize, NetError> {
    let ret = unsafe { FreeRTOS_recv(sock, buf.as_mut_ptr(), buf.len(), 0) };
    match ret {
        0 => Err(NetError::new(NetErrorKind::TimedOut, 0)),
        ERR_ENOTCONN => Ok(0),
        n if n < 0 => Err(NetError::other(n)),
        n => Ok(n as usize),
    }
}

/// Send data on a TCP socket.
pub fn tcp_send(sock: *mut core::ffi::c_void, buf: &[u8]) -> Result<usize, NetError> {
    let ret = unsafe { FreeRTOS_send(sock, buf.as_ptr(), buf.len(), 0) };
    if ret < 0 {
        let kind = match ret {
            ERR_ENOTCONN => NetErrorKind::Closed,
            _ => NetErrorKind::Other,
        };
        return Err(NetError::new(kind, ret));
    }
    Ok(ret as usize)
}

/// Close a socket.
pub fn close(sock: *mut core::ffi::c_void) {
    unsafe {
        FreeRTOS_closesocket(sock);
    }
}

/// Set receive timeout (milliseconds).
pub fn set_recv_timeout(sock: *mut core::ffi::c_void, timeout_ms: u32) {
    let ticks = timeout_ms; // FreeRTOS+TCP expects ticks, 1 tick = 1 ms at 1000 Hz
    unsafe {
        FreeRTOS_setsockopt(
            sock,
            0,
            FREERTOS_SO_RCVTIMEO,
            &ticks as *const u32 as *const core::ffi::c_void,
            core::mem::size_of::<u32>() as u32,
        );
    }
}

/// Create a UDP socket bound to a local port.
pub fn udp_socket(local_port: u16) -> Result<*mut core::ffi::c_void, NetError> {
    let sock =
        unsafe { FreeRTOS_socket(FREERTOS_AF_INET, FREERTOS_SOCK_DGRAM, FREERTOS_IPPROTO_UDP) };
    if socket_invalid(sock) {
        return Err(NetError::other(-1));
    }
    let sa = FreertosSocketAddr::ipv4(0, local_port); // INADDR_ANY
    let ret = unsafe { FreeRTOS_bind(sock, &sa, FreertosSocketAddr::SIZE) };
    if ret != 0 {
        unsafe { FreeRTOS_closesocket(sock) };
        return Err(NetError::new(bind_kind(ret), ret));
    }
    Ok(sock)
}

/// Classify a `FreeRTOS_bind` failure.
fn bind_kind(ret: i32) -> NetErrorKind {
    match ret {
        ERR_EADDRINUSE => NetErrorKind::AddrInUse,
        _ => NetErrorKind::Other,
    }
}

/// Send a UDP datagram.
pub fn udp_sendto(
    sock: *mut core::ffi::c_void,
    buf: &[u8],
    addr: u32,
    port: u16,
) -> Result<usize, NetError> {
    let sa = FreertosSocketAddr::ipv4(addr, port);
    let ret = unsafe {
        FreeRTOS_sendto(
            sock,
            buf.as_ptr(),
            buf.len(),
            0,
            &sa,
            FreertosSocketAddr::SIZE,
        )
    };
    if ret < 0 {
        return Err(NetError::other(ret));
    }
    Ok(ret as usize)
}

/// Receive a UDP datagram (blocking).  Returns (bytes_read, source_addr, source_port).
pub fn udp_recvfrom(
    sock: *mut core::ffi::c_void,
    buf: &mut [u8],
) -> Result<(usize, u32, u16), NetError> {
    let mut sa = FreertosSocketAddr::zeroed();
    let mut sa_len = FreertosSocketAddr::SIZE;
    let ret =
        unsafe { FreeRTOS_recvfrom(sock, buf.as_mut_ptr(), buf.len(), 0, &mut sa, &mut sa_len) };
    if ret < 0 {
        // -EWOULDBLOCK is how FreeRTOS_recvfrom reports SO_RCVTIMEO expiry.
        let kind = match ret {
            ERR_EWOULDBLOCK => NetErrorKind::TimedOut,
            _ => NetErrorKind::Other,
        };
        return Err(NetError::new(kind, ret));
    }
    Ok((
        ret as usize,
        sa.sin_addr.swap_bytes(),
        u16::from_be(sa.sin_port),
    ))
}

/// Bind a TCP socket to a local port and start listening.
pub fn tcp_listen(sock: *mut core::ffi::c_void, port: u16) -> Result<(), NetError> {
    let sa = FreertosSocketAddr::ipv4(0, port); // INADDR_ANY
    let ret = unsafe { FreeRTOS_bind(sock, &sa, FreertosSocketAddr::SIZE) };
    if ret != 0 {
        return Err(NetError::new(bind_kind(ret), ret));
    }
    let ret = unsafe { FreeRTOS_listen(sock, 1) };
    if ret != 0 {
        return Err(NetError::other(ret));
    }
    Ok(())
}

/// Accept an incoming TCP connection (blocking).
///
/// `FreeRTOS_accept` returns NULL for both SO_RCVTIMEO expiry and hard
/// failure with no way to tell them apart; once a timeout is set, expiry
/// is the overwhelmingly common case, so classify NULL as `TimedOut`.
pub fn tcp_accept(sock: *mut core::ffi::c_void) -> Result<*mut core::ffi::c_void, NetError> {
    let mut sa = FreertosSocketAddr::zeroed();
    let mut sa_len = FreertosSocketAddr::SIZE;
    let client = unsafe { FreeRTOS_accept(sock, &mut sa, &mut sa_len) };
    if socket_invalid(client) {
        return Err(NetError::new(NetErrorKind::TimedOut, -1));
    }
    Ok(client)
}

/// Check if the FreeRTOS+TCP network stack is up.
///
/// `FreeRTOS_IsNetworkUp()` is vacuously true before any endpoint is
/// registered (empty endpoint list), so also require a nonzero assigned
/// IP — the stack is only usable once DHCP (or static config) has
/// produced an address.
pub fn is_network_up() -> bool {
    unsafe { FreeRTOS_IsNetworkUp() != 0 && FreeRTOS_GetIPAddress() != 0 }
}

/// Get the assigned IP address (from DHCP or static config), in the
/// picodroid HAL convention (MSB = first octet) — matching the sim HAL
/// and `picodroid.net.InetAddress`.
pub fn get_ip_address() -> u32 {
    unsafe { FreeRTOS_GetIPAddress() }.swap_bytes()
}

/// Resolve a hostname to a packed IPv4 address in the picodroid HAL
/// convention (MSB = first octet), suitable for `tcp_connect`.  If the
/// name is already a dotted-quad literal, FreeRTOS_gethostbyname returns
/// it without hitting the network.  On failure the upstream returns 0.
pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
    // FreeRTOS_gethostbyname requires a NUL-terminated C string.
    let mut cbuf = [0u8; 256];
    let bytes = hostname.as_bytes();
    if bytes.len() >= cbuf.len() {
        return Err(NetError::new(NetErrorKind::HostLookup, -1));
    }
    cbuf[..bytes.len()].copy_from_slice(bytes);
    // cbuf[bytes.len()] is already 0.
    let addr = unsafe { FreeRTOS_gethostbyname(cbuf.as_ptr()) };
    if addr == 0 {
        return Err(NetError::new(NetErrorKind::HostLookup, 0));
    }
    Ok(addr.swap_bytes())
}
