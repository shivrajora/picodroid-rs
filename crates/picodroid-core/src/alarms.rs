// SPDX-License-Identifier: GPL-3.0-only
//! Alarms that outlive the app that set them — the framework half of
//! `picodroid.app.AlarmManager`.
//!
//! An app asks to have one of its Activities started at a time and is then
//! free to exit. Because one app runs at a time and an app switch resets the
//! heap ([`crate::boot::run_app`]), an alarm cannot live in the app's own
//! memory: this table is a module static, like [`crate::packages`]'s
//! directory and the wall-clock anchor in [`crate::os::system_clock`], and is
//! deliberately absent from `run_app`'s reset list.
//!
//! It is RAM, not flash. A reset loses every alarm, exactly as it loses the
//! wall clock on a board with no battery-backed RTC, and an app re-registers
//! what it still wants on its next start — Android's `BOOT_COMPLETED`
//! contract, arrived at from the other direction.
//!
//! # What a poll decides
//!
//! [`poll`] runs once per UI tick and returns at most one [`Action`], so a
//! frame queues at most one lifecycle op. An alarm whose owner is the running
//! app is delivered ([`Action::Fire`]); an alarm belonging to some other app
//! asks for that app first ([`Action::Wake`]), and is delivered by a later
//! poll once it is up.
//!
//! A triggered entry moves to [`State::Firing`] and leaves the identity space:
//! neither [`set`] nor [`cancel`] can touch it. That is what makes the cold
//! relaunch correct. The woken app starts, its service reads its own store and
//! re-arms every alarm it knows about — including the one that is at that
//! moment being delivered — and those fresh `Armed` entries are separate rows.
//! Without the split, the app would cancel its own wake-up on the way in.
//!
//! # Who writes
//!
//! The JVM task, through the natives and through [`poll`] on the lifecycle
//! tick — the same task in both cases. A Java thread calling `setExact` is the
//! exception, so every entry point holds an [`AtomicSection`].

use pico_jvm::atomic_section::AtomicSection;

use crate::packages::Name;

/// Alarms the framework holds at once, across every app.
pub const CAPACITY: usize = 12;

/// Alarms one app may have armed. Below [`CAPACITY`] so a busy app cannot
/// starve every other one; `Firing` entries do not count, a delivery in
/// flight being something the app can no longer prevent.
pub const PER_PACKAGE: usize = 8;

/// Extras an alarm carries, mirrored by `PendingIntent.MAX_EXTRAS`.
pub const MAX_EXTRAS: usize = 2;

/// Longest extra key, mirrored by `PendingIntent.MAX_KEY_LENGTH`.
pub const KEY_MAX: usize = 15;

/// Longest target class name. `picoclock/ui/RingActivity` is 25 bytes.
pub const CLASS_MAX: usize = 48;

/// Times the framework starts an owner for one alarm before giving up. An
/// app that runs without ever reaching a UI tick — a service-only app, or one
/// that faults on the way in — would otherwise be started for ever.
pub const MAX_WAKE_ATTEMPTS: u8 = 2;

/// Which clock an alarm's trigger is on: `AlarmManager`'s `RTC*` constants
/// against the wall clock, its `ELAPSED_REALTIME*` ones against time since
/// boot.
///
/// Keeping them apart is what makes `setCurrentTimeMillis` behave as Android
/// says: moving the wall clock forward brings RTC alarms due and leaves
/// elapsed ones where they were.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clock {
    Rtc,
    Elapsed,
}

impl Clock {
    /// `AlarmManager`'s four type constants. The `_WAKEUP` variants differ
    /// only in whether they wake a sleeping device, and nothing here suspends
    /// the JVM, so each pair maps to one clock.
    pub fn from_type(alarm_type: i32) -> Option<Clock> {
        match alarm_type {
            0 | 1 => Some(Clock::Rtc),
            2 | 3 => Some(Clock::Elapsed),
            _ => None,
        }
    }
}

/// Both clocks, read once per poll so every entry is judged against the same
/// instant.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub wall_ms: i64,
    pub elapsed_ms: i64,
}

