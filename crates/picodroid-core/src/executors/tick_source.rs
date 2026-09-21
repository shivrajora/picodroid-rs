// SPDX-License-Identifier: GPL-3.0-only
//! Periodic 16 ms LVGL tick source feeding the unified main queue.
//!
//! Mirrors Android's split between `Looper` (the dispatcher) and
//! `Choreographer` (the vsync-driven frame source): the main loop in
//! `lifecycle::run_activity` is a pure dispatcher that blocks on
//! `main_queue::recv_blocking`, while this module periodically posts
//! `MainTask::LvglTick` so LVGL animations and widget callbacks tick at a
//! steady cadence.
//!
//! The two backings this file used to carry inline — a FreeRTOS software
//! timer on device, a deadline-paced `std::thread` in the simulator — moved
//! into the platform's [`crate::rtos::Rtos`] implementation. What is left
//! here is the policy: the period, and what a tick does.
//!
//! [`pause`] / [`resume`] let the lifecycle loop quiesce the tick while the
//! display is in low-power sleep. Platforms are expected to genuinely stop
//! their timer rather than filter at the callback — that is what allows the
//! chip to reach a deeper idle state.
//!
//! [`step_ms`] is how many milliseconds a tick advances LVGL's clock (and the
//! toast, snackbar and property-animation clocks that share it). It is the
//! period while the loop keeps up and the measured backlog once it falls
//! behind — see the function for why neither a literal nor a raw wall-clock
//! delta is right.

use crate::rtos;

/// The tick cadence. `LV_DEF_REFR_PERIOD` in `lvgl/lv_conf.h` must equal it
/// (guarded below), and it is the one step `Display.update()` takes.
pub(crate) const TICK_PERIOD_MS: u32 = 16;

/// What the ticks have fed the UI clocks against wall time.
#[derive(Clone, Copy)]
struct TickClock {
    /// Wall time, in ms, up to which the fed steps account: the schedule of
    /// the next tick while the loop keeps up. Meaningless until `started`.
    ///
    /// 32 bits, wrapping, with lateness read as a signed difference: exact
    /// for any gap under 24 days, and a longer pause (a display asleep that
    /// long) merely steps one period on waking. Eight bytes of RAM on a
    /// board that ratchets every one of them, against sixteen for a 64-bit
    /// option.
    fed_ms: u32,
    started: bool,
}

impl TickClock {
    const fn new() -> Self {
        Self {
            fed_ms: 0,
            started: false,
        }
    }

    /// Forget the past: the next step is one period, whatever the gap.
    fn reset(&mut self) {
        self.started = false;
    }

    /// The step for a tick arriving at `now_ms`: one period, or the tick's
    /// lateness against the fed clock when that is more.
    ///
    /// A literal period is wrong when a tick is late: the source coalesces
    /// ticks while the loop is busy, so a 200 ms frame reaches LVGL as 16 ms
    /// and every animation runs slow (audit F16). A raw wall-clock delta is
    /// wrong when the loop keeps up: `lv_timer` stamps `last_run` with the
    /// tick it ran at and carries no credit, so a 15 ms step against the
    /// 16 ms refresh period skips that frame and the next one paints two
    /// periods late (docs/designs/scroll-performance-2026-09.md S3).
    ///
    /// Lateness is measured against the schedule the fed clock keeps, not
    /// against the previous tick, because the timer keeps that schedule too:
    /// a tick drained a few ms late is followed by one on time, and stepping
    /// a period for both is exact. Lateness of a whole period or more means
    /// a tick was coalesced or the timer itself stalled, and that much wall
    /// time is fed in one step. Either way the fed clock never falls behind
    /// the wall clock and never runs more than a period ahead of it.
    fn step(&mut self, now_ms: u32) -> u32 {
        if !self.started {
            self.started = true;
            self.fed_ms = now_ms.wrapping_add(TICK_PERIOD_MS);
            return TICK_PERIOD_MS;
        }
        let late = (now_ms.wrapping_sub(self.fed_ms) as i32).max(0) as u32;
        let step = late.max(TICK_PERIOD_MS);
        self.fed_ms = self.fed_ms.wrapping_add(step);
        step
    }
}

struct TickClockCell(core::cell::Cell<TickClock>);
// SAFETY: UI-task only. `step_ms` runs from the lifecycle loop's tick arm and
// the calibration loops, `reset` from `start` — all on the JVM task, which
// is the single drainer of the main queue.
unsafe impl Sync for TickClockCell {}

static CLOCK: TickClockCell = TickClockCell(core::cell::Cell::new(TickClock::new()));

