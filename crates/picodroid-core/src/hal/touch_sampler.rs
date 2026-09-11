// SPDX-License-Identifier: GPL-3.0-only
//! Touch sampling — who reads the panel, and how often.
//!
//! The panel used to be read from inside LVGL's input-device callback, which
//! runs once per *rendered frame*. On a board where a frame costs 120-200 ms
//! that samples a finger five times a second: a 300 ms swipe produced two
//! positions 234 px apart, so the page teleported instead of scrolling and the
//! fling inherited a velocity computed from two samples
//! (`docs/designs/scroll-performance-2026-09.md` §1, S1).
//!
//! This module breaks that coupling. A dedicated task reads the panel every
//! [`PERIOD_MS`] and pushes each *changed* reading into a small ring; the
//! input callback drains the whole ring in one pass, handing LVGL one read per
//! queued sample. Rendering stays as slow as it is, but the gesture LVGL sees
//! is the one the finger actually made.
//!
//! # Not every board
//!
//! The task only starts where the controller has a bus of its own
//! (`TOUCH_PRIVATE_BUS`, emitted from `[touch] driver` — see
//! `build_support/config.rs`). A resistive panel on the display's SPI bus
//! cannot be read from a second task: a read drops the bus clock to the
//! panel's frequency and nothing serialises that against a band flush, so
//! today's safety rests entirely on both happening on the UI task. Those
//! boards keep the inline read, through the same [`sample_panel`] used here,
//! and [`running`] stays false. There the ring and the task are not merely
//! unused but absent — `touch_private_bus` gates them out of the image,
//! because the RP2040 board that would otherwise carry them is the one with no
//! flash to spare.
//!
//! # Ordering
//!
//! One producer (the sampler), one consumer (the UI task). `HEAD` and `TAIL`
//! are free-running counters, each written only by its own side; a slot is
//! published by a `Release` store and read by an `Acquire` load, which is what
//! carries the sample across cores on an SMP kernel.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::hal;

// board.toml decides both of these; a build with no `[touch]` still gets them
// (both false), because this module compiles on every board.
#[allow(dead_code)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/touch_config.rs"));
}

/// Whether the previous panel reading was a press. Owned by whoever samples,
/// which is exactly one task either way.
static WAS_PRESSED: AtomicBool = AtomicBool::new(false);

// ── Sampling ────────────────────────────────────────────────────────────────

/// Read the panel once, applying the first-sample discard where the controller
/// needs it.
///
/// A resistive panel's first reading after touch-down is taken before its RC
/// network has settled and can be 20-60 px off, so it is reported as "still
/// released" and the next read — one [`PERIOD_MS`] later, not one frame —
/// carries the position. A capacitive controller reports finished pixels, so
/// `TOUCH_DISCARD_FIRST_SAMPLE` is false there and a tap registers on the
/// sample that saw it.
///
/// The caller must be the panel's only reader.
pub fn sample_panel() -> Option<(u16, u16)> {
    match hal::touch::read_point() {
        Some((x, y)) => {
            // Load-then-store, not a swap: the M0+ has no atomic read-modify-
            // write, and it would buy nothing here — exactly one task samples
            // the panel, so nobody else writes this.
            let was_pressed = WAS_PRESSED.load(Ordering::Relaxed);
            WAS_PRESSED.store(true, Ordering::Relaxed);
            if !was_pressed && generated::TOUCH_DISCARD_FIRST_SAMPLE {
                None
            } else {
                Some((x, y))
            }
        }
        None => {
            WAS_PRESSED.store(false, Ordering::Relaxed);
            None
        }
    }
}

// ── The ring, and the task that fills it ─────────────────────────────
//
// Present only where the panel has a bus of its own — and in test builds, so
// that `cargo test` covers the ring on whatever board it happens to be
// configured for.
#[cfg(any(touch_private_bus, test))]
mod ring {
    use super::*;
    use crate::rtos::{self, TaskKind, TaskSpec};
    use core::sync::atomic::{AtomicU32, AtomicUsize};

    /// Sampling period. 100 Hz is the rate a finger needs to be tracked rather
    /// than guessed at, and at ~0.6 ms per read it costs well under 10 % of the
    /// core it preempts. The GT911's INT line is wired and unread; an
    /// interrupt-driven sampler is the end state this period stands in for.
    const PERIOD_MS: u32 = 10;

    /// Queued samples. One frame's worth at the slowest frame measured on the
    /// touch board (348 ms) is ~35, so 64 leaves the ring headroom it should
    /// never need — and it only fills with *changed* readings, a held finger
    /// occupying exactly one slot.
    const RING: usize = 64;

