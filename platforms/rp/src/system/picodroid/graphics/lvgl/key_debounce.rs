// SPDX-License-Identifier: GPL-3.0-only
// Boards without buttons (`cfg(not(has_buttons))`) include this module but
// never call into it from `events.rs`. The dead_code lint can't see the
// has_buttons-gated callsite, so suppress it at module scope rather than
// duplicating the cfg gate on every item.
#![cfg_attr(not(any(has_buttons, test)), allow(dead_code))]
//! Per-pin contact-debounce for the button edge queue.
//!
//! Sized from a 2026-07-24 tuning session on Pico Enviro+ Pack hardware
//! (150 physical edges across GP12-15): real contact chatter arrives as
//! bursts of 2-8 edges with gaps of 2-603 µs, each burst spanning at most
//! ~1.6 ms — the Y button contributed 16 of the 19 chatter deltas. The
//! nearest legitimate same-pin gaps sit far above that: a 19.1 ms mechanical
//! re-actuation (a second full press with a 131.8 ms hold — not chatter),
//! 39.7 ms PDB synthetic press width, 45.7 ms fastest deliberate re-press,
//! ~74 ms shortest real hold. A 5 ms window is 3x the worst burst and 4-9x
//! below everything real.
//!
//! The comparison MUST use the ISR-captured `GpioEvent::t_us`, not a clock
//! read at drain time: edges sit in the queue for up to ~400 ms observed
//! while the UI task stalls on an Activity transition, so drain-time deltas
//! do not measure the switch.
//!
//! The window is non-retriggerable — it runs from the last *accepted* edge,
//! and rejected edges do not extend it — so continuous chatter cannot
//! suppress input indefinitely. Because a burst always settles at the level
//! its first edge moved to, collapsing a burst to its first edge keeps the
//! tracked state consistent; the phantom-release filter downstream
//! (`key_filter.rs`) still drops any residual unpaired release.
//!
//! Pure data; no `static mut`. `keypad_read_cb` in `events.rs` owns one
//! instance behind its own synchronisation; tests can construct any number
//! of `KeyDebounce`s in isolation.

/// Dead-time after an accepted edge during which further edges on the same
/// pin are dropped as contact chatter. See the module doc for how this was
/// measured; re-run the `btn-tune` logging session (git history of
/// `hal/rp/gpio.rs`) before changing it.
pub const DEBOUNCE_WINDOW_US: u32 = 5_000;

/// Per-pin debounce state, indexed by `pin & 0x1f`.
pub struct KeyDebounce {
    /// `t_us` of the last accepted edge, per pin.
    last_accepted_us: [u32; 32],
    /// Bit `i` set ⇒ pin `i` has an accepted edge recorded (so a first edge
    /// near timer value 0 is never compared against the array default).
    seen: u32,
}

impl KeyDebounce {
    pub const fn new() -> Self {
        Self {
            last_accepted_us: [0; 32],
            seen: 0,
        }
    }

