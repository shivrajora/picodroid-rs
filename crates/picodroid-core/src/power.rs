// SPDX-License-Identifier: GPL-3.0-only
//! Display power: the idle timer, doze, wake, keep-screen-on and the screen
//! timeout setting (docs/designs/app-portability-2026-10.md K5).
//!
//! Doze, not freeze. When no key edge or touch has arrived for the timeout,
//! the panel and backlight go off and LVGL stops ticking; the main loop keeps
//! running, so Runnables, alarms, scheduled tasks, network callbacks and the
//! sensors continue — an appliance keeps measuring with its screen dark, and
//! an alarm still rings. The earlier design blocked the loop on a button
//! interrupt, which is why every board disabled it.
//!
//! Wake is any button edge, or a finger where the controller is still read
//! while asleep (the sampler task, or the panel polled inline), or an app
//! that asks (`Activity.setTurnScreenOn`, an alarm), or a `KEYCODE_WAKEUP`.
//! The wake press is swallowed. `View.setKeepScreenOn(true)` holds the panel
//! on while that view lives, as `FLAG_KEEP_SCREEN_ON` does on Android;
//! `KEYCODE_SLEEP` dozes at once regardless, and `KEYCODE_POWER` toggles.
//!
//! The timeout is the board's `idle_timeout_ms` (60 s by default, `0` for
//! never) until Settings → Display stores one at `/system/display`
//! (`Settings.System.SCREEN_OFF_TIMEOUT`).
//!
//! The pure parts — the timer and the transition rule — are tested on the
//! host; the main loop in `lifecycle::run_activity` applies the result and
//! logs each transition with a running number and its cause
//! (`display: doze #3 after 60000 ms idle`, `display: wake #3 (touch)`), so
//! a log, or a test script, can tell one from the next.

use crate::util::local::Core0;
use core::cell::Cell;

// `IDLE_TIMEOUT_MS: Option<u64>` — the board's default, or `None` for never.
include!(concat!(env!("OUT_DIR"), "/sleep_config.rs"));

/// `Settings.System.SCREEN_OFF_TIMEOUT` stored: a little-endian `u32` of
/// milliseconds, `0` for never.
#[cfg(not(test))]
const STORE_DIR: &str = "/system";
#[cfg(not(test))]
const STORE_PATH: &str = "/system/display";
#[cfg(not(test))]
const STORE_TMP: &str = "/system/display.tmp";

/// Why the panel went dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DozeCause {
    /// The timeout passed; carries how long the panel had been idle.
    Idle(u64),
    /// `KEYCODE_SLEEP`.
    SleepKey,
    /// `KEYCODE_POWER` while awake.
    PowerKey,
}

/// What woke it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeCause {
    /// A button edge.
    Key,
    /// A finger on the glass.
    Touch,
    /// `Activity.setTurnScreenOn`, an alarm, `KEYCODE_WAKEUP` or `KEYCODE_POWER`.
    Request,
}

/// What the loop should do this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    Doze(DozeCause),
    Wake(WakeCause),
}

/// The idle timer and the doze flag, as one value the loop and the tests
/// share. Pure: `now` is handed in.
#[derive(Debug, Clone, Copy)]
pub struct IdleTimer {
    last_activity_ms: u64,
    dozing: bool,
    /// A SLEEP key or POWER toggle: doze on the next poll.
    doze_requested: Option<DozeCause>,
    /// An app, an alarm or a WAKEUP key: wake on the next poll.
    wake_requested: bool,
}

impl IdleTimer {
    pub const fn new() -> Self {
        Self {
            last_activity_ms: 0,
            dozing: false,
            doze_requested: None,
            wake_requested: false,
        }
    }

    /// A key edge or a touch at `now`.
    pub fn activity(&mut self, now_ms: u64) {
        self.last_activity_ms = now_ms;
    }

