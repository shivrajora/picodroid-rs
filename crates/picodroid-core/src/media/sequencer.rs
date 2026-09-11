// SPDX-License-Identifier: GPL-3.0-only
//! The tone sequencer: which frequency should be sounding, and when it changes.
//!
//! One state machine serves both halves of `ToneGenerator`. A built-in tone
//! loads its segments from [`super::tone_table`]; `startToneSequence` loads the
//! app's notes. Nothing downstream can tell the two apart, which is what makes
//! the picodroid extension almost free.
//!
//! Segments are *copied in* rather than borrowed. The table's entries are
//! `&'static`, but an app sequence lives in the JVM heap and must not be held
//! across a garbage collection, so copying is what lets the native call return
//! without rooting anything. It also caps the cost at a fixed
//! [`MAX_SEGMENTS`]-entry array in `.bss` with no allocation anywhere.
//!
//! Time arrives from the caller rather than a clock of its own, so the whole
//! machine is testable on the host.

use super::tone_table::{Segment, ToneDescriptor, REPEAT_FOREVER};

/// Longest sequence the machine holds. Mirrored by
/// `ToneGenerator.MAX_SEQUENCE_LENGTH`, which the test at the bottom pins to
/// this number by reading the Java source.
pub const MAX_SEGMENTS: usize = 32;

/// What the driver should do with the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Sound this frequency. 0 means silence.
    Play(u16),
    /// The tone is over; silence the output.
    Done,
}

