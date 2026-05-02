// SPDX-License-Identifier: GPL-3.0-only
//! Network HAL — FreeRTOS+TCP socket wrappers.
//!
//! Thin Rust wrappers around the FreeRTOS+TCP C socket API.
//! These provide the platform-specific implementation called by the
//! Java Socket/ServerSocket/DatagramSocket native methods.

/// Error returned by network operations.
#[derive(Debug, Clone, Copy)]
pub struct NetError(pub i32);

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
}

/// FreeRTOS+TCP socket address (IPv4).
#[repr(C)]
struct FreertosSocketAddr {
    sin_len: u8,
    sin_family: u8,
    sin_port: u16,
    sin_addr: u32,
}

// Protocol family / type constants (match FreeRTOS+TCP definitions).
const FREERTOS_AF_INET: i32 = 2;
const FREERTOS_SOCK_STREAM: i32 = 1;
const FREERTOS_SOCK_DGRAM: i32 = 2;
const FREERTOS_IPPROTO_TCP: i32 = 6;
const FREERTOS_IPPROTO_UDP: i32 = 17;
const FREERTOS_SO_RCVTIMEO: i32 = 0;

/// Invalid socket sentinel.
const FREERTOS_INVALID_SOCKET: *mut core::ffi::c_void = core::ptr::null_mut();

/// Swap bytes for network byte order (big-endian) port number.
fn htons(val: u16) -> u16 {
    val.to_be()
}

/// Create a TCP socket.  Returns a handle (pointer cast to i32 via handle table).
pub fn tcp_socket() -> Result<*mut core::ffi::c_void, NetError> {
    let sock =
        unsafe { FreeRTOS_socket(FREERTOS_AF_INET, FREERTOS_SOCK_STREAM, FREERTOS_IPPROTO_TCP) };
    if sock == FREERTOS_INVALID_SOCKET {
        return Err(NetError(-1));
    }
    Ok(sock)
}

/// Connect a TCP socket to a remote address.
pub fn tcp_connect(sock: *mut core::ffi::c_void, addr: u32, port: u16) -> Result<(), NetError> {
    let sa = FreertosSocketAddr {
        sin_len: core::mem::size_of::<FreertosSocketAddr>() as u8,
        sin_family: FREERTOS_AF_INET as u8,
        sin_port: htons(port),
        sin_addr: addr,
    };
    let ret =
        unsafe { FreeRTOS_connect(sock, &sa, core::mem::size_of::<FreertosSocketAddr>() as u32) };
    if ret != 0 {
        return Err(NetError(ret));
    }
    Ok(())
}

/// Receive data from a TCP socket (blocking).
pub fn tcp_recv(sock: *mut core::ffi::c_void, buf: &mut [u8]) -> Result<usize, NetError> {
    let ret = unsafe { FreeRTOS_recv(sock, buf.as_mut_ptr(), buf.len(), 0) };
    if ret < 0 {
        return Err(NetError(ret));
    }
    Ok(ret as usize)
}

