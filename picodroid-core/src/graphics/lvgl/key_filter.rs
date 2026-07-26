// SPDX-License-Identifier: GPL-3.0-only
// Boards without buttons (`cfg(not(has_buttons))`) include this module but
// never call into it from `events.rs`. The dead_code lint can't see the
// has_buttons-gated callsite, so suppress it at module scope rather than
// duplicating the cfg gate on every item.
#![cfg_attr(not(any(has_buttons, test)), allow(dead_code))]
//! Press-state filter for the button IRQ path.
//!
//! GPIOs that were in an indeterminate state during `enable_edge_irq` arming
//! tend to fire a phantom rising-edge (release) IRQ within the first ~50 ms
//! of boot. On Pico Enviro+ Pack this dispatched BACK and finished the
//! Activity before sensors delivered a second reading. The filter pins the
//! observed invariant: a release for a pin is only forwarded when we
//! previously saw a press for the same pin.
//!
//! Pure data; no `static mut`. The IRQ path in `events.rs` owns one instance
//! behind its own synchronisation; tests can construct any number of
//! `KeyPressFilter`s in isolation.

/// 32-bit press-state bitmap, indexed by `pin & 0x1f`.
#[derive(Default, Copy, Clone)]
pub struct KeyPressFilter {
    /// Bit `i` set ⇒ pin `i` is currently held down.
    pressed_mask: u32,
}

impl KeyPressFilter {
    pub const fn new() -> Self {
        Self { pressed_mask: 0 }
    }

    /// Process an edge IRQ for `pin` and return whether the event should be
    /// forwarded to the consumer queue. Updates the held-state bitmap.
    ///
    /// - `rising = true` (release): forwarded only if the pin was previously
    ///   pressed; the press bit is cleared.
    /// - `rising = false` (press): always forwarded; the press bit is set.
    pub fn observe(&mut self, pin: u8, rising: bool) -> bool {
        let bit = 1u32 << (pin & 0x1f);
        if rising {
            if self.pressed_mask & bit == 0 {
                return false;
            }
            self.pressed_mask &= !bit;
            true
        } else {
            self.pressed_mask |= bit;
            true
        }
    }

    /// True iff the filter currently holds `pin` as pressed.
    #[cfg(test)]
    pub fn is_pressed(&self, pin: u8) -> bool {
        let bit = 1u32 << (pin & 0x1f);
        self.pressed_mask & bit != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression at the heart of `project_button_irq_phantom_release.md`:
    /// a release for a pin that was never pressed must be dropped.
    #[test]
    fn phantom_release_at_boot_is_dropped() {
        let mut f = KeyPressFilter::new();
        assert!(
            !f.observe(15, true),
            "phantom release before any press must not be forwarded"
        );
        assert!(!f.is_pressed(15));
    }

    #[test]
    fn press_is_always_forwarded() {
        let mut f = KeyPressFilter::new();
        assert!(f.observe(12, false));
        assert!(f.is_pressed(12));
    }

    #[test]
    fn press_then_release_round_trip() {
        let mut f = KeyPressFilter::new();
        assert!(f.observe(12, false));
        assert!(f.is_pressed(12));
        assert!(f.observe(12, true));
        assert!(!f.is_pressed(12));
    }

    /// Two successive presses (e.g. a missed release event from a bouncy
    /// switch) should both be forwarded — the consumer queue is the layer
    /// that de-duplicates, not the filter.
    #[test]
    fn double_press_both_forwarded() {
        let mut f = KeyPressFilter::new();
        assert!(f.observe(12, false));
        assert!(f.observe(12, false));
        assert!(f.is_pressed(12));
    }

    /// After a press is paired with a release, a subsequent stray release
    /// (the next phantom IRQ after suspend/resume) must be dropped again.
    #[test]
    fn release_after_release_is_dropped() {
        let mut f = KeyPressFilter::new();
        f.observe(12, false);
        assert!(f.observe(12, true));
        assert!(!f.observe(12, true));
    }

    #[test]
    fn independent_pins_track_independently() {
        let mut f = KeyPressFilter::new();
        f.observe(12, false);
        f.observe(14, false);
        assert!(f.is_pressed(12));
        assert!(f.is_pressed(14));
        assert!(f.observe(12, true));
        assert!(!f.is_pressed(12));
        assert!(f.is_pressed(14), "releasing pin 12 must not affect pin 14");
    }

    /// Pin indices wrap into the low 5 bits. Verifies that the masking
    /// doesn't accidentally collide pins that differ only above the 32nd bit.
    #[test]
    fn pin_index_wraps_at_32() {
        let mut f = KeyPressFilter::new();
        f.observe(3, false);
        // pin 35 aliases to bit 3 via `& 0x1f` — the filter intentionally
        // doesn't disambiguate because GPIO numbers fit in 5 bits on the RP.
        assert!(f.is_pressed(3 + 32));
    }

    #[test]
    fn all_32_pins_can_be_tracked_simultaneously() {
        let mut f = KeyPressFilter::new();
        for pin in 0..32u8 {
            assert!(f.observe(pin, false));
        }
        for pin in 0..32u8 {
            assert!(f.is_pressed(pin));
        }
        for pin in 0..32u8 {
            assert!(f.observe(pin, true));
        }
        for pin in 0..32u8 {
            assert!(!f.is_pressed(pin));
        }
    }

    #[test]
    fn default_matches_new() {
        let a = KeyPressFilter::default();
        let b = KeyPressFilter::new();
        assert_eq!(a.is_pressed(0), b.is_pressed(0));
        assert_eq!(a.is_pressed(15), b.is_pressed(15));
    }
}
