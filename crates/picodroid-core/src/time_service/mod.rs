// SPDX-License-Identifier: GPL-3.0-only
//! The platform time service: who sets the clock, and what zone it is shown
//! in (docs/designs/time-service-2026-10.md).
//!
//! Boards have no battery-backed clock, so `System.currentTimeMillis()`
//! counts from boot until something anchors it. Until now that something was
//! each app: every HTTPS app ran its own `SntpClient` exchange before its
//! first request, `claudeusage` took the bridge's time and `picoclock` asked
//! the user to turn a stepper after every power cut. Android does not let an
//! app set the clock; the platform anchors it from the network once
//! connectivity is up, and the zone is a user setting.
//!
//! Two halves:
//!
//! - [`task`] (network boards only): a task that waits for the link, runs one
//!   SNTP exchange ([`sntp`]) against `pool.ntp.org`, anchors the wall clock
//!   through `os::system_clock::anchor_wall_clock`, and re-anchors every few
//!   hours. Off when `Settings.Global.AUTO_TIME` is 0.
//! - The zone store, here: the user's UTC offset in minutes, stored at
//!   `/system/time` beside the Wi-Fi credentials, which `java.util.TimeZone
//!   .getDefault()` (hence `ZoneId.systemDefault()` and every `now()`) reads
//!   and Settings → Date & time writes through `AlarmManager.setTimeZone`.
//!   A fixed offset, never a region: there is no tz database on the device
//!   (picoclock-roadmap R4), so daylight saving is a setting the user moves.
//!
//! The store is one 8-byte record, written to a temporary file and renamed
//! over the old one (as `power.rs` and `hal::wifi` do). Its format, and
//! the zone-id parser, are `pd_codec::time` (re-exported here), so the host
//! can fuzz them; the SNTP packet codec is `pd_codec::sntp` likewise.
//! Missing or short: UTC, automatic.

pub use pd_codec::sntp;
pub use pd_codec::time::{
    parse_offset_minutes, valid_offset, Record, MAX_OFFSET_MINUTES, MIN_OFFSET_MINUTES, RECORD_LEN,
};

#[cfg(all(not(test), has_network))]
pub mod task;

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU8, Ordering};

/// Stack of the time task, in bytes. The deepest chain is the stack's DNS
/// resolver (`FreeRTOS_gethostbyname` and its 256-byte name buffer) plus
/// one UDP exchange and a log line; the first sync on a device logs the
/// unused remainder so the figure can be checked on hardware.
pub const TASK_STACK_BYTES: u32 = 4096;
/// The task's name, in the boot budget and the kernel's task list.
pub const TASK_NAME: &str = "timesync";

#[cfg(not(test))]
const STORE_DIR: &str = "/system";
#[cfg(not(test))]
const STORE_PATH: &str = "/system/time";
#[cfg(not(test))]
const STORE_TMP: &str = "/system/time.tmp";

/// Epoch milliseconds of 2001-01-01: a wall clock below this has never been
/// anchored this boot (nothing sets it to a date before 2001), the same
/// line `picoclock`'s `Clock.isSet` draws. Used where "is the clock set"
/// has to be answered from the value alone.
pub const SET_THRESHOLD_MS: i64 = 978_307_200_000;

// The store, cached once read. Atomics rather than `Core0` cells: the time
// task reads `auto_time` from its own task, the natives run on JVM tasks.
static LOADED: AtomicBool = AtomicBool::new(false);
static OFFSET_MINUTES: AtomicI32 = AtomicI32::new(0);
static AUTO_TIME: AtomicU8 = AtomicU8::new(1);

/// How many times the task has anchored the clock this boot, and when it
/// last did (`elapsed_realtime` seconds). Read by Settings → Date & time.
static SYNCS: AtomicU32 = AtomicU32::new(0);
static LAST_SYNC_S: AtomicU32 = AtomicU32::new(0);

fn ensure_loaded() {
    if LOADED.load(Ordering::Acquire) {
        return;
    }
    let r = load_stored().unwrap_or(Record::DEFAULT);
    OFFSET_MINUTES.store(r.offset_minutes, Ordering::Relaxed);
    AUTO_TIME.store(u8::from(r.auto_time), Ordering::Relaxed);
    LOADED.store(true, Ordering::Release);
}

fn current() -> Record {
    ensure_loaded();
    Record {
        offset_minutes: OFFSET_MINUTES.load(Ordering::Relaxed),
        auto_time: AUTO_TIME.load(Ordering::Relaxed) != 0,
    }
}