    /// Decide the tick. `timeout` is the screen timeout, `None` for never;
    /// `held` is whether a view keeps the screen on; `key` and `touch` are
    /// whether an edge or a finger is pending — while dozing that is the
    /// wake, while awake it is activity the input path has already stamped.
    pub fn poll(
        &mut self,
        now_ms: u64,
        timeout: Option<u64>,
        held: bool,
        key: bool,
        touch: bool,
    ) -> Transition {
        if self.dozing {
            let cause = if key {
                Some(WakeCause::Key)
            } else if touch {
                Some(WakeCause::Touch)
            } else if self.wake_requested {
                Some(WakeCause::Request)
            } else {
                None
            };
            if let Some(cause) = cause {
                self.dozing = false;
                self.wake_requested = false;
                self.doze_requested = None;
                self.last_activity_ms = now_ms;
                return Transition::Wake(cause);
            }
            return Transition::None;
        }
        self.wake_requested = false;
        if let Some(cause) = self.doze_requested.take() {
            self.dozing = true;
            return Transition::Doze(cause);
        }
        if let Some(t) = timeout {
            let idle = now_ms.saturating_sub(self.last_activity_ms);
            if !held && idle >= t {
                self.dozing = true;
                return Transition::Doze(DozeCause::Idle(idle));
            }
        }
        Transition::None
    }

    pub fn dozing(&self) -> bool {
        self.dozing
    }
}

