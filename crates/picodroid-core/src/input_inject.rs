// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic input gestures — the shared timing and stepping behind
//! `pdb input …` on hardware and `input …` on the simulator's control
//! channel.
//!
//! Both are the picodroid analog of `adb shell input tap|swipe|keyevent`, and
//! both existed already, separately, with hand-matched constants: a 40 ms key
//! edge gap, a 120 ms tap hold, an 80 ms settle, a 40 ms swipe-down settle and
//! twelve interpolation steps, plus a byte-identical `lerp`. The simulator's
//! copy even carried a comment saying it mirrored the device's.
//!
//! That is the wrong thing to duplicate. These numbers are not arbitrary —
//! they are tuned against how the input pipeline samples (see each constant) —
//! and a simulator that used different ones would report a different answer
//! than the hardware for the same script, which is the one thing a simulator
//! must not do.
//!
//! # Why a trait rather than direct calls
//!
//! The two callers reach the same HAL by different routes. The PDB handler
//! goes through [`crate::hal`]'s facade, which is correct on a device. The
//! simulator's front-end must *not*: inside this crate the facade routes back
//! out through the platform's registration and straight into the simulator
//! functions it started from, so it calls its siblings directly. [`InputSink`]
//! lets one implementation serve both without either paying for the other's
//! routing.

/// Gap between a key PRESS and its RELEASE so the two edges land in distinct
/// LVGL ticks — the keypad indev drains one edge per read, so a shorter gap
/// can lose the release entirely.
pub const KEY_EDGE_GAP_MS: u32 = 40;

/// Hold before a tap's release. On a resistive panel the first reading after
/// touch-down is discarded as unsettled (`hal::touch_sampler::sample_panel`),
/// so a press must survive ≥2 poll cycles to register as `ACTION_DOWN` at all
/// — and where the panel is sampled per rendered frame rather than on the
/// sampler's own 10 ms timer, a poll cycle is a whole frame.
pub const TAP_HOLD_MS: u32 = 120;

/// Settle after releasing, before clearing the override — long enough for the
/// release to be sampled as `ACTION_UP` rather than swallowed by the clear.
pub const TOUCH_SETTLE_MS: u32 = 80;

/// Settle after a swipe's initial press, so `ACTION_DOWN` registers before
/// the first `ACTION_MOVE` arrives.
pub const SWIPE_DOWN_SETTLE_MS: u32 = 40;

/// Intermediate MOVE samples across a swipe. Enough that a gesture detector
/// sees a direction rather than a teleport.
pub const SWIPE_STEPS: u32 = 12;

/// Default swipe duration when a caller does not give one.
pub const SWIPE_DEFAULT_MS: u32 = 300;

/// How long `keyevent --longpress` holds the key: past the repeat timeout
/// that marks the long-press (`key_repeat.rs`), with margin for the 16 ms
/// tick and for a UI task that is mid-transition when the timeout falls. A
/// finger held this long also produces a couple of ordinary repeats on the
/// way, exactly as on Android; an app handles those anyway.
pub const LONG_PRESS_HOLD_MS: u32 =
    crate::graphics::lvgl::key_repeat::KEY_REPEAT_TIMEOUT_MS as u32 + 150;

/// Settle after a key verb's release, before the next verb. Two queued verbs
/// would otherwise put a release and the next press microseconds apart, and
/// the contact debounce (`key_debounce.rs`, 5 ms) would eat the press — whose
/// release then fails the press-state filter too, and the whole verb is lost.
/// One edge gap keeps the next press in a later tick as well.
pub const KEY_SETTLE_MS: u32 = KEY_EDGE_GAP_MS;

/// What a key verb does with its key — the `[--longpress|--down|--up]` of
/// `input keyevent`, and the PDB `KEY_META_*` byte. `--longpress` is
/// Android's flag; `--down` / `--up` are this framework's, for holding a key
/// across other verbs (auto-repeat QA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyHold {
    PressRelease,
    LongPress,
    Down,
    Up,
}

impl KeyHold {
    /// Parse one `--flag` token of a key verb; `None` for anything else.
    pub fn from_flag(tok: &str) -> Option<Self> {
        match tok {
            "--longpress" => Some(Self::LongPress),
            "--down" => Some(Self::Down),
            "--up" => Some(Self::Up),
            _ => None,
        }
    }
}

/// Where injected input goes.
///
/// Associated functions rather than methods: every implementation is a
/// stateless route to a HAL, and the callers are static dispatch sites.
pub trait InputSink {
    /// Drive a button pin. Boards wire buttons active-low, so a press is
    /// `rising = false`.
    fn gpio_inject(pin: u8, rising: bool);
    /// Hold the touch panel at a point.
    fn touch_set(x: u16, y: u16);
    /// Lift, leaving the last point readable until [`touch_clear`].
    fn touch_release();
    /// Stop overriding; real hardware reads resume.
    fn touch_clear();
    fn delay_ms(ms: u32);
}

