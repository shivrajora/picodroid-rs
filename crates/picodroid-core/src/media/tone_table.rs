// SPDX-License-Identifier: GPL-3.0-only
//! The `picodroid.media.ToneGenerator` tone table.
//!
//! Android's tone types are not single frequencies. In AOSP each one is a
//! `ToneDescriptor`: a list of segments carrying frequencies and a duration,
//! plus a repeat count and the segment to repeat from. `TONE_SUP_RINGTONE` is
//! 425 Hz for 1 s, silence for 4 s, forever. So the sequencer the buzzer needs
//! is not an embellishment on `ToneGenerator` — it is what `ToneGenerator` is,
//! and the table below is that structure with the frequencies and cadences of
//! Android's CEPT variant.
//!
//! **One voice.** A piezo on one PWM channel sounds a single square wave, while
//! most of Android's tones are sums of two or three sine components. Each entry
//! here keeps the *lowest* component. The rule is arbitrary but it is at least
//! consistent, and the tones stay recognisable. It does mean the sixteen DTMF
//! digits collapse onto their four row frequencies and will not decode as DTMF;
//! `ToneGenerator`'s Javadoc says so plainly rather than implying otherwise.
//!
//! Pure data and arithmetic, no HAL: this module is compiled and tested on the
//! host through the shim in `lib.rs`.

/// One step of a tone.
///
/// `freq_hz` of 0 is silence, which is how the off half of a cadence is
/// spelled. `duration_ms` of 0 means hold until something stops it — Android's
/// continuous tones (a dial tone, a held DTMF digit) rather than a zero-length
/// step. AOSP spells the same thing `ULONG_MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub freq_hz: u16,
    pub duration_ms: u16,
}

impl Segment {
    pub const fn new(freq_hz: u16, duration_ms: u16) -> Self {
        Self {
            freq_hz,
            duration_ms,
        }
    }

    /// Whether this step runs until stopped rather than for a fixed time.
    pub const fn is_held(&self) -> bool {
        self.duration_ms == 0
    }
}

/// A whole tone: its steps, and how they repeat.
///
/// After the last segment, playback returns to `repeat_segment` while fewer
/// than `repeat_cnt` repeats have been done. [`REPEAT_FOREVER`] never stops,
/// which is AOSP's `TONEGEN_INF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneDescriptor {
    pub segments: &'static [Segment],
    pub repeat_cnt: u8,
    pub repeat_segment: u8,
}

/// A `repeat_cnt` meaning "until stopped".
pub const REPEAT_FOREVER: u8 = u8::MAX;

const fn once(segments: &'static [Segment]) -> ToneDescriptor {
    ToneDescriptor {
        segments,
        repeat_cnt: 0,
        repeat_segment: 0,
    }
}

const fn repeating(segments: &'static [Segment], repeat_cnt: u8) -> ToneDescriptor {
    ToneDescriptor {
        segments,
        repeat_cnt,
        repeat_segment: 0,
    }
}

// --- DTMF (tone types 0-15) -------------------------------------------------
//
// Held until stopped, as on Android. The frequency is the row component of the
// standard pair; see the module note on why the column is lost.
const DTMF_ROW_697: &[Segment] = &[Segment::new(697, 0)];
const DTMF_ROW_770: &[Segment] = &[Segment::new(770, 0)];
const DTMF_ROW_852: &[Segment] = &[Segment::new(852, 0)];
const DTMF_ROW_941: &[Segment] = &[Segment::new(941, 0)];

// --- Supervisory (16-23), CEPT ---------------------------------------------
const SUP_DIAL: &[Segment] = &[Segment::new(425, 0)];
const SUP_BUSY: &[Segment] = &[Segment::new(425, 500), Segment::new(0, 500)];
const SUP_CONGESTION: &[Segment] = &[Segment::new(425, 200), Segment::new(0, 200)];
const SUP_RADIO_ACK: &[Segment] = &[Segment::new(425, 200)];
const SUP_RADIO_NOTAVAIL: &[Segment] = &[Segment::new(425, 200), Segment::new(0, 200)];
// 950 + 1400 + 1800 Hz on Android; the lowest survives.
const SUP_ERROR: &[Segment] = &[Segment::new(950, 330), Segment::new(0, 1000)];
const SUP_CALL_WAITING: &[Segment] = &[
    Segment::new(425, 200),
    Segment::new(0, 600),
    Segment::new(425, 200),
    Segment::new(0, 3000),
];
const SUP_RINGTONE: &[Segment] = &[Segment::new(425, 1000), Segment::new(0, 4000)];
// 350 + 440 Hz on Android.
const SUP_CONFIRM: &[Segment] = &[Segment::new(350, 100), Segment::new(0, 100)];
const SUP_PIP: &[Segment] = &[Segment::new(480, 100), Segment::new(0, 100)];

// --- Proprietary (24-28) ----------------------------------------------------
// BEEP, PROMPT and BEEP2 are 400 + 1200 Hz on Android; NACK is 300 + 400 + 500.
const PROP_BEEP: &[Segment] = &[Segment::new(400, 35)];
const PROP_ACK: &[Segment] = &[Segment::new(1200, 100), Segment::new(0, 100)];
const PROP_NACK: &[Segment] = &[Segment::new(300, 400)];
const PROP_PROMPT: &[Segment] = &[Segment::new(400, 200)];
const PROP_BEEP2: &[Segment] = &[Segment::new(400, 35), Segment::new(0, 200)];