/// The platform zone: minutes east of UTC (`TimeZone.getDefault()`).
pub fn zone_offset_minutes() -> i32 {
    current().offset_minutes
}

/// `AlarmManager.setTimeZone`: store a new offset. False when out of range
/// or when the store could not be written (the value still applies until
/// the next boot).
pub fn set_zone_offset_minutes(minutes: i32) -> bool {
    if !valid_offset(minutes) {
        return false;
    }
    let mut r = current();
    let changed = r.offset_minutes != minutes;
    r.offset_minutes = minutes;
    OFFSET_MINUTES.store(minutes, Ordering::Relaxed);
    let stored = save_stored(&r);
    if changed {
        crate::broadcasts::note_zone_changed();
    }
    crate::pd_info!(
        "time: zone UTC{}{:02}:{:02}{}",
        if minutes < 0 { "-" } else { "+" },
        minutes.abs() / 60,
        minutes.abs() % 60,
        if stored { "" } else { " (not stored)" }
    );
    stored
}

/// `Settings.Global.AUTO_TIME`: whether the task anchors the clock.
pub fn auto_time() -> bool {
    current().auto_time
}

/// Store `Settings.Global.AUTO_TIME`. Turning it on asks the task for a
/// sync at once.
pub fn set_auto_time(on: bool) -> bool {
    let mut r = current();
    r.auto_time = on;
    AUTO_TIME.store(u8::from(on), Ordering::Relaxed);
    let stored = save_stored(&r);
    crate::pd_info!(
        "time: automatic {}{}",
        if on { "on" } else { "off" },
        if stored { "" } else { " (not stored)" }
    );
    #[cfg(all(not(test), has_network))]
    if on {
        task::request_sync_now();
    }
    stored
}

/// Whether the wall clock has been anchored this boot (by the task, an app
/// or the simulator's `PICODROID_SIM_WALL_CLOCK`).
#[cfg(not(test))]
pub fn clock_is_set() -> bool {
    crate::os::system_clock::wall_offset_ms() != 0
}

/// Host tests have no wall clock (`os` is `cfg(not(test))`).
#[cfg(test)]
pub fn clock_is_set() -> bool {
    false
}

/// Record one anchoring by the task.
pub fn note_sync(elapsed_ms: u64) {
    SYNCS.store(SYNCS.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
    LAST_SYNC_S.store((elapsed_ms / 1000) as u32, Ordering::Release);
}

/// How many times the task has anchored the clock this boot.
pub fn sync_count() -> u32 {
    SYNCS.load(Ordering::Relaxed)
}

/// Seconds since boot at the task's last anchoring; 0 before the first.
pub fn last_sync_elapsed_s() -> u32 {
    LAST_SYNC_S.load(Ordering::Acquire)
}

#[cfg(not(test))]
fn load_stored() -> Option<Record> {
    use crate::hal::fs;
    if !fs::is_file(STORE_PATH) {
        return None;
    }
    let mut buf = alloc::vec::Vec::new();
    if fs::read_at(STORE_PATH, 0, &mut buf, RECORD_LEN) < 0 {
        return None;
    }
    Record::decode(&buf)
}

#[cfg(not(test))]
fn save_stored(r: &Record) -> bool {
    use crate::hal::fs;
    if !fs::is_dir(STORE_DIR) && !fs::mkdir(STORE_DIR) {
        return false;
    }
    fs::truncate(STORE_TMP);
    if fs::write_at(STORE_TMP, 0, &r.encode()) < 0 {
        let _ = fs::delete(STORE_TMP);
        return false;
    }
    fs::rename(STORE_TMP, STORE_PATH)
}

#[cfg(test)]
fn load_stored() -> Option<Record> {
    None
}

#[cfg(test)]
fn save_stored(_r: &Record) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setter_bounds_and_defaults() {
        assert!(!set_zone_offset_minutes(MAX_OFFSET_MINUTES + 1));
        assert!(!set_zone_offset_minutes(MIN_OFFSET_MINUTES - 1));
        assert!(set_zone_offset_minutes(MAX_OFFSET_MINUTES));
        assert_eq!(zone_offset_minutes(), MAX_OFFSET_MINUTES);
        assert!(set_zone_offset_minutes(0));
        assert_eq!(zone_offset_minutes(), 0);
        assert!(auto_time());
        assert!(set_auto_time(false));
        assert!(!auto_time());
        assert!(set_auto_time(true));
    }
}
