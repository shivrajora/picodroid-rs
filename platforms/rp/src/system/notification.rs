// SPDX-License-Identifier: GPL-3.0-only
//! Foreground-service notification tracker.
//!
//! v1 records the currently-visible notification id and logs each post /
//! cancel. Actual on-screen rendering (a persistent LVGL banner) is a
//! follow-up: the plan deferred it because Toast already covers most
//! visual-status needs and a banner needs design work (size, layering vs
//! the active Activity's content view, dismissal gesture).

#![cfg(not(test))]

/// id of the notification currently "visible" (or [`NONE`] for none).
///
/// Plain `static mut` rather than `AtomicI32` — RP2040's Cortex-M0+ has no
/// LL/SC and so doesn't implement `compare_exchange`. The main thread is the
/// only writer (notifications fire from `Service.startForeground` /
/// `Service.stopForeground`, both processed inside the JVM dispatcher on the
/// main thread); nothing reads from another core.
static mut VISIBLE_ID: i32 = NONE;
const NONE: i32 = i32::MIN;

#[inline]
fn visible_id_load() -> i32 {
    // SAFETY: single-threaded access — see the field's doc comment.
    #[allow(static_mut_refs)]
    unsafe {
        VISIBLE_ID
    }
}

#[inline]
fn visible_id_store(v: i32) {
    // SAFETY: single-threaded access — see the field's doc comment.
    #[allow(static_mut_refs)]
    unsafe {
        VISIBLE_ID = v;
    }
}

/// Post (or replace) the foreground-service notification.
///
/// Title/text are already resolved by the caller (the native dispatcher
/// reads them off the Java `Notification` object) so this module needs no
/// JVM access. Empty strings are accepted — they simply render as blank.
pub(crate) fn notify(notif_id: i32, title: &str, text: &str) {
    visible_id_store(notif_id);
    #[cfg(feature = "sim")]
    println!("[notification {}] {}: {}", notif_id, title, text);
    #[cfg(not(feature = "sim"))]
    defmt::info!(
        "[notif {}] {}: {}",
        notif_id,
        defmt::Display2Format(&title),
        defmt::Display2Format(&text),
    );
}

/// Clear the banner if it currently shows notification id `notif_id`.
pub(crate) fn cancel(notif_id: i32) {
    if visible_id_load() != notif_id {
        return;
    }
    visible_id_store(NONE);
    #[cfg(feature = "sim")]
    println!("[notification {}] cancelled", notif_id);
    #[cfg(not(feature = "sim"))]
    defmt::info!("[notif {}] cancelled", notif_id);
}