/// Clamp a host-sent coordinate into `[0, max - 1]`.
pub fn clamp_coord(v: i32, max: u16) -> u16 {
    v.clamp(0, max.saturating_sub(1) as i32) as u16
}

/// Linear interpolation between two on-screen coordinates.
///
/// Both ends are valid `u16`, so the result stays within their range and the
/// cast back is lossless.
pub fn lerp(a: u16, b: u16, i: u32, n: u32) -> u16 {
    let (a, b) = (a as i32, b as i32);
    (a + (b - a) * i as i32 / n as i32) as u16
}

/// Press and release a button.
pub fn press_release<S: InputSink>(pin: u8) {
    S::gpio_inject(pin, false);
    S::delay_ms(KEY_EDGE_GAP_MS);
    S::gpio_inject(pin, true);
}

/// Press a button without releasing it.
pub fn press<S: InputSink>(pin: u8) {
    S::gpio_inject(pin, false);
}

/// Release a held button.
pub fn release<S: InputSink>(pin: u8) {
    S::gpio_inject(pin, true);
}

/// Press, hold past the long-press timeout, release — Android's
/// `input keyevent --longpress`.
pub fn long_press<S: InputSink>(pin: u8) {
    S::gpio_inject(pin, false);
    S::delay_ms(LONG_PRESS_HOLD_MS);
    S::gpio_inject(pin, true);
}

/// Drive `pin` as `hold` says — one verb, settled after its release so the
/// next verb's press is a distinct edge.
pub fn key<S: InputSink>(pin: u8, hold: KeyHold) {
    match hold {
        KeyHold::PressRelease => press_release::<S>(pin),
        KeyHold::LongPress => long_press::<S>(pin),
        KeyHold::Down => press::<S>(pin),
        KeyHold::Up => release::<S>(pin),
    }
    if hold != KeyHold::Down {
        S::delay_ms(KEY_SETTLE_MS);
    }
}

/// Tap once at a point, holding long enough to be sampled.
pub fn tap<S: InputSink>(x: u16, y: u16) {
    S::touch_set(x, y);
    S::delay_ms(TAP_HOLD_MS);
    S::touch_release();
    S::delay_ms(TOUCH_SETTLE_MS);
    S::touch_clear();
}

