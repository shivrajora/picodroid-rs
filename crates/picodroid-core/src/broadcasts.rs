// SPDX-License-Identifier: GPL-3.0-only
//! System broadcasts — the framework half of `picodroid.content.BroadcastReceiver`
//! (docs/designs/broadcasts-2026-10.md).
//!
//! The receiver registry lives in Java, inside `BroadcastReceiver`'s statics,
//! where the heap roots it and the reset at an app switch clears it. What
//! this side keeps is small: the *interest mask*, the OR of the system
//! actions some registered filter names, so a tick with no listener costs
//! one load; and the per-source edge state the event loop folds into one
//! `BroadcastReceiver.fireSystem(mask)` upcall per frame
//! (`lifecycle::broadcast_events`).
//!
//! The bits are `BroadcastReceiver`'s `BIT_*` constants; keep both in step.
//! Their order is the delivery order within one frame: a clock set before the
//! tick that follows it.

use core::sync::atomic::{AtomicU32, Ordering};

/// `ACTION_TIME_CHANGED`: the wall clock was set.
pub const TIME_CHANGED: u32 = 1;
/// `ACTION_TIMEZONE_CHANGED`: the platform zone moved.
pub const TIMEZONE_CHANGED: u32 = 1 << 1;
/// `ACTION_TIME_TICK`: the wall-clock minute changed.
pub const TIME_TICK: u32 = 1 << 2;
/// `ACTION_SCREEN_OFF`: the panel dozed.
pub const SCREEN_OFF: u32 = 1 << 3;
/// `ACTION_SCREEN_ON`: the panel woke.
pub const SCREEN_ON: u32 = 1 << 4;
/// `ACTION_BOOT_COMPLETED`: manifest receivers only; never raised by the loop.
pub const BOOT_COMPLETED: u32 = 1 << 5;
/// `WifiManager.SUPPLICANT_STATE_CHANGED_ACTION`: the station's state moved.
pub const SUPPLICANT_STATE: u32 = 1 << 6;
/// `WifiManager.NETWORK_STATE_CHANGED_ACTION`: the link came up or dropped.
pub const NETWORK_STATE: u32 = 1 << 7;
/// `WifiManager.SCAN_RESULTS_AVAILABLE_ACTION`: a scan finished.
pub const SCAN_RESULTS: u32 = 1 << 8;
/// `ConnectivityManager.CONNECTIVITY_ACTION`: the link came up or dropped.
pub const CONNECTIVITY: u32 = 1 << 9;

/// What some registered filter wants. Written by the `nativeSetInterest`
/// native (any Java thread), read by the UI tick: one relaxed atomic.
static INTEREST: AtomicU32 = AtomicU32::new(0);

/// `BroadcastReceiver.nativeSetInterest`: the bits to watch for; 0 is nobody.
pub fn set_interest(mask: u32) {
    INTEREST.store(mask, Ordering::Relaxed);
}

/// The bits some registered filter names.
pub fn interest() -> u32 {
    INTEREST.load(Ordering::Relaxed)
}

/// The previous app's registrations died with its heap; its interest must
/// not outlive them, or the loop would upcall into a class nobody loaded.
pub fn reset() {
    INTEREST.store(0, Ordering::Relaxed);
}

/// The wall-clock minute `wall_ms` is in. `TIME_TICK` fires when this
/// changes between two ticks; floor division keeps a clock before the epoch
/// (unset, counting from boot) on the same rule.
pub fn minute_of(wall_ms: i64) -> i64 {
    wall_ms.div_euclid(60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_are_distinct_and_ordered() {
        let bits = [
            TIME_CHANGED,
            TIMEZONE_CHANGED,
            TIME_TICK,
            SCREEN_OFF,
            SCREEN_ON,
            BOOT_COMPLETED,
            SUPPLICANT_STATE,
            NETWORK_STATE,
            SCAN_RESULTS,
            CONNECTIVITY,
        ];
        for (i, b) in bits.iter().enumerate() {
            assert_eq!(b.count_ones(), 1);
            assert_eq!(*b, 1 << i, "delivery order is bit order");
        }
    }

    #[test]
    fn minute_edges() {
        assert_eq!(minute_of(0), 0);
        assert_eq!(minute_of(59_999), 0);
        assert_eq!(minute_of(60_000), 1);
        assert_eq!(minute_of(-1), -1, "a clock before the epoch still ticks");
        assert_eq!(minute_of(1_760_000_039_999), 29_333_333);
        assert_eq!(minute_of(1_760_000_040_000), 29_333_334);
    }

    #[test]
    fn interest_round_trips_and_resets() {
        set_interest(TIME_TICK | SCREEN_ON);
        assert_eq!(interest(), TIME_TICK | SCREEN_ON);
        reset();
        assert_eq!(interest(), 0);
    }
}
