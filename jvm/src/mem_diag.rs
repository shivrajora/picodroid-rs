// SPDX-License-Identifier: GPL-3.0-only
//! Steady-state growth sentinel for the `mem-diag` feature
//! (docs/memory-diagnostics.md).
//!
//! Platform-agnostic core of the memory monitor: the platform layer feeds one
//! "used bytes" floor per monitor window and this module answers the only
//! question that matters for leak detection — *has the floor been rising for
//! several consecutive windows?* A rising **post-GC** floor cannot be
//! explained by not-yet-collected garbage; it is retained growth.
//!
//! Everything here is plain fields and fixed-size arrays: no allocation, no
//! atomics (thumbv6m has no atomic RMW), no timestamps (windows are the
//! platform's clock). Two instances run in practice — one fed the JVM
//! post-GC live-bytes floor, one fed native-arena used bytes.

/// Consecutive windows inspected by the trip condition.
pub const SENTINEL_WINDOWS: usize = 8;

/// Minimum total floor rise (bytes) across [`SENTINEL_WINDOWS`] required to
/// trip. One fields-arena growth step ([`crate::object_heap`]'s
/// `FIELDS_ARENA_CHUNK` = 256 values = 4 KB), so jitter below a single arena
/// chunk never fires.
pub const SENTINEL_THRESHOLD_BYTES: u32 = 4096;

/// Windows discarded after [`GrowthSentinel::arm`] before the baseline is
/// recorded — lets the first post-onCreate frames settle.
pub const SETTLE_WINDOWS: u8 = 2;

/// A tripped sentinel: the floor rose `delta` bytes over `windows` windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeakReport {
    /// Floor rise across the inspected windows (newest − oldest).
    pub delta: u32,
    /// Floor recorded when the sentinel armed (after settling).
    pub baseline: u32,
    /// Newest window floor.
    pub now: u32,
    /// Number of windows the rise spans.
    pub windows: u32,
}

/// Detects a monotonically-rising used-bytes floor in steady state.
///
/// Dormant until [`arm`](Self::arm) (apps legitimately grow during onCreate /
/// class loading); after arming it discards [`SETTLE_WINDOWS`] windows, then
/// records the baseline. Trips when, over the last [`SENTINEL_WINDOWS`]
/// window-to-window deltas, at least `SENTINEL_WINDOWS - 1` are positive
/// (one flat/noisy window tolerated) AND both the rise across those windows
/// and the rise above baseline exceed [`SENTINEL_THRESHOLD_BYTES`]. After a
/// trip the ring resets, so a persisting leak re-trips every
/// `SENTINEL_WINDOWS` windows instead of spamming every window.
pub struct GrowthSentinel {
    /// Last `SENTINEL_WINDOWS + 1` window floors, oldest first.
    ring: [u32; SENTINEL_WINDOWS + 1],
    len: u8,
    armed: bool,
    settle: u8,
    baseline: u32,
}

impl GrowthSentinel {
    pub const fn new() -> Self {
        Self {
            ring: [0; SENTINEL_WINDOWS + 1],
            len: 0,
            armed: false,
            settle: 0,
            baseline: 0,
        }
    }

