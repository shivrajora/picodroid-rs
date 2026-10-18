// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `TimePicker` — three side-by-side `lv_roller`s (hour +
//! minute + AM/PM) parented to a flex-row container.
//!
//! The container's `lv_obj_t*` is what we register in `handle_table` and
//! return as the Java `nativeHandle`. The three roller pointers and the
//! current display mode (24-hour vs 12-hour) are tracked in a small static
//! slot table keyed by the container pointer.
//!
//! Either roller emitting `LV_EVENT_VALUE_CHANGED` queues the *container's*
//! handle for dispatch, so a single `fireTimeChanged` callback covers
//! all three axes. Programmatic adjustments (e.g. during `set_is_24hour`'s
//! preserve-time round-trip) set a per-slot `suppress` flag so the
//! trampoline silently drops events while the round-trip is in flight.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

const HOURS_24: &[u8] = b"00\n01\n02\n03\n04\n05\n06\n07\n08\n09\n10\n11\n\
12\n13\n14\n15\n16\n17\n18\n19\n20\n21\n22\n23\0";

/// 12-hour ordering matches Android: "12" comes first (representing 12 AM
/// / 12 PM at index 0), then 01..11 at indices 1..11.
const HOURS_12: &[u8] = b"12\n01\n02\n03\n04\n05\n06\n07\n08\n09\n10\n11\0";

const MINUTES_OPTIONS: &[u8] = b"00\n01\n02\n03\n04\n05\n06\n07\n08\n09\n\
10\n11\n12\n13\n14\n15\n16\n17\n18\n19\n\
20\n21\n22\n23\n24\n25\n26\n27\n28\n29\n\
30\n31\n32\n33\n34\n35\n36\n37\n38\n39\n\
40\n41\n42\n43\n44\n45\n46\n47\n48\n49\n\
50\n51\n52\n53\n54\n55\n56\n57\n58\n59\0";

const AM_PM_OPTIONS: &[u8] = b"AM\nPM\0";

const VISIBLE_ROWS: u32 = 3;

const MAX_PICKERS: usize = 4;

#[derive(Copy, Clone)]
struct PickerSlot {
    container: usize,
    hour: usize,
    minute: usize,
    am_pm: usize,
    is_24hour: bool,
    /// Set during programmatic round-trips (e.g. `set_is_24hour` rebuilding
    /// hour/AM-PM selections after swapping options). The trampoline
    /// checks this and drops the event so the user-visible
    /// `fireTimeChanged` callback only fires for actual taps/drags.
    suppress: bool,
}

const EMPTY_SLOT: PickerSlot = PickerSlot {
    container: 0,
    hour: 0,
    minute: 0,
    am_pm: 0,
    // Never read: a free slot is matched by no container, and
    // `register_picker` writes every field. False keeps the table in .bss.
    is_24hour: false,
    suppress: false,
};

// ── Change-event ring buffer (container handles) ────────────────────────────

const QUEUE_SIZE: usize = 8;

// ── Roller → container map (so the trampoline can resolve the parent) ──────
//
// Three rollers per picker (hour, minute, am_pm).

const MAX_ROLLERS: usize = MAX_PICKERS * 3;

struct State {
    slots: [Cell<PickerSlot>; MAX_PICKERS],
    queue: LocalRing<i32, QUEUE_SIZE>,
    roller_map: [Cell<(usize, usize)>; MAX_ROLLERS],
}

// SAFETY: filled by natives and LVGL callbacks, drained by natives, all on
// core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const { Cell::new(EMPTY_SLOT) }; MAX_PICKERS],
        queue: LocalRing::new(0),
        roller_map: [const { Cell::new((0, 0)) }; MAX_ROLLERS],
    })
};

lv_event_cb! {
    /// Release a picker's `SLOTS` entry and its three `ROLLER_MAP` entries when the
    /// container is deleted. Registered on every picker at create time, not only on
    /// ones that took a listener: without it the four slots are consumed for good
    /// and the *fifth* `TimePicker` a process ever builds gets none, at which point
    /// `set_time` silently does nothing and `get_hour`/`get_minute` answer zero —
    /// a screen offering midnight instead of the time, and setting midnight if the
    /// user accepts it. Screens are built and destroyed freely, so four is a handful
    /// of navigations, not a lifetime's worth.
    fn slot_delete_cb(e) {
        let container = e.target().addr();
        if container == 0 {
            return;
        }
        for slot in &STATE.slots {
            if slot.get().container == container {
                slot.set(EMPTY_SLOT);
            }
        }
        for entry in &STATE.roller_map {
            if entry.get().1 == container {
                entry.set((0, 0));
            }
        }
    }
}