/// Send data on a TCP socket.
pub fn tcp_send(sock: *mut core::ffi::c_void, buf: &[u8]) -> Result<usize, NetError> {
    let ret = unsafe { FreeRTOS_send(sock, buf.as_ptr(), buf.len(), 0) };
    if ret < 0 {
        return Err(NetError(ret));
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
    if sock == FREERTOS_INVALID_SOCKET {
        return Err(NetError(-1));
    }
    let sa = FreertosSocketAddr {
        sin_len: core::mem::size_of::<FreertosSocketAddr>() as u8,
        sin_family: FREERTOS_AF_INET as u8,
        sin_port: htons(local_port),
        sin_addr: 0, // INADDR_ANY
    };
    let ret =
        unsafe { FreeRTOS_bind(sock, &sa, core::mem::size_of::<FreertosSocketAddr>() as u32) };
    if ret != 0 {
        unsafe { FreeRTOS_closesocket(sock) };
        return Err(NetError(ret));
    }
    Ok(sock)
}

/// Send a UDP datagram.
pub fn udp_sendto(
    sock: *mut core::ffi::c_void,
    buf: &[u8],
    addr: u32,
    port: u16,
) -> Result<usize, NetError> {
    let sa = FreertosSocketAddr {
        sin_len: core::mem::size_of::<FreertosSocketAddr>() as u8,
        sin_family: FREERTOS_AF_INET as u8,
        sin_port: htons(port),
        sin_addr: addr,
    };
    let ret = unsafe {
        FreeRTOS_sendto(
            sock,
            buf.as_ptr(),
            buf.len(),
            0,
            &sa,
            core::mem::size_of::<FreertosSocketAddr>() as u32,
        )
    };
    if ret < 0 {
        return Err(NetError(ret));
    }
    Ok(ret as usize)
}

/// Receive a UDP datagram (blocking).  Returns (bytes_read, source_addr, source_port).
pub fn udp_recvfrom(
    sock: *mut core::ffi::c_void,
    buf: &mut [u8],
) -> Result<(usize, u32, u16), NetError> {
    let mut sa = FreertosSocketAddr {
        sin_len: 0,
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
    };
    let mut sa_len = core::mem::size_of::<FreertosSocketAddr>() as u32;
    let ret =
        unsafe { FreeRTOS_recvfrom(sock, buf.as_mut_ptr(), buf.len(), 0, &mut sa, &mut sa_len) };
    if ret < 0 {
        return Err(NetError(ret));
    }
    Ok((ret as usize, sa.sin_addr, u16::from_be(sa.sin_port)))
}

/// Bind a TCP socket to a local port and start listening.
pub fn tcp_listen(sock: *mut core::ffi::c_void, port: u16) -> Result<(), NetError> {
    let sa = FreertosSocketAddr {
        sin_len: core::mem::size_of::<FreertosSocketAddr>() as u8,
        sin_family: FREERTOS_AF_INET as u8,
        sin_port: htons(port),
        sin_addr: 0, // INADDR_ANY
    };
    let ret =
        unsafe { FreeRTOS_bind(sock, &sa, core::mem::size_of::<FreertosSocketAddr>() as u32) };
    if ret != 0 {
        return Err(NetError(ret));
    }
    let ret = unsafe { FreeRTOS_listen(sock, 1) };
    if ret != 0 {
        return Err(NetError(ret));
    }
    Ok(())
}

/// Accept an incoming TCP connection (blocking).
pub fn tcp_accept(sock: *mut core::ffi::c_void) -> Result<*mut core::ffi::c_void, NetError> {
    let mut sa = FreertosSocketAddr {
        sin_len: 0,
        sin_family: 0,
        sin_port: 0,
        sin_addr: 0,
    };
    let mut sa_len = core::mem::size_of::<FreertosSocketAddr>() as u32;
    let client = unsafe { FreeRTOS_accept(sock, &mut sa, &mut sa_len) };
    if client == FREERTOS_INVALID_SOCKET {
        return Err(NetError(-1));
    }
    Ok(client)
}

/// Check if the FreeRTOS+TCP network stack is up.
pub fn is_network_up() -> bool {
    unsafe { FreeRTOS_IsNetworkUp() != 0 }
}

/// Get the assigned IP address (from DHCP or static config).
pub fn get_ip_address() -> u32 {
    unsafe { FreeRTOS_GetIPAddress() }
}

/// Resolve a hostname to a packed IPv4 address.
///
/// Returns the same packed u32 form FreeRTOS+TCP uses for `sin_addr`
/// (so it can be passed straight to `tcp_connect`).  If the name is
/// already a dotted-quad literal, FreeRTOS_gethostbyname returns it
/// without hitting the network.  On failure the upstream returns 0.
pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
    // FreeRTOS_gethostbyname requires a NUL-terminated C string.
    let mut cbuf = [0u8; 256];
    let bytes = hostname.as_bytes();
    if bytes.len() >= cbuf.len() {
        return Err(NetError(-1));
    }
    cbuf[..bytes.len()].copy_from_slice(bytes);
    // cbuf[bytes.len()] is already 0.
    let addr = unsafe { FreeRTOS_gethostbyname(cbuf.as_ptr()) };
    if addr == 0 {
        return Err(NetError(-1));
    }
    Ok(addr)
}
