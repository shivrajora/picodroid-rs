// SPDX-License-Identifier: GPL-3.0-only
//! Edge detection for the IP stack's network up/down hook.
//!
//! FreeRTOS+TCP calls its network-event hook with `eNetworkDown` on every
//! initialisation retry (once every 3 s by default) for as long as the link
//! driver's `pfInitialise` fails — a WiFi join that keeps failing produced
//! thirty `net: down` lines in a 90 s soak (network-seam design, A9). The
//! repeats carry no information: what matters is the transition. [`NetEdge`]
//! remembers the last state it saw and reports only a change, so the hook
//! logs `net: down` once when the link first fails (or is lost) and `net: up`
//! once when it comes up, plus again whenever the address changes.
//!
//! Family-neutral: no stack or kernel names, so any IP stack's event hook
//! can use it, and the host tests cover it without a kernel to link.
//!
//! [`LINK_CHANGES`] is the same idea one level up, for the app: every
//! transition any link reports bumps a generation the event loop compares
//! against the one it last saw (`lifecycle::net_events`), so the tick's fast
//! path is one atomic load and Java's `ConnectivityManager` hears about the
//! link only when it changed.

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

const UNKNOWN: u8 = 0;
const DOWN: u8 = 1;
const UP: u8 = 2;

/// A transition the caller should report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetTransition {
    /// The link came up, or is up with a different address. Carries the
    /// address exactly as the caller passed it (no byte-order opinion).
    Up(u32),
    /// The link went down, or failed to come up for the first time.
    Down,
}

/// The last link state an event hook saw. One per stack; a `static`.
pub struct NetEdge {
    state: AtomicU8,
    ip: AtomicU32,
}

impl NetEdge {
    /// Before the first event, so the first `down` and the first `up` are
    /// both transitions.
    pub const fn new() -> Self {
        NetEdge {
            state: AtomicU8::new(UNKNOWN),
            ip: AtomicU32::new(0),
        }
    }

    /// Record an event; `Some` when it changed the state the caller should
    /// log. Down after down is `None`; up after up is `None` unless `ip`
    /// differs. Only the stack's event context calls this, one event at a
    /// time, so the two loads and stores need no ordering between them.
    pub fn observe(&self, up: bool, ip: u32) -> Option<NetTransition> {
        let prev = self.state.load(Ordering::Relaxed);
        if up {
            let prev_ip = self.ip.load(Ordering::Relaxed);
            self.state.store(UP, Ordering::Relaxed);
            self.ip.store(ip, Ordering::Relaxed);
            if prev == UP && prev_ip == ip {
                return None;
            }
            Some(NetTransition::Up(ip))
        } else {
            self.state.store(DOWN, Ordering::Relaxed);
            if prev == DOWN {
                return None;
            }
            Some(NetTransition::Down)
        }
    }
}

impl Default for NetEdge {
    fn default() -> Self {
        Self::new()
    }
}

/// How many times the link has changed state since boot, as a generation
/// the event loop polls (one relaxed load per tick). Bumped by whatever
/// reports link transitions on a platform — the IP stack's event hook on
/// device, `net up|down` in the simulator — and read on the JVM task, which
/// then asks the HAL for the state itself; the counter carries no state, so
/// a bump that lands a frame late costs nothing but the frame.
pub struct LinkChanges {
    generation: AtomicU32,
}

impl LinkChanges {
    pub const fn new() -> Self {
        LinkChanges {
            generation: AtomicU32::new(0),
        }
    }

    /// Record one transition. Load-then-store rather than `fetch_add`: the
    /// RP2040 (thumbv6m) has no atomic RMW, and one writer per platform
    /// reports link events, so no bump can be lost to a race.
    pub fn note(&self) {
        let next = self.generation.load(Ordering::Relaxed).wrapping_add(1);
        self.generation.store(next, Ordering::Release);
    }

    /// The generation now; unequal to an earlier read iff `note` ran between.
    pub fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }
}

impl Default for LinkChanges {
    fn default() -> Self {
        Self::new()
    }
}

/// The one link's change counter (docs: `ConnectivityManager`).
pub static LINK_CHANGES: LinkChanges = LinkChanges::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_down_is_reported_once() {
        let e = NetEdge::new();
        assert_eq!(e.observe(false, 0), Some(NetTransition::Down));
        for _ in 0..30 {
            assert_eq!(e.observe(false, 0), None);
        }
    }

    #[test]
    fn up_then_down_then_up_are_three_transitions() {
        let e = NetEdge::new();
        assert_eq!(
            e.observe(true, 0x2a01_a8c0),
            Some(NetTransition::Up(0x2a01_a8c0))
        );
        assert_eq!(e.observe(true, 0x2a01_a8c0), None);
        assert_eq!(e.observe(false, 0), Some(NetTransition::Down));
        assert_eq!(e.observe(false, 0), None);
        assert_eq!(
            e.observe(true, 0x2a01_a8c0),
            Some(NetTransition::Up(0x2a01_a8c0))
        );
    }

    #[test]
    fn a_new_address_while_up_is_a_transition() {
        let e = NetEdge::new();
        assert_eq!(e.observe(true, 1), Some(NetTransition::Up(1)));
        assert_eq!(e.observe(true, 2), Some(NetTransition::Up(2)));
        assert_eq!(e.observe(true, 2), None);
    }

    #[test]
    fn first_event_is_always_a_transition() {
        assert_eq!(NetEdge::new().observe(false, 0), Some(NetTransition::Down));
        assert_eq!(NetEdge::new().observe(true, 7), Some(NetTransition::Up(7)));
    }

    #[test]
    fn link_changes_generation_moves_once_per_note() {
        let c = LinkChanges::new();
        let g0 = c.generation();
        assert_eq!(c.generation(), g0);
        c.note();
        assert_ne!(c.generation(), g0);
        let g1 = c.generation();
        c.note();
        c.note();
        assert_eq!(c.generation(), g1.wrapping_add(2));
    }
}
