// SPDX-License-Identifier: GPL-3.0-only
//! System broadcasts — the framework half of `picodroid.content.BroadcastReceiver`
//! (docs/designs/broadcasts-2026-10.md).
//!
//! The receiver registry lives in Java, inside `BroadcastReceiver`'s statics,
//! where the heap roots it and the reset at an app switch clears it. What
//! this side keeps is small: the *interest mask*, the OR of the system
//! actions some registered filter names, so a tick with no listener costs
//! one load; and the per-source edge state the event loop folds into one
//! `BroadcastReceiver.fireSystem(mask)` upcall per frame
//! (`lifecycle::broadcast_events`).
//!
//! The bits are `BroadcastReceiver`'s `BIT_*` constants; keep both in step.
//! Their order is the delivery order within one frame: a clock set before the
//! tick that follows it.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::hal::net_edge::LinkChanges;

/// `ACTION_TIME_CHANGED`: the wall clock was set.
pub const TIME_CHANGED: u32 = 1;
/// `ACTION_TIMEZONE_CHANGED`: the platform zone moved.
pub const TIMEZONE_CHANGED: u32 = 1 << 1;
/// `ACTION_TIME_TICK`: the wall-clock minute changed.
pub const TIME_TICK: u32 = 1 << 2;
/// `ACTION_SCREEN_OFF`: the panel dozed.
pub const SCREEN_OFF: u32 = 1 << 3;
/// `ACTION_SCREEN_ON`: the panel woke.
pub const SCREEN_ON: u32 = 1 << 4;
/// `ACTION_BOOT_COMPLETED`: manifest receivers only; never raised by the loop.
pub const BOOT_COMPLETED: u32 = 1 << 5;
/// `WifiManager.SUPPLICANT_STATE_CHANGED_ACTION`: the station's state moved.
pub const SUPPLICANT_STATE: u32 = 1 << 6;
/// `WifiManager.NETWORK_STATE_CHANGED_ACTION`: the link came up or dropped.
pub const NETWORK_STATE: u32 = 1 << 7;
/// `WifiManager.SCAN_RESULTS_AVAILABLE_ACTION`: a scan finished.
pub const SCAN_RESULTS: u32 = 1 << 8;
/// `ConnectivityManager.CONNECTIVITY_ACTION`: the link came up or dropped.
pub const CONNECTIVITY: u32 = 1 << 9;

/// A time-service re-anchoring that moves the clock by less than this is a
/// drift correction, not a `TIME_CHANGED`; a set by a user or an app always
/// is one (`os::system_clock::Anchor`).
pub const TIME_SET_STEP_MS: i64 = 2_000;

/// What some registered filter wants. Written by the `nativeSetInterest`
/// native (any Java thread), read by the UI tick: one relaxed atomic.
static INTEREST: AtomicU32 = AtomicU32::new(0);

/// Bumped by every clock set that counts (`anchor_wall_clock`); the loop
/// compares it with the one it last acted on. Single writer at a time: both
/// writers hold the wall clock's `AtomicSection`.
static TIME_SETS: LinkChanges = LinkChanges::new();

/// Bumped by every zone change (`time_service::set_zone_offset_minutes`,
/// the JVM task).
static ZONE_CHANGES: LinkChanges = LinkChanges::new();

/// The screen bits raised since the loop last took them. Written and taken
/// by the UI task alone (`service_display_power`, the dispatch), so a
/// load-then-store is a plain read-modify-write.
static PENDING: AtomicU32 = AtomicU32::new(0);

/// `BroadcastReceiver.nativeSetInterest`: the bits to watch for; 0 is nobody.
pub fn set_interest(mask: u32) {
    INTEREST.store(mask, Ordering::Relaxed);
}

/// The bits some registered filter names.
pub fn interest() -> u32 {
    INTEREST.load(Ordering::Relaxed)
}

/// The previous app's registrations died with its heap; its interest must
/// not outlive them, or the loop would upcall into a class nobody loaded.
/// A screen edge it did not take goes too.
pub fn reset() {
    INTEREST.store(0, Ordering::Relaxed);
    PENDING.store(0, Ordering::Relaxed);
}

/// The wall clock was set in a way that counts as `ACTION_TIME_CHANGED`.
pub fn note_time_set() {
    TIME_SETS.note();
}

/// The generation now; unequal to an earlier read iff a set happened between.
pub fn time_set_generation() -> u32 {
    TIME_SETS.generation()
}