    /// Begin watching. Idempotent; re-arming while armed is a no-op.
    pub fn arm(&mut self) {
        if !self.armed {
            self.armed = true;
            self.settle = SETTLE_WINDOWS;
            self.len = 0;
        }
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Baseline floor recorded after settling (0 until then).
    pub fn baseline(&self) -> u32 {
        self.baseline
    }

    /// Feed the floor for the window that just ended. Returns a report when
    /// the trip condition is met.
    pub fn push_window(&mut self, floor: u32) -> Option<LeakReport> {
        if !self.armed {
            return None;
        }
        if self.settle > 0 {
            self.settle -= 1;
            if self.settle == 0 {
                self.baseline = floor;
            }
            return None;
        }
        if (self.len as usize) < self.ring.len() {
            self.ring[self.len as usize] = floor;
            self.len += 1;
        } else {
            self.ring.copy_within(1.., 0);
            self.ring[SENTINEL_WINDOWS] = floor;
        }
        if (self.len as usize) < self.ring.len() {
            return None;
        }

        let oldest = self.ring[0];
        let newest = self.ring[SENTINEL_WINDOWS];
        let mut rising = 0usize;
        for i in 0..SENTINEL_WINDOWS {
            if self.ring[i + 1] > self.ring[i] {
                rising += 1;
            }
        }
        let span_rise = newest.saturating_sub(oldest);
        let above_baseline = newest.saturating_sub(self.baseline);
        if rising >= SENTINEL_WINDOWS - 1
            && span_rise >= SENTINEL_THRESHOLD_BYTES
            && above_baseline >= SENTINEL_THRESHOLD_BYTES
        {
            self.len = 0; // reset so a persisting leak re-trips, not spams
            return Some(LeakReport {
                delta: span_rise,
                baseline: self.baseline,
                now: newest,
                windows: SENTINEL_WINDOWS as u32,
            });
        }
        None
    }
}

impl Default for GrowthSentinel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed_and_settled(baseline: u32) -> GrowthSentinel {
        let mut s = GrowthSentinel::new();
        s.arm();
        for _ in 0..SETTLE_WINDOWS {
            assert_eq!(s.push_window(baseline), None);
        }
        assert_eq!(s.baseline(), baseline);
        s
    }

    #[test]
    fn dormant_until_armed() {
        let mut s = GrowthSentinel::new();
        for i in 0..100u32 {
            assert_eq!(s.push_window(i * 10_000), None);
        }
    }

    #[test]
    fn flat_floor_never_trips() {
        let mut s = armed_and_settled(50_000);
        for _ in 0..100 {
            assert_eq!(s.push_window(50_000), None);
        }
    }

    #[test]
    fn jitter_below_threshold_never_trips() {
        let mut s = armed_and_settled(50_000);
        // Oscillate ±2 KB — rises exist but the span stays under 4 KB.
        for i in 0..100u32 {
            let floor = 50_000 + if i % 2 == 0 { 0 } else { 2_048 };
            assert_eq!(s.push_window(floor), None);
        }
    }

    #[test]
    fn steady_leak_trips_with_report() {
        let mut s = armed_and_settled(50_000);
        // +1 KB per window: after SENTINEL_WINDOWS+1 samples the span rise is
        // 8 KB with 8/8 rising deltas.
        let mut report = None;
        for i in 0..=(SENTINEL_WINDOWS as u32) {
            report = s.push_window(50_000 + i * 1_024);
            if report.is_some() {
                break;
            }
        }
        let r = report.expect("steady 1KB/window leak must trip");
        assert_eq!(r.baseline, 50_000);
        assert_eq!(r.delta, SENTINEL_WINDOWS as u32 * 1_024);
        assert_eq!(r.now, 50_000 + SENTINEL_WINDOWS as u32 * 1_024);
    }

    #[test]
    fn one_flat_window_is_tolerated() {
        let mut s = armed_and_settled(10_000);
        // 7 rising deltas + 1 flat one, total rise well above threshold.
        let floors = [
            10_000u32, 12_000, 14_000, 16_000, 16_000, 18_000, 20_000, 22_000, 24_000,
        ];
        let mut tripped = false;
        for f in floors {
            tripped |= s.push_window(f).is_some();
        }
        assert!(tripped, "K-1 rising windows above threshold must trip");
    }

    #[test]
    fn single_step_then_flat_does_not_trip() {
        // A one-off allocation (e.g. a lazily-built cache) raises the floor
        // once and then stays flat — that is growth, but not a leak trend.
        let mut s = armed_and_settled(10_000);
        assert_eq!(s.push_window(10_000), None);
        for _ in 0..50 {
            assert_eq!(s.push_window(30_000), None);
        }
    }

    #[test]
    fn retrip_after_reset_on_persisting_leak() {
        let mut s = armed_and_settled(0);
        let mut trips = 0;
        for i in 0..(4 * SENTINEL_WINDOWS as u32) {
            if s.push_window(i * 2_048).is_some() {
                trips += 1;
            }
        }
        assert!(trips >= 2, "persisting leak should re-trip, got {trips}");
    }
}