/// Why [`set`] refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SetError {
    /// No free row anywhere.
    Full,
    /// This app already has [`PER_PACKAGE`] alarms armed.
    PackageFull,
    /// A class name or extra key longer than the store's fixed rows. Java
    /// checks the keys first, so this is a framework-side backstop.
    TooLong,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Waiting for its trigger; [`set`] and [`cancel`] may replace it.
    Armed,
    /// Triggered and on its way to the owner. `wake_gen` is the run the last
    /// wake-up was asked in, so one launch is requested per run rather than
    /// one per tick.
    Firing { wake_gen: u32, attempts: u8 },
}

#[derive(Clone, Copy)]
struct Extra {
    key: [u8; KEY_MAX],
    key_len: u8,
    value: i32,
}

impl Extra {
    const EMPTY: Extra = Extra {
        key: [0; KEY_MAX],
        key_len: 0,
        value: 0,
    };

    fn new(key: &str, value: i32) -> Extra {
        let bytes = key.as_bytes();
        let n = bytes.len().min(KEY_MAX);
        let mut e = Extra::EMPTY;
        e.key[..n].copy_from_slice(&bytes[..n]);
        e.key_len = n as u8;
        e.value = value;
        e
    }

    fn key(&self) -> Option<&str> {
        core::str::from_utf8(&self.key[..self.key_len as usize]).ok()
    }
}

#[derive(Clone, Copy)]
struct Entry {
    owner: Name,
    class: [u8; CLASS_MAX],
    class_len: u8,
    request_code: i32,
    clock: Clock,
    trigger_ms: i64,
    extras: [Extra; MAX_EXTRAS],
    n_extras: u8,
    state: State,
}

impl Entry {
    fn class(&self) -> &str {
        core::str::from_utf8(&self.class[..self.class_len as usize]).unwrap_or("")
    }

    fn owns(&self, owner: &str, class: &str, request_code: i32) -> bool {
        self.request_code == request_code
            && self.owner.get() == Some(owner)
            && self.class() == class
    }

    fn is_due(&self, now: Now) -> bool {
        self.now_on_my_clock(now) >= self.trigger_ms
    }

    fn now_on_my_clock(&self, now: Now) -> i64 {
        match self.clock {
            Clock::Rtc => now.wall_ms,
            Clock::Elapsed => now.elapsed_ms,
        }
    }
}

/// One alarm, copied out of the table for delivery. Owned bytes rather than
/// borrows: the row is freed the moment it is handed over, and the upcall
/// that follows runs arbitrary Java.
pub struct Fire {
    owner: Name,
    class: [u8; CLASS_MAX],
    class_len: u8,
    extras: [Extra; MAX_EXTRAS],
    n_extras: u8,
    /// How far past its trigger the alarm is being delivered.
    pub late_ms: i64,
    pub request_code: i32,
}

impl Fire {
    /// The Activity to start, in JVM internal form.
    pub fn class(&self) -> &str {
        core::str::from_utf8(&self.class[..self.class_len as usize]).unwrap_or("")
    }

    /// The app the alarm belongs to.
    pub fn package(&self) -> &str {
        self.owner.get().unwrap_or("")
    }

    /// Extra `i`, or `None` past the last one.
    pub fn extra(&self, i: usize) -> Option<(&str, i32)> {
        if i >= self.n_extras as usize {
            return None;
        }
        let e = &self.extras[i];
        Some((e.key()?, e.value))
    }
}

/// The app an alarm is waiting for.
pub struct Wake {
    owner: Name,
}

impl Wake {
    pub fn package(&self) -> &str {
        self.owner.get().unwrap_or("")
    }
}

/// What a poll wants the caller to do.
pub enum Action {
    /// Start this Activity in the running app.
    Fire(Fire),
    /// Start this app; the alarm is delivered once it is up.
    Wake(Wake),
}

struct TableCell(core::cell::UnsafeCell<[Option<Entry>; CAPACITY]>);
// SAFETY: single-writer discipline — see the module docs. Every entry point
// holds an `AtomicSection` for the whole of its access.
unsafe impl Sync for TableCell {}

static TABLE: TableCell = TableCell(core::cell::UnsafeCell::new([None; CAPACITY]));

#[allow(clippy::mut_from_ref)]
fn table() -> &'static mut [Option<Entry>; CAPACITY] {
    unsafe { &mut *TABLE.0.get() }
}

