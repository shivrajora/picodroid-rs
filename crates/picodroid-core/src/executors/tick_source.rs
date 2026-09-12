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

use crate::rtos;

const TICK_PERIOD_MS: u32 = 16;

/// Posted to the main queue on every tick.
///
/// A plain `fn` rather than a closure because the seam takes a function
/// pointer: there is one tick source per process, so there is no captured
/// state to carry across it.
fn on_tick() {
    super::main_queue::enqueue_tick();
}

/// Start the periodic 16 ms LVGL tick source. Idempotent; if already
/// running, ensures it is unpaused.
pub fn start() {
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

// ── Guard: LVGL's refresh period must match the tick ────────────────────────

/// Text-scan helpers, shared with the other guards in the workspace.
#[cfg(test)]
#[path = "../../../test_support/source_scan.rs"]
mod source_scan;

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
}