lv_event_cb! {
    fn value_changed_cb(e) {
        let roller = e.target().addr();
        let container = STATE
            .roller_map
            .iter()
            .map(Cell::get)
            .find(|&(r, _)| r == roller)
            .map_or(0, |(_, c)| c);
        if container == 0 {
            return;
        }
        // Drop programmatic events — they happen during set_is_24hour /
        // set_time round-trips and should not look like user input.
        if STATE
            .slots
            .iter()
            .map(Cell::get)
            .any(|slot| slot.container == container && slot.suppress)
        {
            return;
        }
        // The roller is a direct child of its container (`add_roller`).
        let handle = lv_obj_get_parent(e.target()).map_or(0, handle_table::handle_of);
        if handle != 0 {
            STATE.queue.push(handle);
        }
    }
}

/// One roller of a picker: made by `lv_roller_create`, queued on
/// VALUE_CHANGED and on KEY (a key-driven change — `lv_roller_set_selected`
/// from LVGL's own key handler, docs/designs/app-portability-2026-10.md K7 —
/// sends no VALUE_CHANGED; the KEY event, which runs after the class handled
/// it, is the same notice).
fn add_roller(container: Obj, options: &[u8], mode: lv_roller_mode_t) -> Option<Obj> {
    let roller = lv_roller_create(container)?;
    // SAFETY: `roller` was just made by `lv_roller_create`; `options` is one
    // of this module's NUL-terminated option lists, which LVGL copies.
    unsafe {
        lv_roller_set_options(roller, options.as_ptr() as *const c_char, mode);
        lv_roller_set_visible_row_count(roller, VISIBLE_ROWS);
    }
    add_event_cb(roller, value_changed_cb, LV_EVENT_VALUE_CHANGED);
    add_event_cb(roller, value_changed_cb, LV_EVENT_KEY);
    Some(roller)
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(container) = lifecycle::screen_ptr().and_then(|o| lv_obj_create(o)) else {
        return 0;
    };
    lv_obj_set_size(container, 200, 100);
    lv_obj_set_style_pad_left(container, 0, 0);
    lv_obj_set_style_pad_right(container, 0, 0);
    lv_obj_set_style_pad_top(container, 0, 0);
    lv_obj_set_style_pad_bottom(container, 0, 0);
    lv_obj_set_flex_flow(container, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(
        container,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
        LV_FLEX_ALIGN_CENTER,
    );

    // Default mode is 24-hour, infinite scroll feels right for 0..23.
    let hour = add_roller(container, HOURS_24, LV_ROLLER_MODE_INFINITE);
    let minute = add_roller(container, MINUTES_OPTIONS, LV_ROLLER_MODE_INFINITE);
    // AM/PM roller — NORMAL mode (wrapping over 2 options would be
    // jarring). Hidden initially since 24-hour is the default.
    let am_pm = add_roller(container, AM_PM_OPTIONS, LV_ROLLER_MODE_NORMAL);
    if let Some(am_pm) = am_pm {
        lv_obj_add_flag(am_pm, LV_OBJ_FLAG_HIDDEN);
    }

    register_picker(
        container.addr(),
        hour.map_or(0, Obj::addr),
        minute.map_or(0, Obj::addr),
        am_pm.map_or(0, Obj::addr),
    );
    add_event_cb(container, slot_delete_cb, LV_EVENT_DELETE);
    handle_table::register(container)
}

/// A roller `create` made with `lv_roller_create`, as [`rollers`] answers it.
#[derive(Copy, Clone)]
struct Roller(Obj);

/// Slot `idx`'s three rollers, each `None` if it could not be made.
fn rollers(idx: usize) -> [Option<Roller>; 3] {
    let slot = STATE.slots[idx].get();
    // SAFETY: a slot names the rollers `create` made under its container,
    // from `register_picker` until `slot_delete_cb` clears it, which LVGL
    // runs on the container's delete before it frees the container's
    // children.
    [slot.hour, slot.minute, slot.am_pm]
        .map(|r| unsafe { Obj::from_raw(r as *mut lv_obj_t) }.map(Roller))
}

/// The selected row of a roller from [`rollers`].
fn selected(roller: Roller) -> u32 {
    // SAFETY: a `Roller` was made by `lv_roller_create`.
    unsafe { lv_roller_get_selected(roller.0) }
}

/// Select row `row` of a roller from [`rollers`], without animation.
fn select(roller: Roller, row: u32) {
    // SAFETY: as in [`selected`].
    unsafe { lv_roller_set_selected(roller.0, row, LV_ANIM_OFF) };
}

/// The slot of `id`'s picker, if the handle is live and has one.
fn slot_of(id: i32) -> Option<usize> {
    find_slot_idx(handle_table::get(id)?.addr())
}

pub(in crate::graphics) fn set_time(id: i32, hour: i32, minute: i32) {
    let Some(slot_idx) = slot_of(id) else {
        return;
    };
    let m = minute.clamp(0, 59) as u32;
    let h24 = hour.clamp(0, 23);
    apply_hour_and_minute(slot_idx, h24, m);
}

pub(in crate::graphics) fn get_hour(id: i32) -> i32 {
    match slot_of(id) {
        Some(slot_idx) => read_24hour(slot_idx),
        None => 0,
    }
}

pub(in crate::graphics) fn get_minute(id: i32) -> i32 {
    let Some(slot_idx) = slot_of(id) else {
        return 0;
    };
    let [_, minute, _] = rollers(slot_idx);
    minute.map_or(0, |m| selected(m) as i32)
}

/// `setIs24HourView(boolean)` — runtime toggle that preserves the currently
/// selected time across the switch.
pub(in crate::graphics) fn set_is_24hour(id: i32, is_24hour: bool) {
    let Some(slot_idx) = slot_of(id) else {
        return;
    };
    let cell = &STATE.slots[slot_idx];
    let prev_24hour = cell.get().is_24hour;
    if prev_24hour == is_24hour {
        return;
    }
    // Snapshot the current 24-hour value BEFORE mutating mode/options so
    // the round-trip preserves what the user was looking at.
    let saved_h = read_24hour(slot_idx);
    let [hour, minute, am_pm] = rollers(slot_idx);
    let saved_m = minute.map_or(0, |m| selected(m) as i32);

    cell.set(PickerSlot {
        suppress: true,
        is_24hour,
        ..cell.get()
    });
    if is_24hour {
        // 24-hour: swap to the 0..23 list, hide AM/PM column.
        if let Some(hour) = hour {
            // SAFETY: a `Roller` was made by `lv_roller_create`; the options
            // are a NUL-terminated static LVGL copies.
            unsafe {
                lv_roller_set_options(
                    hour.0,
                    HOURS_24.as_ptr() as *const c_char,
                    LV_ROLLER_MODE_INFINITE,
                );
            }
        }
        if let Some(am_pm) = am_pm {
            lv_obj_add_flag(am_pm.0, LV_OBJ_FLAG_HIDDEN);
        }
    } else {
        // 12-hour: swap to 12,01..11 (NORMAL, no wrap), reveal AM/PM.
        if let Some(hour) = hour {
            // SAFETY: as above.
            unsafe {
                lv_roller_set_options(
                    hour.0,
                    HOURS_12.as_ptr() as *const c_char,
                    LV_ROLLER_MODE_NORMAL,
                );
            }
        }
        if let Some(am_pm) = am_pm {
            lv_obj_remove_flag(am_pm.0, LV_OBJ_FLAG_HIDDEN);
        }
    }

    apply_hour_and_minute(slot_idx, saved_h, saved_m as u32);

    cell.set(PickerSlot {
        suppress: false,
        ..cell.get()
    });
}

pub(in crate::graphics) fn is_24hour(id: i32) -> bool {
    let Some(container) = handle_table::get(id) else {
        return true;
    };
    find_slot(container.addr())
        .map(|s| s.is_24hour)
        .unwrap_or(true)
}

pub fn drain_time_picker_queue() -> Option<i32> {
    STATE.queue.pop()
}

pub fn reset_time_picker_state() {
    for slot in &STATE.slots {
        slot.set(EMPTY_SLOT);
    }
    for entry in &STATE.roller_map {
        entry.set((0, 0));
    }
    STATE.queue.clear();
}

// ── Internals ───────────────────────────────────────────────────────────────

fn register_picker(container: usize, hour: usize, minute: usize, am_pm: usize) {
    match STATE.slots.iter().find(|s| s.get().container == 0) {
        Some(slot) => slot.set(PickerSlot {
            container,
            hour,
            minute,
            am_pm,
            is_24hour: true,
            suppress: false,
        }),
        // A picker with no slot is inert: set_time does nothing and the
        // getters answer zero. Say so rather than hand back a widget that
        // quietly ignores every call made on it.
        None => crate::pd_warn!("TimePicker: the slot table is full, this picker is inert"),
    }
    for &r in &[hour, minute, am_pm] {
        if let Some(entry) = STATE.roller_map.iter().find(|e| e.get().0 == 0) {
            entry.set((r, container));
        }
    }
}

fn find_slot(container: usize) -> Option<PickerSlot> {
    for slot in &STATE.slots {
        let slot = slot.get();
        if slot.container == container {
            return Some(slot);
        }
    }
    None
}

fn find_slot_idx(container: usize) -> Option<usize> {
    STATE
        .slots
        .iter()
        .position(|s| s.get().container == container)
}

/// Read the current 24-hour value from the rollers (independent of mode).
fn read_24hour(slot_idx: usize) -> i32 {
    let [hour, _, am_pm] = rollers(slot_idx);
    let Some(hour) = hour else {
        return 0;
    };
    let h_idx = selected(hour) as i32;
    if STATE.slots[slot_idx].get().is_24hour {
        h_idx
    } else {
        let pm = am_pm.is_some_and(|r| selected(r) == 1);
        twelve_hour_to_24(h_idx, pm)
    }
}

/// Apply a 24-hour `(hour, minute)` to the rollers, translating into the
/// current display mode. Caller is responsible for setting/clearing the
/// `suppress` flag if the call shouldn't fire `fireTimeChanged`.
fn apply_hour_and_minute(slot_idx: usize, hour_24: i32, minute: u32) {
    let [hour, minute_roller, am_pm] = rollers(slot_idx);
    if STATE.slots[slot_idx].get().is_24hour {
        if let Some(hour) = hour {
            select(hour, hour_24 as u32);
        }
    } else {
        let (idx, pm) = twentyfour_to_12(hour_24);
        if let Some(hour) = hour {
            select(hour, idx);
        }
        if let Some(am_pm) = am_pm {
            select(am_pm, if pm { 1 } else { 0 });
        }
    }
    if let Some(m) = minute_roller {
        select(m, minute);
    }
}

/// 12-hour display → 24-hour value. `idx` is the hour roller index in the
/// 12-hour list (0 = "12", 1..11 = "1".."11").
fn twelve_hour_to_24(idx: i32, pm: bool) -> i32 {
    let i = idx.clamp(0, 11);
    match (i, pm) {
        (0, false) => 0,     // 12 AM = 0
        (h, false) => h,     // 1..11 AM = 1..11
        (0, true) => 12,     // 12 PM = 12
        (h, true) => h + 12, // 1..11 PM = 13..23
    }
}

/// 24-hour value → (hour-roller idx in HOURS_12, pm flag).
fn twentyfour_to_12(hour_24: i32) -> (u32, bool) {
    let h = hour_24.clamp(0, 23);
    match h {
        0 => (0, false),              // 0 = 12 AM
        1..=11 => (h as u32, false),  // 1..11 AM
        12 => (0, true),              // 12 = 12 PM
        _ => ((h - 12) as u32, true), // 13..23 = 1..11 PM
    }
}

// Pure arithmetic only: LVGL is linked into the test binary, not initialised.
#[cfg(test)]
mod tests {
    use super::{twelve_hour_to_24, twentyfour_to_12};

    /// Midnight and noon are where a 12-hour clock goes wrong: both show
    /// "12", which is roller index 0, and differ only in AM/PM.
    #[test]
    fn midnight_and_noon_are_both_twelve_on_the_roller() {
        assert_eq!(twentyfour_to_12(0), (0, false));
        assert_eq!(twentyfour_to_12(12), (0, true));
        assert_eq!(twelve_hour_to_24(0, false), 0);
        assert_eq!(twelve_hour_to_24(0, true), 12);
    }

    #[test]
    fn the_hours_either_side_of_the_boundaries() {
        assert_eq!(twentyfour_to_12(11), (11, false));
        assert_eq!(twentyfour_to_12(13), (1, true));
        assert_eq!(twentyfour_to_12(23), (11, true));
        assert_eq!(twelve_hour_to_24(11, false), 11);
        assert_eq!(twelve_hour_to_24(1, true), 13);
        assert_eq!(twelve_hour_to_24(11, true), 23);
    }

    /// Switching a picker between 12- and 24-hour mode must never move the
    /// time it shows.
    #[test]
    fn every_hour_survives_the_round_trip() {
        for hour in 0..24 {
            let (idx, pm) = twentyfour_to_12(hour);
            assert!(idx < 12, "hour {hour} -> roller index {idx}");
            assert_eq!(twelve_hour_to_24(idx as i32, pm), hour);
        }
    }

    #[test]
    fn out_of_range_inputs_clamp_instead_of_wrapping() {
        assert_eq!(twentyfour_to_12(-5), (0, false));
        assert_eq!(twentyfour_to_12(99), (11, true));
        assert_eq!(twelve_hour_to_24(-1, false), 0);
        assert_eq!(twelve_hour_to_24(40, true), 23);
    }
}
