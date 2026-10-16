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
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Once, OnceLock};
use std::time::Duration;

pub use crate::hal::types::{NetError, NetErrorKind};

/// The arena blocks a connected TCP socket holds on the device, charged
/// to the simulator's modelled arena for as long as this socket is
/// connected (docs/parity-audit.md M9). The first socket also charges the
/// stack's window-segment pool, which the device allocates once and keeps.
struct NetCharge {
    blocks: [u32; 4],
}

impl NetCharge {
    fn new() -> Self {
        static POOL: Once = Once::new();
        POOL.call_once(|| {
            super::boot_budget::charge_net_permanent(
                crate::net_budget::win_seg_pool_bytes(),
                "tcp window pool",
            );
        });
        let blocks = crate::net_budget::socket_blocks();
        for (b, what) in blocks.iter().zip([
            "tcp socket",
            "socket events",
            "tcp rx stream",
            "tcp tx stream",
        ]) {
            super::boot_budget::charge_net_socket(*b, what);
        }
        Self { blocks }
    }
}

impl Drop for NetCharge {
    fn drop(&mut self) {
        for b in self.blocks {
            super::boot_budget::release_net_socket(b);
        }
    }
}

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

/// A TCP handle's state. [`tcp_socket`] makes it unbound; [`tcp_connect`]
/// fills `stream` and [`tcp_listen`] fills `listener`, each at most once,
/// and nothing empties them: the socket is closed by dropping the record.
///
/// Everything here is reached through `&self`: std's socket calls take
/// `&self`, the two `OnceLock`s are filled through `&self`, and the timeout
/// is an atomic. So a socket needs no `&mut` after `box_socket`, and two
/// Java threads on one socket — a reader blocked in `tcp_recv` with the run
/// lock released while another sends — hold two shared references, which
/// is allowed.
struct TcpState {
    /// The connected stream: `tcp_connect`'s, or an accepted connection's.
    stream: OnceLock<TcpStream>,
    /// The listening socket (`tcp_listen`).
    listener: OnceLock<TcpListener>,
    /// The receive timeout set while `stream` is empty, in nanoseconds; 0
    /// is none. The device stores `FREERTOS_SO_RCVTIMEO` in the socket,
    /// where it bounds `FreeRTOS_connect` and `FreeRTOS_accept` and then
    /// stays on as the receive timeout; std splits that one value across
    /// `connect_timeout`, an accept poll and `set_read_timeout`, so the
    /// setting is stashed here until [`tcp_connect`] applies it to both,
    /// and [`tcp_accept`] reads it as its deadline. (`TcpListener` has no
    /// native accept deadline, unlike read timeouts on streams, so the
    /// accept emulates one with a nonblocking poll loop.)
    timeout_ns: AtomicU64,
    /// The device's arena cost of the connection, released when the socket
    /// is dropped (closed).
    charge: OnceLock<NetCharge>,
}

impl TcpState {
    fn unbound() -> Self {
        Self {
            stream: OnceLock::new(),
            listener: OnceLock::new(),
            timeout_ns: AtomicU64::new(0),
            charge: OnceLock::new(),
        }
    }

    /// An accepted connection, charged like a connected one.
    fn connected(stream: TcpStream) -> Self {
        Self {
            stream: OnceLock::from(stream),
            listener: OnceLock::new(),
            timeout_ns: AtomicU64::new(0),
            charge: OnceLock::from(NetCharge::new()),
        }
    }

    /// The connected stream, or the error every operation that needs one
    /// reports on a handle that has none.
    fn stream(&self) -> Result<&TcpStream, NetError> {
        self.stream.get().ok_or_else(bad_handle)
    }

    fn timeout(&self) -> Option<Duration> {
        let ns = self.timeout_ns.load(Ordering::Relaxed);
        (ns != 0).then(|| Duration::from_nanos(ns))
    }

