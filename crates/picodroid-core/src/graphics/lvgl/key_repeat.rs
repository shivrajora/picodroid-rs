// SPDX-License-Identifier: GPL-3.0-only
//! Auto-repeat for held hardware keys — Android's input dispatcher, in one
//! struct: a key held past [`KEY_REPEAT_TIMEOUT_MS`] yields a synthetic
//! `ACTION_DOWN` with `repeatCount` 1, then one more every
//! [`KEY_REPEAT_DELAY_MS`] until the release. The first repeat is the
//! long-press (`KeyEvent.FLAG_LONG_PRESS`); the Java side (`KeyEvent.dispatch`)
//! turns it into `onKeyLongPress` and cancels the release, exactly as
//! Android does, so nothing here knows about tracking or callbacks.
//!
//! Pure data, polled: the main loop calls [`KeyRepeat::press`] /
//! [`KeyRepeat::release`] as it drains the GPIO edge queue and then
//! [`KeyRepeat::next_due`] with the current time, once per tick, so a repeat
//! is late by at most one tick (16 ms). After a stall the engine fires ONE
//! repeat and re-arms from now rather than replaying every missed one — a
//! burst of stale repeats is what a user would least expect from a held key,
//! and it is what Android's dispatcher does too (it schedules the next
//! repeat from the time it actually sent the previous one).
//!
//! Timings match the Java `ViewConfiguration` (`getKeyRepeatTimeout()` /
//! `getKeyRepeatDelay()`); `sdk_view_configuration_agrees` pins the two.

/// Time from a press to its first repeat — Android's
/// `ViewConfiguration.getKeyRepeatTimeout()`, which is its long-press timeout.
pub const KEY_REPEAT_TIMEOUT_MS: u64 = 400;

/// Time between repeats — Android's `ViewConfiguration.getKeyRepeatDelay()`.
pub const KEY_REPEAT_DELAY_MS: u64 = 50;

/// Keys that can be held at once. Boards have at most four buttons; a fifth
/// simultaneous press just does not repeat.
pub const MAX_HELD: usize = 4;

/// One synthetic repeat, ready to dispatch as `ACTION_DOWN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeat {
    pub keycode: i32,
    /// 1 for the first repeat (the long-press), then 2, 3, …
    pub repeat_count: i32,
    /// When the key went down, for `KeyEvent.getDownTime()`.
    pub down_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct Held {
    keycode: i32,
    down_ms: u64,
    repeat_count: i32,
    next_ms: u64,
}

/// The held-key table. `Default`/`new()` is empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyRepeat {
    held: [Option<Held>; MAX_HELD],
}

impl KeyRepeat {
    pub const fn new() -> Self {
        Self {
            held: [None; MAX_HELD],
        }
    }

    /// A key went down at `down_ms`. A key already held (its release was
    /// lost) starts over. Returns `false` when the table is full and the key
    /// will not repeat.
    pub fn press(&mut self, keycode: i32, down_ms: u64) -> bool {
        let slot = match self.find(keycode) {
            Some(i) => Some(i),
            None => self.held.iter().position(Option::is_none),
        };
        match slot {
            Some(i) => {
                self.held[i] = Some(Held {
                    keycode,
                    down_ms,
                    repeat_count: 0,
                    next_ms: down_ms + KEY_REPEAT_TIMEOUT_MS,
                });
                true
            }
            None => false,
        }
    }

    /// A key came up. Returns when it went down, for the release event's
    /// `getDownTime()`; `None` for a release with no press on record.
    pub fn release(&mut self, keycode: i32) -> Option<u64> {
        let i = self.find(keycode)?;
        let down_ms = self.held[i].map(|h| h.down_ms);
        self.held[i] = None;
        down_ms
    }

    /// The next repeat that is due at `now_ms`, if any, re-armed
    /// `KEY_REPEAT_DELAY_MS` from now. Call in a loop until `None`; each key
    /// yields at most one repeat per call sequence at a given `now_ms`.
    pub fn next_due(&mut self, now_ms: u64) -> Option<Repeat> {
        let h = self
            .held
            .iter_mut()
            .flatten()
            .find(|h| now_ms >= h.next_ms)?;
        h.repeat_count += 1;
        h.next_ms = now_ms + KEY_REPEAT_DELAY_MS;
        Some(Repeat {
            keycode: h.keycode,
            repeat_count: h.repeat_count,
            down_ms: h.down_ms,
        })
    }