/// How far this tick advances the UI clocks, in ms. UI task only.
///
/// One copy: the calibration loops call it from five places, and inlining
/// the clock read and the 64-bit division into each cost 700 B of RP2040
/// flash.
#[inline(never)]
pub fn step_ms() -> u32 {
    let now_ms = (crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000) as u32;
    let mut clock = CLOCK.0.get();
    let step = clock.step(now_ms);
    CLOCK.0.set(clock);
    step
}

/// Posted to the main queue on every tick.
///
/// A plain `fn` rather than a closure because the seam takes a function
/// pointer: there is one tick source per process, so there is no captured
/// state to carry across it.
fn on_tick() {
    super::main_queue::enqueue_tick();
    // The scheduling monitor's self-test wants a real-time-band task to
    // hold its core once; this callback runs on the timer service task on
    // both targets (docs/scheduling-diagnostics.md).
    #[cfg(feature = "sched-diag")]
    crate::sched_diag::selftest_on_timer_task();
}

/// Start the periodic 16 ms LVGL tick source. Idempotent; if already
/// running, ensures it is unpaused.
///
/// Also restarts the step clock, so the first tick of an app is one period
/// rather than the whole gap since the previous app's last one.
pub fn start() {
    let mut clock = CLOCK.0.get();
    clock.reset();
    CLOCK.0.set(clock);
    rtos::tick_timer_start(TICK_PERIOD_MS, on_tick);
}

/// Stop posting ticks but keep the source ready to resume. Used by the
/// activity loop's low-power sleep branch (only reachable on boards with
/// physical buttons), so this is dead on sim / touch-only builds.
pub fn pause() {
    rtos::tick_timer_pause();
}

/// Resume posting ticks after a [`pause`] call. See [`pause`] for why this
/// is `#[allow(dead_code)]`.
pub fn resume() {
    rtos::tick_timer_resume();
}

/// Tear the tick source down.
pub fn stop() {
    rtos::tick_timer_stop();
}

#[cfg(test)]
mod step_tests {
    use super::*;

    const P: u32 = TICK_PERIOD_MS;

    #[test]
    fn first_tick_after_reset_is_one_period() {
        let mut c = TickClock::new();
        assert_eq!(c.step(5_000), TICK_PERIOD_MS);
        c.reset();
        assert_eq!(
            c.step(90_000),
            TICK_PERIOD_MS,
            "a gap before the first tick is not owed"
        );
    }

    #[test]
    fn keeping_up_steps_one_period_through_timer_jitter() {
        let mut c = TickClock::new();
        let mut now = 1_000;
        c.step(now);
        for jitter in [16u32, 17, 15, 16, 18, 14, 16, 16] {
            now += jitter;
            assert_eq!(c.step(now), TICK_PERIOD_MS, "at {now}");
        }
        assert_eq!(c.fed_ms, 1_000 + 9 * P, "the schedule, not the drain times");
    }

    #[test]
    fn a_coalesced_tick_steps_the_lateness() {
        let mut c = TickClock::new();
        c.step(0);
        assert_eq!(c.step(16), 16);
        // A 200 ms frame from t=16: the tick posted at 32 sat in the queue
        // while later ones were coalesced, and is drained at 216.
        assert_eq!(c.step(216), 216 - 32, "everything since its schedule");
        assert_eq!(c.fed_ms, 216);
        // The timer kept its schedule: the next tick is the one due at 224.
        assert_eq!(c.step(224), TICK_PERIOD_MS);
        assert_eq!(c.step(240), TICK_PERIOD_MS);
    }

    #[test]
    fn a_long_pause_is_fed_in_one_step() {
        let mut c = TickClock::new();
        c.step(0);
        // Display sleep with the tick paused: the time still passed.
        assert_eq!(c.step(60_000), 60_000 - 16);
    }

    #[test]
    fn lateness_under_a_period_is_absorbed_by_the_schedule() {
        let mut c = TickClock::new();
        c.step(0);
        // A 30 ms frame: the tick due at 16 is drained at 30, the one due
        // at 32 arrives on time. Two periods for two ticks — nothing lost.
        assert_eq!(c.step(30), TICK_PERIOD_MS);
        assert_eq!(c.step(32), TICK_PERIOD_MS);
        assert_eq!(c.fed_ms, 48);
    }

    #[test]
    fn fed_clock_stays_within_a_period_of_the_wall_clock() {
        let mut c = TickClock::new();
        let mut now = 0;
        c.step(now);
        for gap in [16u32, 16, 36, 12, 16, 200, 8, 16, 16, 17, 15, 16, 1_000, 16] {
            now += gap;
            c.step(now);
            let fed = c.fed_ms;
            assert!(fed >= now, "behind the wall clock at {now}: fed {fed}");
            assert!(
                fed - now <= P,
                "more than a period ahead at {now}: fed {fed}"
            );
        }
    }

