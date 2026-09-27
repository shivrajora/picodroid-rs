// SPDX-License-Identifier: GPL-3.0-only
//! HTTPS for `HttpURLConnection` on a `has_tls` board: the TLS 1.3 session
//! (`pd-tls`) over the network HAL's socket, with this runtime's wall
//! clock behind certificate validity and the family's entropy behind the
//! key exchange. Design and numbers: docs/designs/tls-2026-09.md.
//!
//! Every blocking step — the handshake, each record read and write —
//! runs with the JVM run lock released, like the HAL's own socket calls:
//! a handshake is a second of elliptic-curve arithmetic on the RP2350, and
//! the UI thread must not wait for it.
//!
//! The handshake itself runs on a task of its own, spawned for the call
//! and gone after it: `rustpki`'s certificate decode and the curve
//! arithmetic run 24–32 KB deep, more than any Java task's stack (the
//! interpreter's 32 KB, less its own frames — the first device run took a
//! HardFault inside `der::Decode::from_der`; a `Thread`'s 16 KB; a pool
//! worker's 6 KB). The caller blocks, unlocked, until the task reports.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use pd_tls::embedded_io::{self, ErrorKind};
use pd_tls::{Failure, OpenError, TlsClock, TlsError, TlsSession};
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::JvmError;

use super::helpers::throw_named_exception;
use crate::hal::types::{NetError, NetErrorKind};
use crate::shrink_names::c;

// ── The HAL socket as an embedded-io stream ──────────────────────────────

/// A connected HAL socket behind the `embedded_io` traits the session
/// reads and writes through. Blocking; the facade already releases the
/// JVM run lock around each call.
pub struct HalSocket(pub *mut c_void);

#[derive(Debug)]
pub struct IoError(pub NetError);

impl core::fmt::Display for IoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?} (err {})", self.0.kind, self.0.raw)
    }
}

impl core::error::Error for IoError {}

impl embedded_io::Error for IoError {
    fn kind(&self) -> ErrorKind {
        match self.0.kind {
            NetErrorKind::TimedOut => ErrorKind::TimedOut,
            NetErrorKind::Closed => ErrorKind::ConnectionReset,
            NetErrorKind::Refused => ErrorKind::ConnectionRefused,
            NetErrorKind::Unreachable => ErrorKind::NotConnected,
            NetErrorKind::AddrInUse => ErrorKind::AddrInUse,
            NetErrorKind::HostLookup | NetErrorKind::Other => ErrorKind::Other,
        }
    }
}

impl embedded_io::ErrorType for HalSocket {
    type Error = IoError;
}

impl embedded_io::Read for HalSocket {
    /// `Ok(0)` is end of stream on every platform (the device HAL remaps
    /// FreeRTOS's inverted encoding), which is what the session expects.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
        crate::hal::net::tcp_recv(self.0, buf).map_err(IoError)
    }
}