    fn set_timeout(&self, dur: Option<Duration>) {
        let ns = dur.map_or(0, |d| d.as_nanos().min(u64::MAX as u128) as u64);
        self.timeout_ns.store(ns, Ordering::Relaxed);
    }
}

/// Tagged union wrapping the std::net socket types behind a single
/// `*mut c_void` pointer for the HAL API.
enum SimSocket {
    Tcp(TcpState),
    Udp(UdpSocket),
}

// `deref_socket` hands a `&SimSocket` to whichever Java thread holds the
// handle, so the record must be shareable between threads.
const _: () = {
    fn assert_sync<T: Sync>() {}
    let _ = assert_sync::<SimSocket>;
};

/// Box a SimSocket and leak it as a raw pointer.
fn box_socket(s: SimSocket) -> *mut c_void {
    Box::into_raw(Box::new(s)) as *mut c_void
}

/// Recover a `&SimSocket` from a raw pointer. Shared, not exclusive: see
/// [`TcpState`] for why no operation needs `&mut`.
///
/// # Safety
/// `ptr` must have been created by `box_socket` and not yet passed to
/// [`close`].
unsafe fn deref_socket(ptr: *mut c_void) -> &'static SimSocket {
    &*(ptr as *const SimSocket)
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
    Ok(box_socket(SimSocket::Tcp(TcpState::unbound())))
}

pub fn tcp_connect(sock: *mut c_void, addr: u32, port: u16) -> Result<(), NetError> {
    if !link_up() {
        return Err(link_down());
    }
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let SimSocket::Tcp(t) = s else {
        return Err(bad_handle());
    };
    if t.stream.get().is_some() || t.listener.get().is_some() {
        // Already connected or listening: `java.net.Socket.connect` throws
        // on a second connect, and the device's stack refuses one too.
        return Err(NetError::other(-1));
    }
    let pending = t.timeout();
    let sa = SocketAddrV4::new(u32_to_ipv4(addr), port);
    let stream = match pending {
        // std's connect_timeout tracks its deadline across EINTR
        // internally (a poll loop with recomputed remaining time),
        // so no retry_eintr wrapper — one would restart the window.
        Some(d) => {
            TcpStream::connect_timeout(&std::net::SocketAddr::V4(sa), d).map_err(map_io_err)?
        }
        None => retry_eintr(|| TcpStream::connect(sa))?,
    };
    if let Some(d) = pending {
        // Device parity: a pre-connect RCVTIMEO stays on as the
        // socket's receive timeout after connect.
        let _ = stream.set_read_timeout(Some(d));
    }
    // Two connects racing on one unbound handle: the first to finish owns
    // the socket, the other's stream is dropped here and it reports the
    // "already connected" error above.
    if t.stream.set(stream).is_err() {
        return Err(NetError::other(-1));
    }
    let _ = t.charge.set(NetCharge::new());
    Ok(())
}

pub fn tcp_send(sock: *mut c_void, buf: &[u8]) -> Result<usize, NetError> {
    use std::io::Write;
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let SimSocket::Tcp(t) = s else {
        return Err(bad_handle());
    };
    // `Write` is implemented for `&TcpStream`: a send needs no `&mut`.
    let mut stream = t.stream()?;
    retry_eintr(|| stream.write(buf))
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
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let SimSocket::Tcp(t) = s else {
        return Err(bad_handle());
    };
    // `Read` is implemented for `&TcpStream`: a receive needs no `&mut`.
    let mut stream = t.stream()?;
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

pub fn tcp_listen(sock: *mut c_void, port: u16) -> Result<(), NetError> {
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let SimSocket::Tcp(t) = s else {
        return Err(bad_handle());
    };
    if t.stream.get().is_some() || t.listener.get().is_some() {
        return Err(bad_handle());
    }
    let listener =
        retry_eintr(|| TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)))?;
    // Two listens racing on one handle: the first to finish owns it.
    t.listener.set(listener).map_err(|_| bad_handle())
}

