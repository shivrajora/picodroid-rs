// SPDX-License-Identifier: GPL-3.0-only
//! Types that cross the HAL seam.
//!
//! These moved out of the per-family `hal::gpio` / `hal::net` modules when
//! the HAL became a trait: shared code names them, so they must be owned by
//! the shared crate. Every family's definitions were already field-identical
//! (verified across `hal/rp/gpio.rs` and `hal/sim/gpio.rs` at extraction
//! time), so this is a re-home, not a redesign.
//!
//! The seam uses `extern "Rust"`, not `extern "C"` — Rust's own ABI carries
//! enums, structs, `Option`, and `Result` across the boundary unchanged, so
//! these need no `#[repr(C)]` and no lowering to raw integers.

/// Input pull configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    None,
    Up,
    Down,
}

/// Which edge(s) an input pin should raise an interrupt on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeTrigger {
    Rising,
    Falling,
    Both,
}

/// A button edge drained from the platform's GPIO interrupt queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpioEvent {
    pub pin: u8,
    pub rising: bool,
    /// µs timestamp (wrapping) captured at *enqueue* time — in the ISR on
    /// hardware, at inject time in the simulator.
    ///
    /// The contact debounce in the LVGL event layer compares these rather
    /// than a drain-time clock: edges can sit in the queue for hundreds of
    /// ms while the UI task stalls on an Activity transition, so only
    /// enqueue-time deltas measure the switch itself.
    pub t_us: u32,
}

/// Why a network operation failed, normalized across families.
///
/// `kind` is the cross-platform contract — shared code maps it to the Java
/// exception taxonomy (`ConnectException`, `SocketTimeoutException`, …).
/// `raw` carries the family's own code for the human-readable `(err N)`
/// suffix only: it is a positive host errno on the simulator and a negated
/// FreeRTOS+TCP errno on device, so shared code must never branch on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetError {
    pub kind: NetErrorKind,
    pub raw: i32,
}

impl NetError {
    pub const fn new(kind: NetErrorKind, raw: i32) -> Self {
        NetError { kind, raw }
    }

    /// An error with no better classification than its family code.
    pub const fn other(raw: i32) -> Self {
        NetError {
            kind: NetErrorKind::Other,
            raw,
        }
    }
}

/// Semantic classification of a [`NetError`], platform-neutral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetErrorKind {
    /// Peer actively refused the connection (RST during connect).
    Refused,
    /// connect/recv/accept exceeded its timeout.
    TimedOut,
    /// No route to the host (ARP/routing failure), where distinguishable.
    Unreachable,
    /// Peer closed or the socket is no longer connected (post-connect).
    Closed,
    /// Local bind conflict — the address/port is already in use.
    AddrInUse,
    /// Hostname resolution failed.
    HostLookup,
    /// Anything else; `raw` carries the family code.
    Other,
}