    #[test]
    fn the_clock_wraps_at_forty_nine_days() {
        let mut c = TickClock::new();
        let mut now = u32::MAX - 40;
        c.step(now);
        for _ in 0..6 {
            now = now.wrapping_add(16);
            assert_eq!(c.step(now), TICK_PERIOD_MS, "across the wrap at {now}");
        }
        // The last tick was drained at 55 past the wrap (`u32::MAX` is the
        // instant before 0), so the schedule stands at 71; a tick at 156 is
        // 85 late.
        assert_eq!(now, 55);
        assert_eq!(c.step(156), 85, "lateness survives the wrap");
    }
}

// ── Guard: LVGL's refresh period must match the tick ────────────────────────

/// Text-scan helpers, shared with the other guards in the workspace.
#[cfg(test)]
use test_support::source_scan;

#[cfg(test)]
mod refresh_period_guard {
    use std::path::Path;

    use super::source_scan::read_stripped;

    /// `LV_DEF_REFR_PERIOD` as `lvgl/lv_conf.h` declares it. Read as text
    /// because the value is a C preprocessor define: there is nothing to call
    /// from a host test, and the C side is not compiled for one.
    fn configured_refresh_period() -> u32 {
        let conf = Path::new(env!("CARGO_MANIFEST_DIR")).join("lvgl/lv_conf.h");
        let text = read_stripped(&conf);
        let after = text
            .split("#define LV_DEF_REFR_PERIOD")
            .nth(1)
            .expect("lvgl/lv_conf.h defines no LV_DEF_REFR_PERIOD");
        after
            .split_whitespace()
            .next()
            .expect("LV_DEF_REFR_PERIOD has no value")
            .parse()
            .expect("LV_DEF_REFR_PERIOD is not an integer")
    }

    /// The two have to agree. `lv_timer` stamps `last_run = lv_tick_get()` and
    /// carries no credit, so a refresh period that is not the tick quantises up
    /// to the next whole tick and the difference is spent waiting — three ticks
    /// per paint instead of one when this was 33 against a 16 ms tick
    /// (docs/designs/scroll-performance-2026-09.md S3).
    #[test]
    fn refresh_period_equals_the_tick_period() {
        assert_eq!(
            configured_refresh_period(),
            super::TICK_PERIOD_MS,
            "LV_DEF_REFR_PERIOD in lvgl/lv_conf.h must equal TICK_PERIOD_MS, \
             or every frame waits for the next whole tick"
        );
    }

    /// No tick takes a literal step. The lifecycle loop feeds
    /// [`super::step_ms`]; `Display.update()` feeds the named period on
    /// purpose (its contract is one fixed frame per call). A literal `16`
    /// at any of these calls is audit F16 coming back: a late tick advancing
    /// the UI clocks by one period regardless of how late it was.
    #[test]
    fn no_tick_call_takes_a_literal_step() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        super::source_scan::sources(&src, &["rs"], Some("tick_source.rs"), &mut files);
        let mut literal_calls = Vec::new();
        let mut tick_inc_sites = Vec::new();
        for path in files {
            let rel = super::source_scan::rel(&src, &path);
            // The `extern "C"` declaration is not a call.
            if rel == "lvgl_ffi.rs" {
                continue;
            }
            let text = read_stripped(&path);
            for needle in [".tick(", "lifecycle::tick(", "lv_tick_inc("] {
                for (at, _) in text.match_indices(needle) {
                    let arg = text[at + needle.len()..].trim_start();
                    if arg.starts_with(|c: char| c.is_ascii_digit()) {
                        let line = text[..at].matches('\n').count() + 1;
                        literal_calls.push(format!(
                            "{rel}:{line}: {needle}{}",
                            &arg[..arg.len().min(8)]
                        ));
                    }
                    if needle == "lv_tick_inc(" {
                        tick_inc_sites.push(rel.clone());
                    }
                }
            }
        }
        assert!(
            literal_calls.is_empty(),
            "UI clocks are advanced by a literal at:\n  {}\nfeed `tick_source::step_ms()` (or \
             `TICK_PERIOD_MS` where one fixed frame per call is the contract)",
            literal_calls.join("\n  ")
        );
        assert_eq!(
            tick_inc_sites,
            vec!["graphics/lvgl/lifecycle.rs".to_string()],
            "lv_tick_inc has one caller, lifecycle::tick, so the step rule has one seam"
        );
    }
}