    /// Forget every held key: between app runs, and when an Activity
    /// transition means the next repeat would land on a screen that never
    /// saw the press.
    pub fn reset(&mut self) {
        self.held = [None; MAX_HELD];
    }

    /// Whether any key is held (so the caller knows a poll can produce work).
    pub fn any_held(&self) -> bool {
        self.held.iter().any(Option::is_some)
    }

    fn find(&self, keycode: i32) -> Option<usize> {
        self.held
            .iter()
            .position(|h| matches!(h, Some(h) if h.keycode == keycode))
    }
}

/// The wall time of a GPIO edge in milliseconds, from the ISR's wrapping
/// 32-bit microsecond stamp and the 64-bit microsecond clock read now. Both
/// come from the same counter (the low word of it in the ISR), so the age is
/// their wrapping difference; an edge can sit in the queue for hundreds of
/// milliseconds while the UI task stalls, and the hold timer must run from
/// when the finger landed, not from when the loop got round to it.
pub fn edge_time_ms(now_us: u64, edge_t_us: u32) -> u64 {
    let age_us = (now_us as u32).wrapping_sub(edge_t_us) as u64;
    now_us.saturating_sub(age_us) / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: i32 = 19;
    const B: i32 = 20;

    #[test]
    fn nothing_repeats_before_the_timeout() {
        let mut r = KeyRepeat::new();
        assert!(r.press(A, 1_000));
        assert_eq!(r.next_due(1_000), None);
        assert_eq!(r.next_due(1_000 + KEY_REPEAT_TIMEOUT_MS - 1), None);
    }

    #[test]
    fn the_first_repeat_is_count_one_at_the_timeout_then_every_delay() {
        let mut r = KeyRepeat::new();
        r.press(A, 1_000);
        let t1 = 1_000 + KEY_REPEAT_TIMEOUT_MS;
        assert_eq!(
            r.next_due(t1),
            Some(Repeat {
                keycode: A,
                repeat_count: 1,
                down_ms: 1_000
            })
        );
        // One per poll: the same instant does not yield a second.
        assert_eq!(r.next_due(t1), None);
        assert_eq!(r.next_due(t1 + KEY_REPEAT_DELAY_MS - 1), None);
        let t2 = t1 + KEY_REPEAT_DELAY_MS;
        assert_eq!(r.next_due(t2).map(|x| x.repeat_count), Some(2));
        assert_eq!(
            r.next_due(t2 + KEY_REPEAT_DELAY_MS).map(|x| x.repeat_count),
            Some(3)
        );
    }

    #[test]
    fn a_release_stops_the_repeats_and_reports_the_down_time() {
        let mut r = KeyRepeat::new();
        r.press(A, 500);
        assert_eq!(r.release(A), Some(500));
        assert_eq!(r.next_due(500 + KEY_REPEAT_TIMEOUT_MS * 2), None);
        assert!(!r.any_held());
    }

    #[test]
    fn a_release_with_no_press_is_not_an_error() {
        let mut r = KeyRepeat::new();
        assert_eq!(r.release(A), None);
    }

    /// A press and release that were both queued behind a stall are drained
    /// in the same tick: the key is no longer held when the poll runs, so no
    /// repeat is synthesised however long ago the press was.
    #[test]
    fn a_press_and_release_drained_together_never_repeat() {
        let mut r = KeyRepeat::new();
        r.press(A, 0);
        r.release(A);
        assert_eq!(r.next_due(10_000), None);
    }

    /// After a stall the engine fires once and re-arms from now — no burst
    /// of the repeats that would have happened during the stall.
    #[test]
    fn a_stall_yields_one_repeat_and_rearms_from_now() {
        let mut r = KeyRepeat::new();
        r.press(A, 0);
        let late = KEY_REPEAT_TIMEOUT_MS + 10 * KEY_REPEAT_DELAY_MS;
        assert_eq!(r.next_due(late).map(|x| x.repeat_count), Some(1));
        assert_eq!(r.next_due(late), None);
        assert_eq!(r.next_due(late + KEY_REPEAT_DELAY_MS - 1), None);
        assert_eq!(
            r.next_due(late + KEY_REPEAT_DELAY_MS)
                .map(|x| x.repeat_count),
            Some(2)
        );
    }

    #[test]
    fn two_held_keys_repeat_independently() {
        let mut r = KeyRepeat::new();
        r.press(A, 0);
        r.press(B, 100);
        let t = KEY_REPEAT_TIMEOUT_MS + 100;
        let mut got = [r.next_due(t).unwrap(), r.next_due(t).unwrap()];
        got.sort_by_key(|x| x.keycode);
        assert_eq!(got[0].keycode, A);
        assert_eq!(got[0].down_ms, 0);
        assert_eq!(got[1].keycode, B);
        assert_eq!(got[1].down_ms, 100);
        assert_eq!(r.next_due(t), None);
        assert_eq!(r.release(B), Some(100));
        assert_eq!(
            r.next_due(t + KEY_REPEAT_DELAY_MS).map(|x| x.keycode),
            Some(A)
        );
        assert_eq!(r.next_due(t + KEY_REPEAT_DELAY_MS), None);
    }

    /// A second press of a held key (its release was lost) starts the hold
    /// over rather than continuing the old count.
    #[test]
    fn a_repeated_press_restarts_the_hold() {
        let mut r = KeyRepeat::new();
        r.press(A, 0);
        assert_eq!(
            r.next_due(KEY_REPEAT_TIMEOUT_MS).map(|x| x.repeat_count),
            Some(1)
        );
        r.press(A, 1_000);
        assert_eq!(r.next_due(1_000 + KEY_REPEAT_TIMEOUT_MS - 1), None);
        let rep = r.next_due(1_000 + KEY_REPEAT_TIMEOUT_MS).unwrap();
        assert_eq!((rep.repeat_count, rep.down_ms), (1, 1_000));
    }

    #[test]
    fn a_fifth_key_does_not_repeat_and_says_so() {
        let mut r = KeyRepeat::new();
        for k in 0..MAX_HELD as i32 {
            assert!(r.press(100 + k, 0));
        }
        assert!(!r.press(200, 0));
        r.release(100);
        assert!(r.press(200, 0));
    }

    #[test]
    fn reset_forgets_every_key() {
        let mut r = KeyRepeat::new();
        r.press(A, 0);
        r.press(B, 0);
        r.reset();
        assert!(!r.any_held());
        assert_eq!(r.next_due(u64::MAX / 2), None);
    }

    #[test]
    fn edge_time_subtracts_the_edges_age() {
        // Edge stamped 250 ms before now, in the ISR's low word.
        let now_us: u64 = 10_000_000_000;
        let t_us = (now_us - 250_000) as u32;
        assert_eq!(edge_time_ms(now_us, t_us), (now_us - 250_000) / 1_000);
    }

    /// The ISR stamp is a wrapping 32-bit word; an edge just before the
    /// wrap read now, just after it, is still a few ms old.
    #[test]
    fn edge_time_survives_the_low_word_wrapping() {
        let now_us: u64 = (1u64 << 32) + 5_000;
        let t_us = u32::MAX - 5_000 + 1; // 10 ms before now
        assert_eq!(edge_time_ms(now_us, t_us), ((1u64 << 32) - 5_000) / 1_000);
    }

    /// The Java ViewConfiguration must quote the numbers this engine uses;
    /// an app reads them there.
    #[test]
    fn sdk_view_configuration_agrees() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sdk/java/picodroid/view/ViewConfiguration.java"
        ))
        .expect("ViewConfiguration.java");
        let value = |name: &str| -> u64 {
            let line = src
                .lines()
                .find(|l| l.contains(name) && l.contains('='))
                .unwrap_or_else(|| panic!("{name} not declared"));
            line.split('=')
                .nth(1)
                .unwrap()
                .trim()
                .trim_end_matches(';')
                .parse()
                .unwrap()
        };
        assert_eq!(value("LONG_PRESS_TIMEOUT_MS"), KEY_REPEAT_TIMEOUT_MS);
        assert_eq!(value("KEY_REPEAT_DELAY_MS"), KEY_REPEAT_DELAY_MS);
    }
}