/// Arm `class` in `owner` for `trigger_ms`, replacing any armed alarm with
/// the same owner, class and request code. `Ok(true)` means one was replaced.
pub fn set(
    owner: &str,
    class: &str,
    request_code: i32,
    clock: Clock,
    trigger_ms: i64,
    extras: &[(&str, i32)],
) -> Result<bool, SetError> {
    if class.len() > CLASS_MAX || extras.len() > MAX_EXTRAS {
        return Err(SetError::TooLong);
    }
    if extras.iter().any(|(k, _)| k.len() > KEY_MAX) {
        return Err(SetError::TooLong);
    }

    let mut entry = Entry {
        owner: Name::EMPTY,
        class: [0; CLASS_MAX],
        class_len: class.len() as u8,
        request_code,
        clock,
        trigger_ms,
        extras: [Extra::EMPTY; MAX_EXTRAS],
        n_extras: extras.len() as u8,
        state: State::Armed,
    };
    entry.owner.set(Some(owner));
    entry.class[..class.len()].copy_from_slice(class.as_bytes());
    for (i, (k, v)) in extras.iter().enumerate() {
        entry.extras[i] = Extra::new(k, *v);
    }

    let _atomic = AtomicSection::enter();
    let table = table();

    if let Some(i) = table.iter().position(|slot| {
        slot.as_ref()
            .is_some_and(|e| e.state == State::Armed && e.owns(owner, class, request_code))
    }) {
        table[i] = Some(entry);
        return Ok(true);
    }

    let armed_here = table
        .iter()
        .flatten()
        .filter(|e| e.state == State::Armed && e.owner.get() == Some(owner))
        .count();
    if armed_here >= PER_PACKAGE {
        return Err(SetError::PackageFull);
    }

    let free = table
        .iter()
        .position(|slot| slot.is_none())
        .ok_or(SetError::Full)?;
    table[free] = Some(entry);
    Ok(false)
}

/// Disarm the alarm with this identity. `false` when none was armed — either
/// nothing matched, or the alarm has already triggered and is on its way to
/// the app, which Android does not let a cancel recall either.
pub fn cancel(owner: &str, class: &str, request_code: i32) -> bool {
    let _atomic = AtomicSection::enter();
    let table = table();
    match table.iter().position(|slot| {
        slot.as_ref()
            .is_some_and(|e| e.state == State::Armed && e.owns(owner, class, request_code))
    }) {
        Some(i) => {
            table[i] = None;
            true
        }
        None => false,
    }
}

/// Everything `package` has asked for, forgotten. Called when a package is
/// uninstalled; [`poll`] also drops entries whose owner has gone.
pub fn drop_package(package: &str) -> usize {
    let _atomic = AtomicSection::enter();
    let table = table();
    let mut dropped = 0;
    for slot in table.iter_mut() {
        if slot
            .as_ref()
            .is_some_and(|e| e.owner.get() == Some(package))
        {
            *slot = None;
            dropped += 1;
        }
    }
    dropped
}

/// How many alarms the table holds, armed and firing alike. For diagnostics.
pub fn len() -> usize {
    let _atomic = AtomicSection::enter();
    table().iter().flatten().count()
}