/// Swipe from one point to another over `duration_ms`.
pub fn swipe<S: InputSink>(x1: u16, y1: u16, x2: u16, y2: u16, duration_ms: u32) {
    // At least 1 ms per step: a zero delay would emit every sample inside one
    // poll cycle, which the pipeline sees as a teleport rather than a drag.
    let step_ms = (duration_ms / SWIPE_STEPS).max(1);

    S::touch_set(x1, y1);
    S::delay_ms(SWIPE_DOWN_SETTLE_MS);
    for i in 1..=SWIPE_STEPS {
        S::touch_set(lerp(x1, x2, i, SWIPE_STEPS), lerp(y1, y2, i, SWIPE_STEPS));
        S::delay_ms(step_ms);
    }
    S::touch_release();
    S::delay_ms(TOUCH_SETTLE_MS);
    S::touch_clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, PartialEq, Eq)]
    enum Ev {
        Gpio(u8, bool),
        Set(u16, u16),
        Release,
        Clear,
        Delay(u32),
    }

    static LOG: Mutex<Vec<Ev>> = Mutex::new(Vec::new());
    static SERIAL: Mutex<()> = Mutex::new(());

    struct Rec;
    impl InputSink for Rec {
        fn gpio_inject(pin: u8, rising: bool) {
            LOG.lock().unwrap().push(Ev::Gpio(pin, rising))
        }
        fn touch_set(x: u16, y: u16) {
            LOG.lock().unwrap().push(Ev::Set(x, y))
        }
        fn touch_release() {
            LOG.lock().unwrap().push(Ev::Release)
        }
        fn touch_clear() {
            LOG.lock().unwrap().push(Ev::Clear)
        }
        fn delay_ms(ms: u32) {
            LOG.lock().unwrap().push(Ev::Delay(ms))
        }
    }

    fn record(f: impl FnOnce()) -> Vec<Ev> {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        LOG.lock().unwrap().clear();
        f();
        core::mem::take(&mut *LOG.lock().unwrap())
    }

    /// Buttons are active-low, so a press is the falling edge. Getting this
    /// backwards inverts every injected key.
    #[test]
    fn a_key_press_is_the_falling_edge_and_the_release_the_rising() {
        assert_eq!(
            record(|| press_release::<Rec>(7)),
            vec![
                Ev::Gpio(7, false),
                Ev::Delay(KEY_EDGE_GAP_MS),
                Ev::Gpio(7, true)
            ]
        );
    }

    /// The gap exists because the keypad indev drains one edge per read;
    /// without it the release can be lost, which reads as a stuck key.
    #[test]
    fn the_two_key_edges_are_separated() {
        let evs = record(|| press_release::<Rec>(1));
        assert!(matches!(evs[1], Ev::Delay(ms) if ms > 0));
    }

    /// A long-press is a real hold: the release comes after the repeat
    /// timeout has passed, so the dispatcher has synthesised the long-press
    /// repeat by then.
    #[test]
    fn a_long_press_holds_past_the_repeat_timeout() {
        let evs = record(|| long_press::<Rec>(3));
        assert_eq!(evs[0], Ev::Gpio(3, false));
        assert!(matches!(evs[1], Ev::Delay(ms)
            if ms as u64 > crate::graphics::lvgl::key_repeat::KEY_REPEAT_TIMEOUT_MS));
        assert_eq!(evs[2], Ev::Gpio(3, true));
    }

    #[test]
    fn the_hold_flags_parse_and_drive_the_matching_edges() {
        assert_eq!(KeyHold::from_flag("--longpress"), Some(KeyHold::LongPress));
        assert_eq!(KeyHold::from_flag("--down"), Some(KeyHold::Down));
        assert_eq!(KeyHold::from_flag("--up"), Some(KeyHold::Up));
        assert_eq!(KeyHold::from_flag("--sideways"), None);
        assert_eq!(
            record(|| key::<Rec>(5, KeyHold::Down)),
            vec![Ev::Gpio(5, false)]
        );
        assert_eq!(
            record(|| key::<Rec>(5, KeyHold::Up)),
            vec![Ev::Gpio(5, true), Ev::Delay(KEY_SETTLE_MS)]
        );
        let mut expect = record(|| press_release::<Rec>(5));
        expect.push(Ev::Delay(KEY_SETTLE_MS));
        assert_eq!(record(|| key::<Rec>(5, KeyHold::PressRelease)), expect);
    }

    /// A verb's release is followed by a settle longer than the contact
    /// debounce, so a queued next verb's press is not eaten as chatter.
    #[test]
    fn a_key_verb_settles_past_the_debounce_after_its_release() {
        let evs = record(|| key::<Rec>(5, KeyHold::LongPress));
        assert!(matches!(evs.last(), Some(Ev::Delay(ms))
            if *ms * 1_000 > crate::graphics::lvgl::key_debounce::DEBOUNCE_WINDOW_US));
    }

    #[test]
    fn a_tap_holds_then_releases_then_clears() {
        assert_eq!(
            record(|| tap::<Rec>(10, 20)),
            vec![
                Ev::Set(10, 20),
                Ev::Delay(TAP_HOLD_MS),
                Ev::Release,
                Ev::Delay(TOUCH_SETTLE_MS),
                Ev::Clear,
            ]
        );
    }

    /// A swipe must land exactly on its endpoint — a stepper that stopped one
    /// short would drag *near* the target, which for a fling or a list scroll
    /// is a different gesture.
    #[test]
    fn a_swipe_ends_exactly_on_its_endpoint() {
        let evs = record(|| swipe::<Rec>(0, 0, 100, 50, 120));
        let last_set = evs
            .iter()
            .rev()
            .find_map(|e| match e {
                Ev::Set(x, y) => Some((*x, *y)),
                _ => None,
            })
            .unwrap();
        assert_eq!(last_set, (100, 50));
    }

    #[test]
    fn a_swipe_starts_at_its_origin_and_steps_the_declared_number_of_times() {
        let evs = record(|| swipe::<Rec>(5, 5, 25, 25, 120));
        assert_eq!(evs[0], Ev::Set(5, 5));
        let sets = evs.iter().filter(|e| matches!(e, Ev::Set(..))).count();
        assert_eq!(sets as u32, SWIPE_STEPS + 1, "origin plus one per step");
    }

    /// A zero-duration swipe still has to pace itself; emitting every sample
    /// inside one poll cycle reads as a teleport, not a drag.
    #[test]
    fn a_zero_duration_swipe_still_paces_its_steps() {
        let evs = record(|| swipe::<Rec>(0, 0, 10, 10, 0));
        assert!(evs
            .iter()
            .any(|e| matches!(e, Ev::Delay(ms) if *ms >= 1 && *ms < TAP_HOLD_MS)));
    }

    #[test]
    fn interpolation_hits_both_ends() {
        assert_eq!(lerp(10, 20, 0, 12), 10);
        assert_eq!(lerp(10, 20, 12, 12), 20);
        // Backwards is just as valid — a swipe up is a swipe with b < a.
        assert_eq!(lerp(20, 10, 12, 12), 10);
    }

    #[test]
    fn coordinates_clamp_into_the_screen() {
        assert_eq!(clamp_coord(-5, 240), 0);
        assert_eq!(clamp_coord(1000, 240), 239);
        assert_eq!(clamp_coord(100, 240), 100);
        // A degenerate screen must not underflow to u16::MAX.
        assert_eq!(clamp_coord(5, 0), 0);
    }
}