/// The platform zone moved (`ACTION_TIMEZONE_CHANGED`).
pub fn note_zone_changed() {
    ZONE_CHANGES.note();
}

/// The generation now; unequal to an earlier read iff the zone moved between.
pub fn zone_generation() -> u32 {
    ZONE_CHANGES.generation()
}

/// The panel lit (`on`) or went dark: `ACTION_SCREEN_ON` / `ACTION_SCREEN_OFF`
/// at the next dispatch. UI task.
pub fn note_screen(on: bool) {
    let bit = if on { SCREEN_ON } else { SCREEN_OFF };
    PENDING.store(PENDING.load(Ordering::Relaxed) | bit, Ordering::Relaxed);
}

/// The screen bits raised since the last call, and clear them. UI task.
pub fn take_pending() -> u32 {
    let pending = PENDING.load(Ordering::Relaxed);
    PENDING.store(0, Ordering::Relaxed);
    pending
}

// ── Manifest receivers (multi-app boards) ────────────────────────────────────
//
// A `<receiver>` for `BOOT_COMPLETED`, `TIME_SET` or `TIMEZONE_CHANGED` is
// delivered to a package that is not running by running it: the supervisor
// asks the directory what runs next, the directory asks here first, and the
// headless run (`lifecycle::run_application`) hands the receiver class the
// Intent and leaves without an Activity. One app runs at a time, so the
// deliveries wait for a moment nobody is in the foreground — boot, before
// the boot app; an app handing over to the launcher; or the launcher itself,
// which the tick interrupts for them. The running package hears the same
// action through its registered receivers and is never marked.

/// Manifest deliveries waiting at once, across every package.
#[cfg(has_multi_app)]
pub const MANIFEST_PENDING: usize = 8;

#[cfg(has_multi_app)]
struct Manifest {
    pending: [Option<(crate::packages::Name, u32)>; MANIFEST_PENDING],
    /// The package whose headless run is delivering, and what.
    current: Option<(crate::packages::Name, u32)>,
    booted: bool,
    time_set_seen: u32,
    zone_seen: u32,
}

// SAFETY: every entry point runs on the JVM task (the directory's reader,
// the tick, the supervisor between apps) inside an `AtomicSection`.
#[cfg(has_multi_app)]
static MANIFEST: crate::util::section_cell::SectionCell<Manifest> = unsafe {
    crate::util::section_cell::SectionCell::new(Manifest {
        pending: [None; MANIFEST_PENDING],
        current: None,
        booted: false,
        time_set_seen: 0,
        zone_seen: 0,
    })
};

/// The actions a manifest receiver may declare, as bits.
#[cfg(has_multi_app)]
pub const MANIFEST_ACTIONS: u32 = BOOT_COMPLETED | TIME_CHANGED | TIMEZONE_CHANGED;

/// The Android action string for a manifest bit.
#[cfg(has_multi_app)]
pub fn manifest_action(bit: u32) -> &'static str {
    match bit {
        BOOT_COMPLETED => "android.intent.action.BOOT_COMPLETED",
        TIME_CHANGED => "android.intent.action.TIME_SET",
        TIMEZONE_CHANGED => "android.intent.action.TIMEZONE_CHANGED",
        _ => "",
    }
}

/// Mark every installed package with a receiver for `bit`, the running one
/// excepted. Caller holds the section.
#[cfg(has_multi_app)]
fn mark_all(m: &mut Manifest, bit: u32) {
    let action = manifest_action(bit);
    let running = crate::packages::running();
    for e in crate::packages::entries() {
        if Some(e.package()) == running || !e.has_receiver_for(action) {
            continue;
        }
        if let Some(slot) = m.pending.iter_mut().find(|s| {
            s.as_ref()
                .is_some_and(|(n, _)| n.get() == Some(e.package()))
        }) {
            if let Some((_, bits)) = slot {
                *bits |= bit;
            }
            continue;
        }
        match m.pending.iter_mut().find(|s| s.is_none()) {
            Some(slot) => {
                let mut name = crate::packages::Name::EMPTY;
                name.set(Some(e.package()));
                *slot = Some((name, bit));
            }
            None => crate::pd_warn!(
                "[broadcast] manifest {} for {} dropped: table full",
                action,
                e.package()
            ),
        }
    }
}