/// Decide what the framework should do about alarms this tick, and return at
/// most one action so a frame queues at most one lifecycle op.
///
/// `installed` reports whether a package is still installed — the hook that
/// makes an uninstall forget that package's alarms without the uninstall path
/// having to know about them.
pub fn poll(
    now: Now,
    running: Option<&str>,
    run_gen: u32,
    installed: &dyn Fn(&str) -> bool,
    launch_pending: bool,
) -> Option<Action> {
    let _atomic = AtomicSection::enter();
    let table = table();

    for slot in table.iter_mut() {
        let gone = slot
            .as_ref()
            .is_some_and(|e| !installed(e.owner.get().unwrap_or("")));
        if gone {
            if let Some(e) = slot {
                crate::pd_warn!(
                    "[alarm] drop {} {}#{}: not installed",
                    e.owner.get().unwrap_or(""),
                    e.class(),
                    e.request_code
                );
            }
            *slot = None;
        }
    }

    // Earliest trigger first, so a backlog drains in the order it accrued.
    let mut best: Option<usize> = None;
    for (i, slot) in table.iter().enumerate() {
        let Some(e) = slot else { continue };
        let actionable = match e.state {
            State::Firing { .. } => true,
            State::Armed => e.is_due(now),
        };
        if !actionable {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => e.trigger_ms < table[b].as_ref().map(|o| o.trigger_ms).unwrap_or(i64::MAX),
        };
        if better {
            best = Some(i);
        }
    }
    let i = best?;

    let mut entry = table[i]?;
    let owner_is_running = running.is_some() && running == entry.owner.get();

    match entry.state {
        State::Armed => {
            // Either answer below needs this app to be staying put: a queued
            // launch means its Activity stack is about to be torn down, so a
            // push would only lose the alarm, and asking for another app
            // while one is already on its way would jump that queue.
            if launch_pending {
                return None;
            }
            if owner_is_running {
                table[i] = None;
                Some(Action::Fire(fire_from(entry, now)))
            } else {
                entry.state = State::Firing {
                    wake_gen: run_gen,
                    attempts: 1,
                };
                table[i] = Some(entry);
                Some(Action::Wake(Wake { owner: entry.owner }))
            }
        }
        State::Firing { wake_gen, attempts } => {
            if owner_is_running {
                table[i] = None;
                Some(Action::Fire(fire_from(entry, now)))
            } else if wake_gen == run_gen {
                // A launch asked for in this run has not landed yet.
                None
            } else if attempts >= MAX_WAKE_ATTEMPTS {
                crate::pd_warn!(
                    "[alarm] drop {} {}#{}: owner ran without delivering",
                    entry.owner.get().unwrap_or(""),
                    entry.class(),
                    entry.request_code
                );
                table[i] = None;
                None
            } else {
                entry.state = State::Firing {
                    wake_gen: run_gen,
                    attempts: attempts + 1,
                };
                table[i] = Some(entry);
                Some(Action::Wake(Wake { owner: entry.owner }))
            }
        }
    }
}