/// Plays a segment list against a millisecond clock.
///
/// [`Self::advance`] returns `Some` only at a boundary, so a caller ticking at
/// LVGL's rate does nothing at all for most ticks and touches the PWM hardware
/// once per note.
pub struct Sequencer {
    segments: [Segment; MAX_SEGMENTS],
    len: u8,
    idx: u8,
    repeat_cnt: u8,
    repeat_segment: u8,
    repeats_done: u8,
    /// When the current segment ends. Meaningless while `holding`.
    deadline_ms: u64,
    /// The current segment runs until stopped (a dial tone, a held DTMF digit).
    holding: bool,
    active: bool,
    /// Deadline from `startTone`'s `durationMs`, which truncates the tone
    /// whatever its cadence would otherwise do.
    cap_ms: u64,
    has_cap: bool,
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequencer {
    pub const fn new() -> Self {
        Self {
            segments: [Segment::new(0, 0); MAX_SEGMENTS],
            len: 0,
            idx: 0,
            repeat_cnt: 0,
            repeat_segment: 0,
            repeats_done: 0,
            deadline_ms: 0,
            holding: false,
            active: false,
            cap_ms: 0,
            has_cap: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Load a tone from the table and sound its first segment.
    ///
    /// `max_duration_ms` of 0 lets the tone run its natural course.
    pub fn start_tone(
        &mut self,
        tone: &ToneDescriptor,
        now_ms: u64,
        max_duration_ms: u32,
    ) -> Option<Action> {
        self.load(
            tone.segments,
            tone.repeat_cnt,
            tone.repeat_segment,
            now_ms,
            max_duration_ms,
        )
    }

    /// Load an app-supplied note sequence and sound its first note. Plays once.
    ///
    /// Returns `None` without disturbing whatever is currently playing if the
    /// sequence is empty or longer than [`MAX_SEGMENTS`].
    pub fn start_sequence(
        &mut self,
        segments: &[Segment],
        now_ms: u64,
        max_duration_ms: u32,
    ) -> Option<Action> {
        if segments.is_empty() || segments.len() > MAX_SEGMENTS {
            return None;
        }
        self.load(segments, 0, 0, now_ms, max_duration_ms)
    }

    fn load(
        &mut self,
        segments: &[Segment],
        repeat_cnt: u8,
        repeat_segment: u8,
        now_ms: u64,
        max_duration_ms: u32,
    ) -> Option<Action> {
        if segments.is_empty() || segments.len() > MAX_SEGMENTS {
            return None;
        }
        self.segments[..segments.len()].copy_from_slice(segments);
        self.len = segments.len() as u8;
        self.idx = 0;
        self.repeat_cnt = repeat_cnt;
        // A repeat point past the end would loop on nothing. The table is
        // checked by its own test; an out-of-range value here would have to
        // come from a future caller, so clamp rather than trust.
        self.repeat_segment = if (repeat_segment as usize) < segments.len() {
            repeat_segment
        } else {
            0
        };
        self.repeats_done = 0;
        self.active = true;
        self.has_cap = max_duration_ms > 0;
        self.cap_ms = now_ms.saturating_add(max_duration_ms as u64);
        self.arm(now_ms);
        Some(Action::Play(self.segments[0].freq_hz))
    }

    /// Silence the output and forget the tone.
    pub fn stop(&mut self) {
        self.active = false;
        self.holding = false;
        self.has_cap = false;
    }

    /// Move the clock forward. `Some` at a segment boundary, `None` otherwise.
    pub fn advance(&mut self, now_ms: u64) -> Option<Action> {
        if !self.active {
            return None;
        }
        // The duration cap outranks the cadence, and outranks a held segment:
        // it is the only thing that ends `startTone(TONE_SUP_DIAL, 500)`.
        if self.has_cap && now_ms >= self.cap_ms {
            self.stop();
            return Some(Action::Done);
        }
        if self.holding || now_ms < self.deadline_ms {
            return None;
        }

        let next = self.idx as usize + 1;
        if next < self.len as usize {
            self.idx = next as u8;
        } else if self.repeat_cnt == REPEAT_FOREVER || self.repeats_done < self.repeat_cnt {
            self.repeats_done = self.repeats_done.saturating_add(1);
            self.idx = self.repeat_segment;
        } else {
            self.stop();
            return Some(Action::Done);
        }

        self.arm(now_ms);
        Some(Action::Play(self.segments[self.idx as usize].freq_hz))
    }

    /// Set the deadline for the segment at `idx`.
    ///
    /// The deadline runs from `now_ms` rather than from the previous deadline.
    /// That lets a late tick shorten nothing and drift a little instead, which
    /// for a buzzer is the right trade: a note that is 16 ms long is audibly
    /// wrong, a melody that runs 16 ms late is not.
    fn arm(&mut self, now_ms: u64) {
        let seg = self.segments[self.idx as usize];
        self.holding = seg.is_held();
        self.deadline_ms = now_ms.saturating_add(seg.duration_ms as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::tone_table::descriptor;

    fn seq(pairs: &[(u16, u16)]) -> alloc::vec::Vec<Segment> {
        pairs.iter().map(|&(f, d)| Segment::new(f, d)).collect()
    }

    #[test]
    fn a_one_shot_tone_plays_then_reports_done() {
        let mut s = Sequencer::new();
        // TONE_PROP_BEEP: 400 Hz for 35 ms, once.
        let beep = descriptor(24).expect("beep");
        assert_eq!(s.start_tone(&beep, 1000, 0), Some(Action::Play(400)));
        assert!(s.is_active());
        assert_eq!(s.advance(1020), None, "still inside the note");
        assert_eq!(s.advance(1035), Some(Action::Done));
        assert!(!s.is_active());
        assert_eq!(s.advance(2000), None, "nothing more after Done");
    }

    #[test]
    fn a_held_tone_never_ends_on_its_own() {
        let mut s = Sequencer::new();
        let dial = descriptor(16).expect("dial");
        assert_eq!(s.start_tone(&dial, 0, 0), Some(Action::Play(425)));
        for t in [1, 100, 10_000, 60_000, 86_400_000] {
            assert_eq!(s.advance(t), None, "dial tone ended by itself at {t}");
        }
        assert!(s.is_active());
        s.stop();
        assert!(!s.is_active());
    }

    #[test]
    fn a_duration_cap_truncates_even_a_held_tone() {
        let mut s = Sequencer::new();
        let dial = descriptor(16).expect("dial");
        assert_eq!(s.start_tone(&dial, 0, 500), Some(Action::Play(425)));
        assert_eq!(s.advance(499), None);
        assert_eq!(s.advance(500), Some(Action::Done));
        assert!(!s.is_active());
    }

    #[test]
    fn a_duration_cap_truncates_a_repeating_tone() {
        let mut s = Sequencer::new();
        // TONE_SUP_RINGTONE repeats forever; 1500 ms should cut it mid-gap.
        let ring = descriptor(23).expect("ringtone");
        assert_eq!(s.start_tone(&ring, 0, 1500), Some(Action::Play(425)));
        assert_eq!(s.advance(1000), Some(Action::Play(0)), "into the 4 s gap");
        assert_eq!(s.advance(1500), Some(Action::Done));
    }

    #[test]
    fn a_repeating_tone_returns_to_its_repeat_segment() {
        let mut s = Sequencer::new();
        // TONE_SUP_BUSY: 425 for 500, silence for 500, forever.
        let busy = descriptor(17).expect("busy");
        assert_eq!(s.start_tone(&busy, 0, 0), Some(Action::Play(425)));
        assert_eq!(s.advance(500), Some(Action::Play(0)));
        assert_eq!(s.advance(1000), Some(Action::Play(425)));
        assert_eq!(s.advance(1500), Some(Action::Play(0)));
        assert_eq!(s.advance(2000), Some(Action::Play(425)));
        assert!(s.is_active(), "a forever tone never finishes by itself");
    }

    #[test]
    fn a_burst_count_plays_exactly_that_many_bursts() {
        let mut s = Sequencer::new();
        // TONE_PROP_ACK: 1200 Hz, 100 on / 100 off, 2 bursts.
        let ack = descriptor(25).expect("ack");
        assert_eq!(s.start_tone(&ack, 0, 0), Some(Action::Play(1200)));
        assert_eq!(s.advance(100), Some(Action::Play(0)));
        assert_eq!(s.advance(200), Some(Action::Play(1200)), "burst 2");
        assert_eq!(s.advance(300), Some(Action::Play(0)));
        assert_eq!(s.advance(400), Some(Action::Done), "no burst 3");
    }

    #[test]
    fn advance_is_silent_between_boundaries() {
        let mut s = Sequencer::new();
        let busy = descriptor(17).expect("busy");
        s.start_tone(&busy, 0, 0);
        // A 16 ms tick across a 500 ms segment should speak once, at the end.
        let mut spoke = 0;
        for t in (1..=500).step_by(16) {
            if s.advance(t).is_some() {
                spoke += 1;
            }
        }
        assert_eq!(spoke, 0, "spoke before the segment ended");
        assert!(s.advance(500).is_some());
    }

    #[test]
    fn an_app_sequence_plays_its_notes_in_order_and_stops() {
        let mut s = Sequencer::new();
        let notes = seq(&[(262, 100), (0, 50), (330, 100)]);
        assert_eq!(s.start_sequence(&notes, 0, 0), Some(Action::Play(262)));
        assert_eq!(s.advance(100), Some(Action::Play(0)), "the rest");
        assert_eq!(s.advance(150), Some(Action::Play(330)));
        assert_eq!(s.advance(250), Some(Action::Done));
        assert!(!s.is_active(), "an app sequence does not repeat");
    }

    #[test]
    fn an_oversized_or_empty_sequence_is_refused() {
        let mut s = Sequencer::new();
        assert_eq!(s.start_sequence(&[], 0, 0), None);
        let too_long = seq(&[(440, 10); MAX_SEGMENTS + 1]);
        assert_eq!(s.start_sequence(&too_long, 0, 0), None);
        assert!(!s.is_active());
    }

    #[test]
    fn a_full_length_sequence_is_accepted() {
        let mut s = Sequencer::new();
        let full = seq(&[(440, 10); MAX_SEGMENTS]);
        assert_eq!(s.start_sequence(&full, 0, 0), Some(Action::Play(440)));
    }

    /// Refusing an oversized sequence must not silence the tone already
    /// playing — `startToneSequence` returning false should change nothing.
    #[test]
    fn a_refused_sequence_leaves_the_current_tone_alone() {
        let mut s = Sequencer::new();
        let dial = descriptor(16).expect("dial");
        s.start_tone(&dial, 0, 0);
        let too_long = seq(&[(440, 10); MAX_SEGMENTS + 1]);
        assert_eq!(s.start_sequence(&too_long, 0, 0), None);
        assert!(s.is_active(), "the dial tone was cut short");
    }

    #[test]
    fn starting_a_tone_replaces_the_one_playing() {
        let mut s = Sequencer::new();
        let ring = descriptor(23).expect("ringtone");
        let beep = descriptor(24).expect("beep");
        assert_eq!(s.start_tone(&ring, 0, 0), Some(Action::Play(425)));
        assert_eq!(s.start_tone(&beep, 10, 0), Some(Action::Play(400)));
        // The ringtone's 1000 ms boundary must be gone, not merely masked.
        assert_eq!(s.advance(45), Some(Action::Done));
    }

    /// The clock is milliseconds since boot and never wraps in practice, but
    /// the arithmetic should not panic in a debug build if it ever did.
    #[test]
    fn a_clock_near_the_end_of_time_does_not_overflow() {
        let mut s = Sequencer::new();
        let ring = descriptor(23).expect("ringtone");
        assert!(s.start_tone(&ring, u64::MAX - 10, 5000).is_some());
        assert_eq!(s.advance(u64::MAX), Some(Action::Done));
    }

    /// `MAX_SEGMENTS` is the size of the array app notes are copied into, and
    /// `ToneGenerator.MAX_SEQUENCE_LENGTH` is what apps are told to respect.
    /// They are the same limit, so a drift between them would have
    /// `startToneSequence` refuse a sequence the Javadoc promised.
    #[test]
    fn max_segments_matches_the_java_constant() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../sdk/java/picodroid/media/ToneGenerator.java"
        );
        let src = std::fs::read_to_string(path).expect("read ToneGenerator.java");
        let needle = "MAX_SEQUENCE_LENGTH = ";
        let at = src.find(needle).expect("MAX_SEQUENCE_LENGTH") + needle.len();
        let rest = &src[at..];
        let end = rest.find(';').expect("terminator");
        let declared: usize = rest[..end].trim().parse().expect("an integer");
        assert_eq!(
            declared, MAX_SEGMENTS,
            "ToneGenerator.MAX_SEQUENCE_LENGTH and sequencer::MAX_SEGMENTS disagree"
        );
    }
}