/// Bring the table up to date: the boot-time marks once, then a mark for
/// every clock set or zone change since the last look. JVM task; cheap when
/// nothing moved (three loads).
#[cfg(has_multi_app)]
pub fn manifest_refresh() {
    let mut atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let m = MANIFEST.get(&mut atomic);
    if !m.booted {
        m.booted = true;
        m.time_set_seen = TIME_SETS.generation();
        m.zone_seen = ZONE_CHANGES.generation();
        mark_all(m, BOOT_COMPLETED);
        return;
    }
    let time_set = TIME_SETS.generation();
    if time_set != m.time_set_seen {
        m.time_set_seen = time_set;
        mark_all(m, TIME_CHANGED);
    }
    let zone = ZONE_CHANGES.generation();
    if zone != m.zone_seen {
        m.zone_seen = zone;
        mark_all(m, TIMEZONE_CHANGED);
    }
}

/// Whether a delivery waits for some package other than `running`.
#[cfg(has_multi_app)]
pub fn manifest_pending_for_other(running: Option<&str>) -> bool {
    let mut atomic = pico_jvm::atomic_section::AtomicSection::enter();
    MANIFEST
        .get(&mut atomic)
        .pending
        .iter()
        .flatten()
        .any(|(n, _)| n.get() != running)
}

/// The next package to run headless for its deliveries, taken off the
/// table and recorded as current; `None` when nothing waits (an entry whose
/// package is gone is dropped).
#[cfg(has_multi_app)]
pub fn manifest_take_next() -> Option<&'static crate::packages::Entry> {
    let mut atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let m = MANIFEST.get(&mut atomic);
    for slot in m.pending.iter_mut() {
        let Some((name, bits)) = slot.take() else {
            continue;
        };
        match name.get().and_then(crate::packages::find) {
            Some(e) => {
                m.current = Some((name, bits));
                return Some(e);
            }
            None => crate::pd_warn!("[broadcast] manifest delivery dropped: package gone"),
        }
    }
    None
}

/// What this run of `package` is to deliver, if it is a manifest run; asked
/// once by `run_application`, which then runs headless.
#[cfg(has_multi_app)]
pub fn manifest_current(package: &str) -> Option<u32> {
    let mut atomic = pico_jvm::atomic_section::AtomicSection::enter();
    let m = MANIFEST.get(&mut atomic);
    match m.current.take() {
        Some((name, bits)) if name.get() == Some(package) => Some(bits),
        other => {
            m.current = other;
            None
        }
    }
}

/// The wall-clock minute `wall_ms` is in. `TIME_TICK` fires when this
/// changes between two ticks; floor division keeps a clock before the epoch
/// (unset, counting from boot) on the same rule.
pub fn minute_of(wall_ms: i64) -> i64 {
    wall_ms.div_euclid(60_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_are_distinct_and_ordered() {
        let bits = [
            TIME_CHANGED,
            TIMEZONE_CHANGED,
            TIME_TICK,
            SCREEN_OFF,
            SCREEN_ON,
            BOOT_COMPLETED,
            SUPPLICANT_STATE,
            NETWORK_STATE,
            SCAN_RESULTS,
            CONNECTIVITY,
        ];
        for (i, b) in bits.iter().enumerate() {
            assert_eq!(b.count_ones(), 1);
            assert_eq!(*b, 1 << i, "delivery order is bit order");
        }
    }

    #[test]
    fn minute_edges() {
        assert_eq!(minute_of(0), 0);
        assert_eq!(minute_of(59_999), 0);
        assert_eq!(minute_of(60_000), 1);
        assert_eq!(minute_of(-1), -1, "a clock before the epoch still ticks");
        assert_eq!(minute_of(1_760_000_039_999), 29_333_333);
        assert_eq!(minute_of(1_760_000_040_000), 29_333_334);
    }

    #[test]
    fn interest_round_trips_and_resets() {
        set_interest(TIME_TICK | SCREEN_ON);
        assert_eq!(interest(), TIME_TICK | SCREEN_ON);
        note_screen(false);
        reset();
        assert_eq!(interest(), 0);
        assert_eq!(take_pending(), 0, "reset drops a screen edge nobody took");
    }

    #[test]
    fn screen_edges_fold_and_clear() {
        reset();
        assert_eq!(take_pending(), 0);
        note_screen(false);
        note_screen(true);
        assert_eq!(
            take_pending(),
            SCREEN_OFF | SCREEN_ON,
            "both edges of one frame"
        );
        assert_eq!(take_pending(), 0, "taken once");
    }

    #[test]
    fn generations_move_on_note() {
        let t = time_set_generation();
        note_time_set();
        assert_ne!(time_set_generation(), t);
        let z = zone_generation();
        note_zone_changed();
        assert_ne!(zone_generation(), z);
    }
}