impl embedded_io::Write for HalSocket {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError> {
        match crate::hal::net::tcp_send(self.0, buf) {
            // A blocking send that makes no progress means the peer is gone.
            Ok(0) if !buf.is_empty() => Err(IoError(NetError::new(NetErrorKind::Closed, 0))),
            other => other.map_err(IoError),
        }
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

// ── The wall clock ───────────────────────────────────────────────────────

/// Certificate validity is checked against `System.currentTimeMillis()`'s
/// anchor, which `SystemClock.setCurrentTimeMillis` (an SNTP sync) sets.
/// Unset, it answers `None` and the verifier fails closed.
pub struct WallClock;

impl TlsClock for WallClock {
    fn now() -> Option<u64> {
        let offset = crate::os::system_clock::wall_offset_ms();
        if offset == 0 {
            return None;
        }
        let millis = crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000 + offset;
        u64::try_from(millis / 1000).ok()
    }
}

// ── Entropy ──────────────────────────────────────────────────────────────

extern "C" {
    /// The family's entropy for key material (RP: TRNG words under the
    /// family's critical section, `hal/rp/entropy.rs`; simulator: the
    /// host's random source). Non-blocking: writes what is harvested at
    /// the front of `buf` and returns the count, 0 while the source is
    /// still sampling. Never a pseudo-random fallback.
    fn picodroid_port_entropy_bytes(buf: *mut u8, len: usize) -> usize;
}

/// The handshake seed: 32 bytes of hardware entropy, waiting up to two
/// seconds for the TRNG (a 192-bit harvest takes ~64 ms on the RP2350).
fn seed() -> Option<[u8; 32]> {
    let mut s = [0u8; 32];
    let mut filled = 0usize;
    for _ in 0..400 {
        // SAFETY: `s[filled..]` is a live, writable slice of the stated length.
        let n = unsafe { picodroid_port_entropy_bytes(s[filled..].as_mut_ptr(), s.len() - filled) };
        filled += n.min(s.len() - filled);
        if filled == s.len() {
            return Some(s);
        }
        if n == 0 {
            pd_rtos::delay_ms(5);
        }
    }
    None
}

// ── The stream an HttpConn runs over ─────────────────────────────────────

/// Why an HTTPS connect failed before any request byte was sent.
pub enum TlsFail {
    /// The entropy source gave nothing within the wait.
    NoEntropy,
    /// The arena could not spare the session's record buffers.
    OutOfMemory,
    Handshake(OpenError),
}

/// A TLS session over a connected HAL socket.
pub struct TlsStream {
    session: Box<TlsSession<HalSocket>>,
    sock: *mut c_void,
}

/// The handshake task's stack. Measured on the RP2350 (`pico_display2_w`,
/// 2026-09-27, a 64 KB task): 32,760 B used, the same for a P-256, a P-384
/// and an RSA chain — the peak is `rustpki`'s certificate decode, not the
/// arithmetic; a chain refused before the inner verifier used 12.8 KB. 40 KB
/// leaves an 18 % margin; a 32 KB task overflowed. Every handshake logs its
/// high-water mark (`tls: handshake stack …`), so re-measure before trimming.
/// Transient: allocated from the arena with the task, freed when it ends.
const HANDSHAKE_STACK_BYTES: u32 = 40 * 1024;

/// Stack a task must have spare for `close_notify` (`TlsStream::close`):
/// the record encoder's `ClientRecord` local and the AES-GCM key schedule.
/// A 16 KB Java thread with about 11 KB spare sent it; an 8 KB pool worker
/// with about 3 KB did not survive it. Between those, the bound errs high.
const CLOSE_NOTIFY_STACK_BYTES: u32 = 8 * 1024;

/// The handshake task's priority. On the device, the background tier below
/// the interpreter's: time slicing is off, so a compute-bound handshake at
/// the JVM's own priority would hold core 0 from the UI for the length of a
/// P-384 verify; below it, the handshake runs in the interpreter's idle
/// time, like a sensor task, and the interpreter is idle whenever every
/// Java task is blocked in the kernel — which on the device is any socket
/// wait. The simulator is different: its sockets block in host calls
/// (`poll`, `recv`) that FreeRTOS's POSIX port cannot see, so a Java thread
/// in `accept()` stays the running task and a lower-priority task never
/// gets its first slice (picoenvmon's dashboard thread starved the
/// handshake forever, 2026-09-27; its pool workers share the JVM tier, so
/// they were never affected). There the task takes the JVM tier itself.
#[cfg(not(any(feature = "sim", test)))]
const HANDSHAKE_PRIORITY: u8 = crate::task_priority::PRIORITY_BG_6;
#[cfg(any(feature = "sim", test))]
const HANDSHAKE_PRIORITY: u8 = crate::task_priority::PRIORITY_JVM_NORM;

/// What the handshake task works on; owned by the caller across the wait.
struct HandshakeJob {
    session: Box<TlsSession<HalSocket>>,
    host: String,
    seed: [u8; 32],
    result: Option<Result<(), OpenError>>,
    /// Set by the task after `result`, before it notifies the caller.
    done: AtomicBool,
}

/// The raw job pointer, moved into the task's closure. The session holds
/// raw socket and buffer pointers that only this task touches while it runs.
struct JobPtr(*mut HandshakeJob);
// SAFETY: the caller does not touch the job between the spawn and `done`.
unsafe impl Send for JobPtr {}

/// The current task's unused stack, in bytes, on the device; `None` where
/// the kernel cannot say (the simulator's tasks are host threads).
#[cfg(not(any(feature = "sim", test)))]
fn stack_unused_bytes() -> Option<u32> {
    extern "C" {
        fn uxTaskGetStackHighWaterMark(task: *mut c_void) -> usize;
    }
    // SAFETY: a null handle asks FreeRTOS about the calling task.
    let words = unsafe { uxTaskGetStackHighWaterMark(core::ptr::null_mut()) };
    Some((words * 4) as u32)
}

#[cfg(any(feature = "sim", test))]
fn stack_unused_bytes() -> Option<u32> {
    None
}

/// Run `session.open` on a task with a stack the handshake fits in, and
/// hand the session back once it has.
fn handshake(
    session: Box<TlsSession<HalSocket>>,
    host: &str,
    seed: [u8; 32],
) -> Result<Box<TlsSession<HalSocket>>, TlsFail> {
    let job = Box::into_raw(Box::new(HandshakeJob {
        session,
        host: host.into(),
        seed,
        result: None,
        done: AtomicBool::new(false),
    }));
    let ptr = JobPtr(job);
    let parent = pd_rtos::task_current();
    let spec = pd_rtos::TaskSpec {
        name: "tls-handshake",
        kind: pd_rtos::TaskKind::BgWorker,
        priority: HANDSHAKE_PRIORITY,
        stack_bytes: Some(HANDSHAKE_STACK_BYTES),
    };
    let spawned = pd_rtos::spawn(
        &spec,
        Box::new(move || {
            let ptr = ptr;
            // SAFETY: the caller is parked until `done`; nothing else holds the job.
            let job = unsafe { &mut *ptr.0 };
            let result = job.session.open::<WallClock>(&job.host, job.seed);
            if let Some(unused) = stack_unused_bytes() {
                crate::pd_info!(
                    "tls: handshake stack {} B, {} B unused",
                    HANDSHAKE_STACK_BYTES,
                    unused
                );
            }
            job.result = Some(result);
            job.done.store(true, Ordering::Release);
            pd_rtos::task_notify(parent);
        }),
    );
    if !spawned {
        // SAFETY: the task never ran; the job is ours again.
        drop(unsafe { Box::from_raw(job) });
        return Err(TlsFail::OutOfMemory);
    }
    {
        let _run = crate::jvm_run_lock::unlocked();
        // A notification is a wakeup, not a message: re-check the condition
        // (the debug bridge and the run lock notify tasks too).
        // SAFETY: only `done` is read while the task may still be writing.
        while !unsafe { &*job }.done.load(Ordering::Acquire) {
            pd_rtos::task_wait_notification(pd_rtos::Timeout::Forever);
        }
    }
    // SAFETY: `done` is set after the task's last write to the job.
    let job = unsafe { Box::from_raw(job) };
    match job.result {
        Some(Ok(())) => Ok(job.session),
        Some(Err(e)) => Err(TlsFail::Handshake(e)),
        None => Err(TlsFail::OutOfMemory),
    }
}

impl TlsStream {
    /// Run the handshake over `sock`, which is connected to `host`: SNI
    /// and the host-name check use `host`. The socket's receive timeout
    /// bounds each record read, the handshake's included.
    pub fn open(sock: *mut c_void, host: &str) -> Result<Self, TlsFail> {
        let Some(seed) = seed() else {
            return Err(TlsFail::NoEntropy);
        };
        let session = TlsSession::new(HalSocket(sock)).map_err(|_| TlsFail::OutOfMemory)?;
        let session = handshake(session, host, seed)?;
        Ok(Self { session, sock })
    }