    /// Process an edge captured at `t_us` (ISR time, wrapping 32-bit µs) and
    /// return whether it should be forwarded. Direction-independent: chatter
    /// interleaves press and release edges, so the dead-time applies to any
    /// edge on the pin.
    pub fn accept(&mut self, pin: u8, t_us: u32) -> bool {
        let idx = (pin & 0x1f) as usize;
        let bit = 1u32 << idx;
        if self.seen & bit != 0
            && t_us.wrapping_sub(self.last_accepted_us[idx]) < DEBOUNCE_WINDOW_US
        {
            return false;
        }
        self.seen |= bit;
        self.last_accepted_us[idx] = t_us;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_edge_always_accepted() {
        let mut d = KeyDebounce::new();
        // Near timer 0 and inside what would be the window of the array
        // default — must still be accepted thanks to the `seen` bitmap.
        assert!(d.accept(15, 3_000));
    }

    /// The worst observed storm — Y (GP15) release chatter, log lines
    /// 294-300 of the tuning session: an accepted release followed by six
    /// edges within 1.6 ms. All chatter must be dropped.
    #[test]
    fn y_button_release_storm_collapses_to_first_edge() {
        let mut d = KeyDebounce::new();
        let t0 = 1_000_000;
        assert!(d.accept(15, t0)); // the real release
        for off in [254, 719, 978, 1_354, 1_356, 1_607] {
            assert!(!d.accept(15, t0 + off), "chatter at +{off}us must drop");
        }
    }

    /// The mid-hold glitch — B (GP13) log lines 338-342: press, then a
    /// spurious falling edge 105 ms later followed by release chatter. The
    /// glitch edge is outside any sane window (accepted); its trailing
    /// chatter is not.
    #[test]
    fn b_button_mid_hold_glitch_vector() {
        let mut d = KeyDebounce::new();
        let t0 = 2_000_000;
        assert!(d.accept(13, t0)); // PRS
        assert!(d.accept(13, t0 + 105_062)); // spurious PRS — real gap, passes
        assert!(!d.accept(13, t0 + 105_085)); // REL chatter
        assert!(!d.accept(13, t0 + 105_268)); // REL chatter
    }

    /// Everything legitimate observed in the session must pass: PDB
    /// synthetic press width (39.7 ms), the fastest deliberate re-press
    /// (45.7 ms), and the 19.1 ms mechanical re-actuation (a real press with
    /// a 131.8 ms hold — deliberately NOT treated as bounce).
    #[test]
    fn legitimate_gaps_all_pass() {
        for gap in [19_125u32, 39_736, 45_745, 74_000] {
            let mut d = KeyDebounce::new();
            let t0 = 500_000;
            assert!(d.accept(12, t0));
            assert!(d.accept(12, t0 + gap), "legit gap {gap}us must pass");
        }
    }

    /// Rejected edges must not extend the window: edges every 3 ms means the
    /// third edge (at +6 ms from the last ACCEPTED one) passes. A
    /// retriggerable window would suppress input forever under continuous
    /// chatter.
    #[test]
    fn window_is_non_retriggerable() {
        let mut d = KeyDebounce::new();
        let t0 = 100_000;
        assert!(d.accept(14, t0));
        assert!(!d.accept(14, t0 + 3_000));
        assert!(d.accept(14, t0 + 6_000));
    }

    #[test]
    fn boundary_is_exclusive_below_window() {
        let mut d = KeyDebounce::new();
        let t0 = 100_000;
        assert!(d.accept(12, t0));
        assert!(!d.accept(12, t0 + DEBOUNCE_WINDOW_US - 1));
        let mut d = KeyDebounce::new();
        assert!(d.accept(12, t0));
        assert!(d.accept(12, t0 + DEBOUNCE_WINDOW_US));
    }

    #[test]
    fn pins_are_independent() {
        let mut d = KeyDebounce::new();
        assert!(d.accept(12, 1_000_000));
        // A different pin inside pin 12's window is unaffected.
        assert!(d.accept(13, 1_000_100));
        assert!(!d.accept(12, 1_000_200));
        assert!(!d.accept(13, 1_000_300));
    }

    /// `t_us` is TIMERAWL's low word and wraps every ~71.6 min; deltas that
    /// span the wrap must still measure correctly via `wrapping_sub`.
    #[test]
    fn timer_wraparound_measures_correctly() {
        let mut d = KeyDebounce::new();
        assert!(d.accept(15, u32::MAX - 1_000));
        // 1.5 ms later in real time, past the wrap: still chatter.
        assert!(!d.accept(15, 500));
        // 11 ms after the accepted edge: passes.
        assert!(d.accept(15, 10_000));
    }

    #[test]
    fn pin_index_wraps_at_32() {
        let mut d = KeyDebounce::new();
        assert!(d.accept(3, 1_000_000));
        // pin 35 aliases to bit 3 via `& 0x1f` — same non-disambiguation as
        // KeyPressFilter; GPIO numbers fit in 5 bits on the RP.
        assert!(!d.accept(3 + 32, 1_000_100));
    }
}