    /// A lifted finger. Packing is `(x << 16) | y`, and no panel reports
    /// `x == 0xFFFF`, so the sentinel cannot collide with a real position.
    const RELEASED: u32 = u32::MAX;

    fn pack(p: Option<(u16, u16)>) -> u32 {
        match p {
            Some((x, y)) => ((x as u32) << 16) | y as u32,
            None => RELEASED,
        }
    }

    fn unpack(v: u32) -> Option<(u16, u16)> {
        if v == RELEASED {
            None
        } else {
            Some(((v >> 16) as u16, (v & 0xFFFF) as u16))
        }
    }

    static SLOTS: [AtomicU32; RING] = [const { AtomicU32::new(RELEASED) }; RING];
    /// Written only by the sampler.
    static HEAD: AtomicUsize = AtomicUsize::new(0);
    /// Written only by the UI task.
    static TAIL: AtomicUsize = AtomicUsize::new(0);
    /// The newest reading, whether or not it is still queued. What a poller
    /// (`Display.pollTouch`, the simulator's tap-debug overlay) wants — they ask
    /// "where is the finger now", not "what happened since you last asked".
    static LATEST: AtomicU32 = AtomicU32::new(RELEASED);
    /// True once the task is up. Until then, and forever on a shared-bus board,
    /// the UI task samples inline.
    static RUNNING: AtomicBool = AtomicBool::new(false);
    /// Producer-side: the last value pushed, so an unmoving finger does not fill
    /// the ring with copies of itself.
    static LAST_PUSHED: AtomicU32 = AtomicU32::new(RELEASED);
    // ── Producer ────────────────────────────────────────────────────────────────

    fn push(sample: Option<(u16, u16)>) {
        let v = pack(sample);
        LATEST.store(v, Ordering::Relaxed);
        // Load-then-store for the same reason as in `sample_panel`: no atomic RMW
        // on the M0+, and this is producer-private state.
        let last = LAST_PUSHED.load(Ordering::Relaxed);
        LAST_PUSHED.store(v, Ordering::Relaxed);
        if last == v {
            // Nothing moved. LVGL is told the current state on every read whether
            // or not the ring has anything in it, so a repeat carries no news.
            return;
        }

        let head = HEAD.load(Ordering::Relaxed);
        let tail = TAIL.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING {
            // A full ring means the UI task has not drained for two thirds of a
            // second. Overwrite the newest slot rather than dropping the sample:
            // the position on screen must be the latest one. That slot is never
            // the one the consumer is about to read (a full ring has 64 entries
            // between tail and head), so the write does not race it.
            SLOTS[head.wrapping_sub(1) % RING].store(v, Ordering::Release);
            return;
        }
        SLOTS[head % RING].store(v, Ordering::Release);
        HEAD.store(head.wrapping_add(1), Ordering::Release);
    }

    fn run() {
        loop {
            push(sample_panel());
            rtos::delay_ms(PERIOD_MS);
        }
    }

    /// Start the sampler, if this board's panel can be read off the UI task.
    ///
    /// Called from the graphics backend's init, after `hal::touch::init` — the
    /// task's first act is to read the panel, so the driver has to exist. Safe to
    /// call more than once; a second call does nothing.
    ///
    /// Declining to spawn is not a failure. `cargo test` has no scheduler and
    /// refuses every task, and a device could be out of arena; the UI task then
    /// samples inline exactly as it did before this module existed.
    pub fn start() {
        if !generated::TOUCH_PRIVATE_BUS || RUNNING.load(Ordering::Relaxed) {
            return;
        }
        let spec = TaskSpec {
            name: "touch",
            kind: TaskKind::Touch,
            priority: crate::task_priority::PRIORITY_TOUCH,
            stack_bytes: None, // platform's Touch default (boot budget)
        };
        if rtos::spawn(&spec, alloc::boxed::Box::new(run)) {
            RUNNING.store(true, Ordering::Relaxed);
        } else {
            crate::pd_warn!("[touch] sampler task did not start — sampling per frame");
        }
    }

    /// Whether the sampler owns the panel. False means the caller must sample it
    /// itself through [`sample_panel`].
    pub fn running() -> bool {
        RUNNING.load(Ordering::Relaxed)
    }

    // ── Consumer ────────────────────────────────────────────────────────────────