    /// Encrypt and send `data`; the count is bounded by the write record,
    /// so callers loop like they do on a plain socket.
    pub fn send(&mut self, data: &[u8]) -> Result<usize, NetError> {
        let _run = crate::jvm_run_lock::unlocked();
        let n = self.session.write(data).map_err(net_error)?;
        self.session.flush().map_err(net_error)?;
        Ok(n)
    }

    /// Decrypted application data; `Ok(0)` at `close_notify` or end of stream.
    pub fn recv(&mut self, buf: &mut [u8]) -> Result<usize, NetError> {
        let _run = crate::jvm_run_lock::unlocked();
        self.session.read(buf).map_err(net_error)
    }

    /// Send `close_notify` and hand the socket back for the TCP close.
    pub fn close(mut self) -> *mut c_void {
        let _run = crate::jvm_run_lock::unlocked();
        // The alert is optional for an exchange that is complete (the TCP
        // close says as much), and sending it costs stack the caller may
        // not have: the record encoder puts a `ClientRecord` on the stack
        // and AES-GCM its key schedule, on top of the interpreter's frames
        // -- a pool worker with 3 KB to spare hard-faulted here
        // (pico_enviro_mon_w, 2026-09-27). With room, send it; without,
        // drop the session and say so once.
        match stack_unused_bytes() {
            Some(unused) if unused < CLOSE_NOTIFY_STACK_BYTES => {
                crate::pd_info!(
                    "tls: closing without close_notify, {} B of stack spare",
                    unused
                );
            }
            _ => {
                let _ = self.session.close();
            }
        }
        self.sock
    }
}

/// Record-layer errors in the socket taxonomy the HTTP code already maps
/// (`SocketTimeoutException` on a read timeout, `SocketException` on a
/// reset); anything TLS-specific after the handshake is a closed stream.
fn net_error(e: TlsError) -> NetError {
    match e {
        TlsError::Io(ErrorKind::TimedOut) => NetError::new(NetErrorKind::TimedOut, 0),
        TlsError::Io(ErrorKind::ConnectionReset) | TlsError::ConnectionClosed => {
            NetError::new(NetErrorKind::Closed, 0)
        }
        TlsError::Io(_) => NetError::other(0),
        _ => NetError::new(NetErrorKind::Closed, -1),
    }
}

/// The Java exception for a failed HTTPS connect: `SSLHandshakeException`
/// for anything the peer or its certificate did, `SSLException` for what
/// this side lacked, `SocketTimeoutException` for a handshake that ran
/// into the read timeout — all `IOException`s, as on Android.
pub fn throw_tls_fail(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    fail: TlsFail,
    host: &str,
) -> JvmError {
    let (class, msg): (&'static str, alloc::string::String) = match fail {
        TlsFail::NoEntropy => (
            c::javax_net_ssl_SSLException,
            "no hardware entropy for the handshake".into(),
        ),
        TlsFail::OutOfMemory => (
            c::javax_net_ssl_SSLException,
            "out of memory for the TLS session (record buffers, handshake task)".into(),
        ),
        TlsFail::Handshake(OpenError { tls, certificate }) => match certificate {
            Failure::NoClock => (
                c::javax_net_ssl_SSLHandshakeException,
                "wall clock not set; sync the time first (SntpClient)".into(),
            ),
            Failure::NoAnchor => (
                c::javax_net_ssl_SSLHandshakeException,
                format!("certificate chain of {host} is not issued by a known root"),
            ),
            Failure::Malformed => (
                c::javax_net_ssl_SSLHandshakeException,
                format!("malformed certificate chain from {host}"),
            ),
            Failure::Rejected => (
                c::javax_net_ssl_SSLHandshakeException,
                format!("certificate of {host} rejected (signature, validity or host name)"),
            ),
            Failure::None => match tls {
                TlsError::Io(ErrorKind::TimedOut) => (
                    c::java_net_SocketTimeoutException,
                    "TLS handshake timed out".into(),
                ),
                TlsError::Io(_) | TlsError::ConnectionClosed => (
                    c::javax_net_ssl_SSLHandshakeException,
                    format!("connection to {host} closed during the TLS handshake"),
                ),
                TlsError::HandshakeAborted(_, alert) => (
                    c::javax_net_ssl_SSLHandshakeException,
                    format!("TLS handshake with {host} aborted: {alert:?}"),
                ),
                TlsError::OutOfMemory => (
                    c::javax_net_ssl_SSLException,
                    "out of memory during the TLS handshake".into(),
                ),
                other => (
                    c::javax_net_ssl_SSLHandshakeException,
                    format!("TLS handshake with {host} failed: {other:?}"),
                ),
            },
        },
    };
    throw_named_exception(objects, strings, class, &msg)
}