pub fn tcp_accept(sock: *mut c_void) -> Result<*mut c_void, NetError> {
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let SimSocket::Tcp(t) = s else {
        return Err(bad_handle());
    };
    let Some(listener) = t.listener.get() else {
        return Err(bad_handle());
    };
    let Some(dur) = t.timeout() else {
        let (stream, _addr) = retry_eintr(|| listener.accept())?;
        return Ok(box_socket(SimSocket::Tcp(TcpState::connected(stream))));
    };
    // With a timeout set, emulate an accept deadline by polling a
    // nonblocking listener — WouldBlock here means "nothing pending
    // yet", never the deadline itself, so it must not go through
    // `map_io_err` (which reads WouldBlock as TimedOut).
    //
    // Block in `poll(2)` until a connection is pending or the
    // deadline passes, then accept without blocking. The device
    // blocks in `FreeRTOS_accept` with the timeout in the socket;
    // `SO_RCVTIMEO` would be the host equivalent, but `accept`
    // honours it only on Linux, so the wait is a poll — deadline
    // tracked across the `EINTR` the simulator's 1 ms `SIGALRM`
    // tick delivers, and never a sleep-poll (audit F17).
    let deadline = std::time::Instant::now() + dur;
    listener.set_nonblocking(true).map_err(map_io_err)?;
    let result = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break Err(NetError::new(NetErrorKind::TimedOut, -1));
        }
        let mut pfd = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // Round up: a sub-millisecond remainder must still wait.
        let wait_ms = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        // SAFETY: one valid `pollfd` for the one-element array.
        let ready = unsafe { libc::poll(&mut pfd, 1, wait_ms) };
        if ready < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break Err(map_io_err(e));
        }
        if ready == 0 {
            continue; // deadline is re-checked at the top
        }
        match listener.accept() {
            Ok((stream, _addr)) => {
                let _ = stream.set_nonblocking(false);
                break Ok(box_socket(SimSocket::Tcp(TcpState::connected(stream))));
            }
            // Readable but gone by the time we looked (the peer
            // reset): wait for the next one.
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => break Err(map_io_err(e)),
        }
    };
    let _ = listener.set_nonblocking(false);
    result
}

// ── UDP ──────────────────────────────────────────────────────────────────────

pub fn udp_socket(local_port: u16) -> Result<*mut c_void, NetError> {
    let sock =
        retry_eintr(|| UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, local_port)))?;
    // `java.net.DatagramSocket` turns SO_BROADCAST on by default, and
    // FreeRTOS+TCP has no such switch at all, so the host socket gets it too.
    // `PICODROID_SIM_NET_BROADCAST=0` withholds it to model a network that
    // swallows client broadcasts (an access point with client isolation): a
    // send to a broadcast address then fails, and an app that discovers its
    // peer by broadcast has to fall back to its configured address.
    if !broadcast_disabled() {
        let _ = sock.set_broadcast(true);
    }
    Ok(box_socket(SimSocket::Udp(sock)))
}

fn broadcast_disabled() -> bool {
    std::env::var("PICODROID_SIM_NET_BROADCAST").is_ok_and(|v| v == "0")
}

pub fn udp_sendto(sock: *mut c_void, buf: &[u8], addr: u32, port: u16) -> Result<usize, NetError> {
    if !link_up() {
        return Err(link_down());
    }
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(udp) => {
            let ip = u32_to_ipv4(addr);
            retry_eintr(|| udp.send_to(buf, SocketAddrV4::new(ip, port)))
        }
        _ => Err(bad_handle()),
    }
}