/// The tone for a `ToneGenerator.TONE_*` value, or `None` for one this board
/// does not implement — the CDMA range and the intercept tones, which
/// `startTone` reports as `false` exactly as Android does for a tone its
/// platform lacks.
pub fn descriptor(tone_type: i32) -> Option<ToneDescriptor> {
    // A burst count of N is N-1 repeats: the first pass is not a repeat.
    let d = match tone_type {
        // DTMF, in ToneGenerator's constant order: 0, 1-9, S, P, A, B, C, D.
        0 => once(DTMF_ROW_941),
        1..=3 => once(DTMF_ROW_697),
        4..=6 => once(DTMF_ROW_770),
        7..=9 => once(DTMF_ROW_852),
        10 | 11 => once(DTMF_ROW_941),
        12 => once(DTMF_ROW_697),
        13 => once(DTMF_ROW_770),
        14 => once(DTMF_ROW_852),
        15 => once(DTMF_ROW_941),

        16 => once(SUP_DIAL),
        17 => repeating(SUP_BUSY, REPEAT_FOREVER),
        18 => repeating(SUP_CONGESTION, REPEAT_FOREVER),
        19 => once(SUP_RADIO_ACK),
        20 => repeating(SUP_RADIO_NOTAVAIL, 2),
        21 => repeating(SUP_ERROR, REPEAT_FOREVER),
        22 => repeating(SUP_CALL_WAITING, REPEAT_FOREVER),
        23 => repeating(SUP_RINGTONE, REPEAT_FOREVER),

        24 => once(PROP_BEEP),
        25 => repeating(PROP_ACK, 1),
        26 => once(PROP_NACK),
        27 => once(PROP_PROMPT),
        28 => repeating(PROP_BEEP2, 1),

        32 => repeating(SUP_CONFIRM, 2),
        33 => repeating(SUP_PIP, 3),

        _ => return None,
    };
    Some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dial_tone_is_held_until_stopped() {
        let d = descriptor(16).expect("TONE_SUP_DIAL");
        assert_eq!(d.segments.len(), 1);
        assert_eq!(d.segments[0].freq_hz, 425);
        assert!(d.segments[0].is_held());
        assert_eq!(d.repeat_cnt, 0);
    }

    #[test]
    fn ringtone_repeats_forever_with_a_four_second_gap() {
        let d = descriptor(23).expect("TONE_SUP_RINGTONE");
        assert_eq!(
            d.segments,
            &[Segment::new(425, 1000), Segment::new(0, 4000)]
        );
        assert_eq!(d.repeat_cnt, REPEAT_FOREVER);
        assert_eq!(d.repeat_segment, 0);
    }

    /// Android describes TONE_PROP_ACK as "2 bursts", which is one repeat of a
    /// two-segment cadence rather than two.
    #[test]
    fn two_burst_tones_repeat_once() {
        assert_eq!(descriptor(25).expect("TONE_PROP_ACK").repeat_cnt, 1);
        assert_eq!(descriptor(28).expect("TONE_PROP_BEEP2").repeat_cnt, 1);
    }

    #[test]
    fn burst_counts_match_androids_descriptions() {
        // "200ms ON, 200 OFF 3 bursts"
        assert_eq!(
            descriptor(20).expect("TONE_SUP_RADIO_NOTAVAIL").repeat_cnt,
            2
        );
        // "repeated 3 times in a 100 ms on, 100 ms off cycle"
        assert_eq!(descriptor(32).expect("TONE_SUP_CONFIRM").repeat_cnt, 2);
        // "four bursts of 480 Hz tone (0.1 s on, 0.1 s off)"
        assert_eq!(descriptor(33).expect("TONE_SUP_PIP").repeat_cnt, 3);
    }

    #[test]
    fn dtmf_digits_use_their_row_frequency() {
        // Row 1: 1, 2, 3 and A all sit on 697 Hz once the column is dropped.
        for t in [1, 2, 3, 12] {
            assert_eq!(descriptor(t).expect("dtmf").segments[0].freq_hz, 697);
        }
        assert_eq!(descriptor(4).expect("dtmf 4").segments[0].freq_hz, 770);
        assert_eq!(descriptor(7).expect("dtmf 7").segments[0].freq_hz, 852);
        // 0, *, # and D are the bottom row.
        for t in [0, 10, 11, 15] {
            assert_eq!(descriptor(t).expect("dtmf").segments[0].freq_hz, 941);
        }
    }

    #[test]
    fn every_dtmf_and_named_tone_resolves() {
        for t in 0..=28 {
            assert!(descriptor(t).is_some(), "tone {t} should be defined");
        }
        assert!(descriptor(32).is_some());
        assert!(descriptor(33).is_some());
    }

    #[test]
    fn unimplemented_and_invalid_tones_are_none() {
        // The intercept tones and the CDMA range are deliberately absent.
        for t in [29, 30, 31, 34, 98] {
            assert!(descriptor(t).is_none(), "tone {t} should be undefined");
        }
        assert!(descriptor(-1).is_none());
        assert!(descriptor(i32::MAX).is_none());
    }

    /// A repeat point past the end would loop on nothing.
    #[test]
    fn repeat_segment_is_always_within_the_tone() {
        for t in 0..=33 {
            if let Some(d) = descriptor(t) {
                assert!(
                    (d.repeat_segment as usize) < d.segments.len(),
                    "tone {t} repeats from {} of {} segments",
                    d.repeat_segment,
                    d.segments.len()
                );
            }
        }
    }

    /// A repeating tone whose every segment is held would never advance.
    #[test]
    fn repeating_tones_have_a_finite_cycle() {
        for t in 0..=33 {
            if let Some(d) = descriptor(t) {
                if d.repeat_cnt > 0 {
                    assert!(
                        d.segments.iter().all(|s| !s.is_held()),
                        "tone {t} repeats but holds a segment"
                    );
                }
            }
        }
    }
}