    /// Take the oldest queued sample, or `None` when the ring is empty.
    pub fn next() -> Option<Option<(u16, u16)>> {
        let tail = TAIL.load(Ordering::Relaxed);
        if HEAD.load(Ordering::Acquire) == tail {
            return None;
        }
        let v = SLOTS[tail % RING].load(Ordering::Acquire);
        TAIL.store(tail.wrapping_add(1), Ordering::Release);
        Some(unpack(v))
    }

    /// Whether another sample is queued behind the one [`next`] just returned.
    pub fn pending() -> bool {
        HEAD.load(Ordering::Acquire) != TAIL.load(Ordering::Relaxed)
    }

    /// Where the finger is now — the newest reading, queued or already drained.
    ///
    /// For callers that poll rather than consume. When the sampler is not running
    /// this reads the panel directly, which is what they did before.
    pub fn latest() -> Option<(u16, u16)> {
        if running() {
            unpack(LATEST.load(Ordering::Relaxed))
        } else {
            sample_panel()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The ring and the sentinel, not the task — `cargo test` has no
        /// scheduler, so `start` declines and these run against the statics
        /// directly. Serialised because they share them.
        static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

        fn reset() {
            HEAD.store(0, Ordering::Relaxed);
            TAIL.store(0, Ordering::Relaxed);
            LAST_PUSHED.store(RELEASED, Ordering::Relaxed);
            LATEST.store(RELEASED, Ordering::Relaxed);
        }

        #[test]
        fn a_position_survives_the_round_trip() {
            for p in [(0u16, 0u16), (1, 2), (319, 479), (65534, 65534)] {
                assert_eq!(unpack(pack(Some(p))), Some(p));
            }
            assert_eq!(unpack(pack(None)), None);
        }

        /// The released sentinel is only safe because no panel reports that
        /// coordinate. If a screen ever gets that wide the packing has to change.
        #[test]
        fn the_released_sentinel_is_not_a_position() {
            assert_eq!(pack(Some((0xFFFF, 0xFFFF))), RELEASED);
            assert!(pack(Some((0xFFFE, 0xFFFF))) != RELEASED);
        }

        #[test]
        fn samples_come_back_in_the_order_they_were_taken() {
            let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            reset();
            for y in 0..5u16 {
                push(Some((10, y)));
            }
            for y in 0..5u16 {
                assert_eq!(next(), Some(Some((10, y))));
            }
            assert_eq!(next(), None);
            assert!(!pending());
        }

        /// A finger resting on the panel is read 100 times a second and has
        /// nothing to say. Without this the ring would fill with copies and LVGL
        /// would process 64 identical reads per frame.
        #[test]
        fn an_unmoving_finger_queues_one_sample() {
            let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            reset();
            for _ in 0..20 {
                push(Some((7, 9)));
            }
            assert_eq!(next(), Some(Some((7, 9))));
            assert_eq!(next(), None);
        }

        /// Lifting and re-pressing at the same point is two events, not a repeat.
        #[test]
        fn a_lift_is_never_deduplicated_away() {
            let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            reset();
            push(Some((4, 4)));
            push(None);
            push(Some((4, 4)));
            assert_eq!(next(), Some(Some((4, 4))));
            assert_eq!(next(), Some(None));
            assert_eq!(next(), Some(Some((4, 4))));
            assert_eq!(next(), None);
        }

        /// An overrun keeps the newest position rather than the newest *slot*:
        /// where the finger is matters more than one intermediate step of how it
        /// got there.
        #[test]
        fn a_full_ring_keeps_the_latest_position() {
            let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
            reset();
            for y in 0..(RING as u16 + 10) {
                push(Some((0, y)));
            }
            let mut last = None;
            let mut drained = 0;
            while let Some(s) = next() {
                last = s;
                drained += 1;
            }
            assert_eq!(drained, RING);
            assert_eq!(last, Some((0, RING as u16 + 9)));
            // `latest()` would read the panel here (the sampler is not running in
            // a test build), so check what the producer recorded.
            assert_eq!(
                unpack(LATEST.load(Ordering::Relaxed)),
                Some((0, RING as u16 + 9))
            );
        }
    }
}

// A panel that shares the display's bus: nothing to drain, and the UI task
// reads it inline through [`sample_panel`].
#[cfg(not(any(touch_private_bus, test)))]
mod ring {
    pub fn start() {}
    pub fn running() -> bool {
        false
    }
    pub fn next() -> Option<Option<(u16, u16)>> {
        None
    }
    pub fn pending() -> bool {
        false
    }
    pub fn latest() -> Option<(u16, u16)> {
        super::sample_panel()
    }
}

pub use ring::*;