pub fn udp_recvfrom(sock: *mut c_void, buf: &mut [u8]) -> Result<(usize, u32, u16), NetError> {
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    match s {
        SimSocket::Udp(udp) => {
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
    // SAFETY: `sock` came from `box_socket` (the handle tables hand out no
    // other pointer) and `close` is its last use; the tables drop it after.
    unsafe {
        let _ = Box::from_raw(sock as *mut SimSocket);
    }
}

pub fn set_recv_timeout(sock: *mut c_void, timeout_ms: u32) {
    // SAFETY: `sock` is a live `box_socket` pointer (the handle tables hand
    // out no other) that `close` has not been given.
    let s = unsafe { deref_socket(sock) };
    let dur = if timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };
    match s {
        SimSocket::Tcp(t) => match t.stream.get() {
            Some(stream) => {
                let _ = stream.set_read_timeout(dur);
            }
            // Not connected: stash it. `tcp_connect` uses it as the connect
            // deadline and then applies it as the read timeout — the
            // device's pre-connect RCVTIMEO semantics — and `tcp_accept` as
            // its deadline.
            None => t.set_timeout(dur),
        },
        SimSocket::Udp(udp) => {
            let _ = udp.set_read_timeout(dur);
        }
    }
}

// ── Link state ───────────────────────────────────────────────────────────────

/// Simulated link state. The host's network is always there, so "the WiFi
/// join failed / dropped" is a switch: `PICODROID_SIM_NET=down` boots with the
/// link down, and the control channel's `net up|down` flips it at runtime.
static LINK_UP: AtomicBool = AtomicBool::new(true);
static LINK_INIT: Once = Once::new();

fn link_up() -> bool {
    LINK_INIT.call_once(|| {
        if std::env::var("PICODROID_SIM_NET").is_ok_and(|v| v.eq_ignore_ascii_case("down")) {
            LINK_UP.store(false, Ordering::Relaxed);
            println!("[sim] net: link down at boot (PICODROID_SIM_NET=down)");
        }
    });
    LINK_UP.load(Ordering::Relaxed)
}

/// Take the simulated link up or down (control channel `net up|down`).
///
/// Down means what a lost join means to an app: [`is_network_up`] is false,
/// the address is 0.0.0.0, and new connects, sends and lookups fail. Sockets
/// already connected keep flowing — the sim does not emulate a mid-stream
/// stall.
pub fn set_link_up(up: bool) {
    link_up(); // settle the boot-time env read first, so it cannot undo this
    let was = LINK_UP.swap(up, Ordering::Relaxed);
    println!("[sim] net: link {}", if up { "up" } else { "down" });
    // What the IP stack's event hook does on device: a transition bumps the
    // generation the event loop watches for `ConnectivityManager`.
    if was != up {
        crate::hal::net_edge::LINK_CHANGES.note();
    }
}

/// What a link-down send/connect fails with.
const fn link_down() -> NetError {
    NetError::new(NetErrorKind::Unreachable, -1)
}

/// Check if the network stack is up (true unless the link was taken down —
/// see [`set_link_up`]).
pub fn is_network_up() -> bool {
    link_up()
}

/// Get the assigned IP address: the host's outbound IPv4 address, which is
/// where a listening socket (bound to 0.0.0.0) is reachable from the LAN.
/// 127.0.0.1 on a host with no route; 0.0.0.0 while the link is down.
pub fn get_ip_address() -> u32 {
    if !link_up() {
        return 0;
    }
    // Connecting a UDP socket sends nothing; it only makes the kernel pick
    // the source address it would route from.
    let host = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|s| {
        s.connect((Ipv4Addr::new(192, 0, 2, 1), 9))
            .and(s.local_addr())
    });
    match host {
        Ok(std::net::SocketAddr::V4(v4)) if !v4.ip().is_unspecified() => {
            u32::from_be_bytes(v4.ip().octets())
        }
        _ => 0x7F000001,
    }
}

/// Resolve a hostname to a packed IPv4 address (MSB = first octet).
///
/// Accepts both DNS names and dotted-quad literals — `ToSocketAddrs`
/// handles either.  Returns the first IPv4 result; failure to resolve
/// (including an IPv6-only hostname) is `HostLookup`.
pub fn dns_resolve(hostname: &str) -> Result<u32, NetError> {
    if !link_up() {
        return Err(NetError::new(NetErrorKind::HostLookup, -1));
    }
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