/// A row, as the delivery side needs it.
fn fire_from(e: Entry, now: Now) -> Fire {
    Fire {
        owner: e.owner,
        class: e.class,
        class_len: e.class_len,
        extras: e.extras,
        n_extras: e.n_extras,
        late_ms: e.now_on_my_clock(now) - e.trigger_ms,
        request_code: e.request_code,
    }
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    *table() = [None; CAPACITY];
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// One table, so the tests take turns over it.
    fn guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let g = LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        g
    }

    const APP: &str = "clock";
    const OTHER: &str = "weather";
    const RING: &str = "clock/RingActivity";

    fn now(ms: i64) -> Now {
        Now {
            wall_ms: ms,
            elapsed_ms: ms,
        }
    }

    fn always_installed(_: &str) -> bool {
        true
    }

    /// Poll as the owner's own run, nothing leaving.
    fn poll_as(now_ms: i64, running: Option<&str>, gen: u32) -> Option<Action> {
        poll(now(now_ms), running, gen, &always_installed, false)
    }

    fn fired(action: Option<Action>) -> Fire {
        match action {
            Some(Action::Fire(f)) => f,
            Some(Action::Wake(w)) => panic!("expected a fire, got a wake for {}", w.package()),
            None => panic!("expected a fire, got nothing"),
        }
    }

    fn woke(action: Option<Action>) -> String {
        match action {
            Some(Action::Wake(w)) => w.package().to_string(),
            Some(Action::Fire(f)) => panic!("expected a wake, got a fire for {}", f.class()),
            None => panic!("expected a wake, got nothing"),
        }
    }

    #[test]
    fn set_then_fire_delivers_class_and_extras() {
        let _g = guard();
        set(
            APP,
            RING,
            3,
            Clock::Rtc,
            1_000,
            &[("alarm", 7), ("due", 16)],
        )
        .unwrap();
        assert!(poll_as(999, Some(APP), 1).is_none(), "not due yet");
        let f = fired(poll_as(1_000, Some(APP), 1));
        assert_eq!(f.class(), RING);
        assert_eq!(f.package(), APP);
        assert_eq!(f.request_code, 3);
        assert_eq!(f.late_ms, 0);
        assert_eq!(f.extra(0), Some(("alarm", 7)));
        assert_eq!(f.extra(1), Some(("due", 16)));
        assert_eq!(f.extra(2), None);
        assert_eq!(len(), 0, "a delivered alarm is gone");
    }

    #[test]
    fn set_replaces_by_identity_and_keeps_others() {
        let _g = guard();
        assert_eq!(set(APP, RING, 0, Clock::Rtc, 1_000, &[]), Ok(false));
        assert_eq!(
            set(APP, RING, 0, Clock::Rtc, 5_000, &[]),
            Ok(true),
            "same identity replaces"
        );
        assert_eq!(len(), 1);
        // A different request code, class or owner is a different alarm.
        assert_eq!(set(APP, RING, 1, Clock::Rtc, 1_000, &[]), Ok(false));
        assert_eq!(
            set(APP, "clock/Other", 0, Clock::Rtc, 1_000, &[]),
            Ok(false)
        );
        assert_eq!(set(OTHER, RING, 0, Clock::Rtc, 1_000, &[]), Ok(false));
        assert_eq!(len(), 4);
        // The replaced one kept its new time: nothing is due at 1_000 for it.
        let f = fired(poll_as(1_000, Some(APP), 1));
        assert_eq!(f.request_code, 1);
    }

    #[test]
    fn cancel_removes_only_its_own_identity() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        set(APP, RING, 1, Clock::Rtc, 1_000, &[]).unwrap();
        assert!(cancel(APP, RING, 0));
        assert_eq!(len(), 1);
        assert!(!cancel(APP, RING, 0), "already gone");
        assert!(!cancel(OTHER, RING, 1), "another app cannot cancel it");
        assert_eq!(len(), 1);
    }

    #[test]
    fn earliest_trigger_fires_first_one_per_poll() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 3_000, &[]).unwrap();
        set(APP, RING, 1, Clock::Rtc, 1_000, &[]).unwrap();
        set(APP, RING, 2, Clock::Rtc, 2_000, &[]).unwrap();
        assert_eq!(fired(poll_as(9_000, Some(APP), 1)).request_code, 1);
        assert_eq!(fired(poll_as(9_000, Some(APP), 1)).request_code, 2);
        assert_eq!(fired(poll_as(9_000, Some(APP), 1)).request_code, 0);
        assert!(poll_as(9_000, Some(APP), 1).is_none());
    }

    #[test]
    fn late_delivery_reports_how_late() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        assert_eq!(fired(poll_as(61_000, Some(APP), 1)).late_ms, 60_000);
    }

    #[test]
    fn table_and_per_package_caps() {
        let _g = guard();
        for rc in 0..PER_PACKAGE as i32 {
            assert_eq!(set(APP, RING, rc, Clock::Rtc, 1_000, &[]), Ok(false));
        }
        assert_eq!(
            set(APP, RING, 99, Clock::Rtc, 1_000, &[]),
            Err(SetError::PackageFull),
            "one app cannot take the whole table"
        );
        // Another app still fits: CAPACITY - PER_PACKAGE rows are left.
        for rc in 0..(CAPACITY - PER_PACKAGE) as i32 {
            assert_eq!(set(OTHER, RING, rc, Clock::Rtc, 1_000, &[]), Ok(false));
        }
        assert_eq!(
            set(OTHER, RING, 99, Clock::Rtc, 1_000, &[]),
            Err(SetError::Full)
        );
        assert_eq!(len(), CAPACITY);
    }

    #[test]
    fn oversized_class_or_key_is_refused() {
        let _g = guard();
        let long_class = "a".repeat(CLASS_MAX + 1);
        assert_eq!(
            set(APP, &long_class, 0, Clock::Rtc, 0, &[]),
            Err(SetError::TooLong)
        );
        let long_key = "k".repeat(KEY_MAX + 1);
        assert_eq!(
            set(APP, RING, 0, Clock::Rtc, 0, &[(&long_key, 1)]),
            Err(SetError::TooLong)
        );
        assert_eq!(
            set(APP, RING, 0, Clock::Rtc, 0, &[("a", 1), ("b", 2), ("c", 3)]),
            Err(SetError::TooLong)
        );
        assert_eq!(len(), 0);
    }

    #[test]
    fn a_wall_clock_jump_forward_brings_rtc_alarms_due_and_leaves_elapsed() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 100_000, &[]).unwrap();
        set(APP, RING, 1, Clock::Elapsed, 100_000, &[]).unwrap();
        // The user sets the clock: wall jumps, elapsed does not.
        let jumped = Now {
            wall_ms: 500_000,
            elapsed_ms: 5_000,
        };
        let f = fired(poll(jumped, Some(APP), 1, &always_installed, false));
        assert_eq!(f.request_code, 0, "the RTC alarm is due");
        assert!(
            poll(jumped, Some(APP), 1, &always_installed, false).is_none(),
            "the elapsed alarm is not"
        );
        // Backwards, nothing fires.
        set(APP, RING, 2, Clock::Rtc, 600_000, &[]).unwrap();
        let back = Now {
            wall_ms: 10_000,
            elapsed_ms: 6_000,
        };
        assert!(poll(back, Some(APP), 1, &always_installed, false).is_none());
    }

    #[test]
    fn another_apps_alarm_wakes_its_owner_then_fires() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[("alarm", 4)]).unwrap();
        // The launcher is up; the alarm asks for its own app.
        assert_eq!(woke(poll_as(1_000, Some("launcher"), 1)), APP);
        // Still in that run: the launch is in flight, so no second request.
        assert!(poll_as(1_100, Some("launcher"), 1).is_none());
        assert!(poll_as(1_200, Some("launcher"), 1).is_none());
        // The app is up — a new run, and the entry is delivered.
        let f = fired(poll_as(1_300, Some(APP), 2));
        assert_eq!(f.extra(0), Some(("alarm", 4)));
        assert_eq!(len(), 0);
    }

    #[test]
    fn a_firing_alarm_ignores_set_and_cancel() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[("alarm", 1)]).unwrap();
        assert_eq!(woke(poll_as(1_000, Some("launcher"), 1)), APP);
        // The relaunched app re-arms the same identity from its own store and
        // then cancels it; neither may recall the delivery in flight.
        assert_eq!(
            set(APP, RING, 0, Clock::Rtc, 90_000, &[("alarm", 1)]),
            Ok(false),
            "a fresh row, not a replacement"
        );
        assert!(
            cancel(APP, RING, 0),
            "the cancel removes the fresh armed row, not the delivery"
        );
        let f = fired(poll_as(1_100, Some(APP), 2));
        assert_eq!(f.request_code, 0);
        assert_eq!(f.extra(0), Some(("alarm", 1)));
    }

    #[test]
    fn an_owner_that_never_delivers_is_given_up_on() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        assert_eq!(woke(poll_as(1_000, Some("launcher"), 1)), APP);
        // The app ran and exited without ever polling: a new run, still not it.
        assert_eq!(woke(poll_as(1_100, Some("launcher"), 2)), APP);
        // A third run without delivery gives up rather than looping for ever.
        assert!(poll_as(1_200, Some("launcher"), 3).is_none());
        assert_eq!(len(), 0);
    }

    #[test]
    fn an_app_on_its_way_out_keeps_its_alarm_armed() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        assert!(
            poll(now(1_000), Some(APP), 1, &always_installed, true).is_none(),
            "a queued launch means this stack is about to go"
        );
        assert_eq!(len(), 1, "still armed");
        let f = fired(poll_as(1_000, Some(APP), 1));
        assert_eq!(f.request_code, 0);
    }

    #[test]
    fn an_uninstalled_owners_alarms_are_forgotten() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        set(OTHER, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        let only_other = |p: &str| p == OTHER;
        let f = fired(poll(now(1_000), Some(OTHER), 1, &only_other, false));
        assert_eq!(f.package(), OTHER);
        assert_eq!(len(), 0, "the uninstalled app's alarm went with it");
    }

    #[test]
    fn drop_package_forgets_one_app() {
        let _g = guard();
        set(APP, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        set(APP, RING, 1, Clock::Rtc, 1_000, &[]).unwrap();
        set(OTHER, RING, 0, Clock::Rtc, 1_000, &[]).unwrap();
        assert_eq!(drop_package(APP), 2);
        assert_eq!(len(), 1);
        assert_eq!(drop_package("nobody"), 0);
    }

    #[test]
    fn alarm_types_map_to_two_clocks() {
        assert_eq!(Clock::from_type(0), Some(Clock::Rtc));
        assert_eq!(Clock::from_type(1), Some(Clock::Rtc));
        assert_eq!(Clock::from_type(2), Some(Clock::Elapsed));
        assert_eq!(Clock::from_type(3), Some(Clock::Elapsed));
        assert_eq!(Clock::from_type(4), None);
        assert_eq!(Clock::from_type(-1), None);
    }
}