impl Default for IdleTimer {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: UI-task state: the input callbacks and the main loop run on the
// JVM task.
static TIMER: Core0<Cell<IdleTimer>> = unsafe { Core0::new(Cell::new(IdleTimer::new())) };
/// How many live views keep the screen on.
// SAFETY: widget-layer state, reached only from JVM tasks.
static KEEP_ON: Core0<Cell<u32>> = unsafe { Core0::new(Cell::new(0)) };
/// The stored timeout once read: `Some(Some(ms))`, `Some(None)` for never,
/// `None` before the first read.
// SAFETY: UI-task state.
static STORED_TIMEOUT: Core0<Cell<Option<Option<u64>>>> = unsafe { Core0::new(Cell::new(None)) };
/// How many times the panel has dozed: the number in the log lines.
// SAFETY: UI-task state.
static DOZES: Core0<Cell<u32>> = unsafe { Core0::new(Cell::new(0)) };

fn with_timer<R>(f: impl FnOnce(&mut IdleTimer) -> R) -> R {
    let mut t = TIMER.get();
    let r = f(&mut t);
    TIMER.set(t);
    r
}

fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

/// A key edge or a touch sample arrived: the panel stays on.
pub fn user_activity() {
    with_timer(|t| t.activity(now_ms()));
}

/// Whether the panel is on (`PowerManager.isInteractive()`).
pub fn is_interactive() -> bool {
    !TIMER.get().dozing()
}

/// Doze on the next tick (`KEYCODE_SLEEP`, or `KEYCODE_POWER` while awake).
pub fn request_doze(cause: DozeCause) {
    with_timer(|t| t.doze_requested = Some(cause));
}

/// Wake on the next tick (`Activity.setTurnScreenOn`, an alarm,
/// `KEYCODE_WAKEUP`, or `KEYCODE_POWER` while dozing).
pub fn request_wake() {
    with_timer(|t| t.wake_requested = true);
}

/// `KEYCODE_POWER`: doze if awake, wake if dozing.
pub fn request_toggle() {
    if is_interactive() {
        request_doze(DozeCause::PowerKey)
    } else {
        request_wake()
    }
}

/// A view set `keepScreenOn`, or dropped it (deleted, or set it false).
pub fn hold_screen(on: bool) {
    let n = KEEP_ON.get();
    KEEP_ON.set(if on { n + 1 } else { n.saturating_sub(1) });
}

/// Forget every hold: the app that held them is gone.
pub fn reset_holds() {
    KEEP_ON.set(0);
}

/// The screen timeout in force: the stored setting, else the board's
/// default. `None` is never.
pub fn timeout_ms() -> Option<u64> {
    match STORED_TIMEOUT.get() {
        Some(stored) => stored,
        None => {
            let stored = load_stored().unwrap_or(IDLE_TIMEOUT_MS);
            STORED_TIMEOUT.set(Some(stored));
            stored
        }
    }
}

/// `Settings.System.putInt(SCREEN_OFF_TIMEOUT, ms)`: `0` is never. True when
/// stored.
pub fn set_timeout_ms(ms: u32) -> bool {
    let value = if ms == 0 { None } else { Some(u64::from(ms)) };
    STORED_TIMEOUT.set(Some(value));
    save_stored(ms)
}

/// The loop's tick: what to do now, given whether an edge or a finger is
/// pending. Stamps the activity on a wake and counts a doze.
pub fn poll(key_pending: bool, touch_pending: bool) -> Transition {
    let timeout = timeout_ms();
    let held = KEEP_ON.get() > 0;
    let t = with_timer(|t| t.poll(now_ms(), timeout, held, key_pending, touch_pending));
    if matches!(t, Transition::Doze(_)) {
        DOZES.set(DOZES.get() + 1);
    }
    t
}

/// The number of the doze in progress or just ended, for the log.
pub fn doze_number() -> u32 {
    DOZES.get()
}

#[cfg(not(test))]
fn load_stored() -> Option<Option<u64>> {
    use crate::hal::fs;
    if !fs::is_file(STORE_PATH) {
        return None;
    }
    let mut buf = alloc::vec::Vec::new();
    if fs::read_at(STORE_PATH, 0, &mut buf, 4) < 0 || buf.len() < 4 {
        return None;
    }
    let ms = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    Some(if ms == 0 { None } else { Some(u64::from(ms)) })
}

/// Written to a temporary file and renamed over the store, so a reset
/// mid-write leaves the old value intact (as `hal::wifi` keeps its network).
#[cfg(not(test))]
fn save_stored(ms: u32) -> bool {
    use crate::hal::fs;
    if !fs::is_dir(STORE_DIR) && !fs::mkdir(STORE_DIR) {
        return false;
    }
    fs::truncate(STORE_TMP);
    if fs::write_at(STORE_TMP, 0, &ms.to_le_bytes()) < 0 {
        let _ = fs::delete(STORE_TMP);
        return false;
    }
    fs::rename(STORE_TMP, STORE_PATH)
}

#[cfg(test)]
fn load_stored() -> Option<Option<u64>> {
    None
}

#[cfg(test)]
fn save_stored(_ms: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Option<u64> = Some(60_000);

    #[test]
    fn times_out_only_without_a_hold_and_wakes_on_input() {
        let mut t = IdleTimer::new();
        t.activity(1_000);
        assert_eq!(t.poll(30_000, T, false, false, false), Transition::None);
        assert_eq!(
            t.poll(61_000, T, false, false, false),
            Transition::Doze(DozeCause::Idle(60_000))
        );
        assert!(t.dozing());
        // Still asleep until something arrives.
        assert_eq!(t.poll(90_000, T, false, false, false), Transition::None);
        assert_eq!(
            t.poll(91_000, T, false, false, true),
            Transition::Wake(WakeCause::Touch)
        );
        assert!(!t.dozing());
        // The wake stamped activity: the timer runs from then.
        assert_eq!(t.poll(120_000, T, false, false, false), Transition::None);
        assert_eq!(
            t.poll(151_000, T, false, false, false),
            Transition::Doze(DozeCause::Idle(60_000))
        );
        // A key outranks a finger as the reported cause.
        assert_eq!(
            t.poll(152_000, T, false, true, true),
            Transition::Wake(WakeCause::Key)
        );
    }

    #[test]
    fn a_hold_and_never_both_keep_the_panel_on() {
        let mut t = IdleTimer::new();
        assert_eq!(t.poll(100_000, T, true, false, false), Transition::None);
        assert_eq!(t.poll(100_000, None, false, false, false), Transition::None);
    }

    #[test]
    fn requests_win_over_the_timer_and_the_hold() {
        let mut t = IdleTimer::new();
        t.doze_requested = Some(DozeCause::SleepKey);
        assert_eq!(
            t.poll(10, T, true, false, false),
            Transition::Doze(DozeCause::SleepKey)
        );
        t.wake_requested = true;
        assert_eq!(
            t.poll(20, T, false, false, false),
            Transition::Wake(WakeCause::Request)
        );
        // A stale wake request while awake is dropped, not carried.
        t.wake_requested = true;
        assert_eq!(t.poll(30, T, false, false, false), Transition::None);
        assert!(!t.wake_requested);
    }
}
