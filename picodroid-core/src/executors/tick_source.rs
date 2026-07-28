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
