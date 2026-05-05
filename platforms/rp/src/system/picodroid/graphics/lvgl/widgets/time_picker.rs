// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `TimePicker` — three side-by-side `lv_roller`s (hour +
//! minute + AM/PM) parented to a flex-row container.
//!
//! The container's `lv_obj_t*` is what we register in `handle_table` and
//! return as the Java `nativeHandle`. The three roller pointers and the
//! current display mode (24-hour vs 12-hour) are tracked in a small static
//! slot table keyed by the container pointer.
//!
//! Either roller emitting `LV_EVENT_VALUE_CHANGED` queues the *container*
//! pointer for dispatch, so a single `fireTimeChanged` callback covers
//! all three axes. Programmatic adjustments (e.g. during `set_is_24hour`'s
//! preserve-time round-trip) set a per-slot `suppress` flag so the
//! trampoline silently drops events while the round-trip is in flight.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

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
    is_24hour: true,
    suppress: false,
};

static mut SLOTS: [PickerSlot; MAX_PICKERS] = [EMPTY_SLOT; MAX_PICKERS];

// ── Change-event ring buffer ────────────────────────────────────────────────

const QUEUE_SIZE: usize = 8;
static mut QUEUE: [usize; QUEUE_SIZE] = [0; QUEUE_SIZE];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;

// ── Container → Java object map ─────────────────────────────────────────────

const MAX_LISTENERS: usize = 4;
static mut HANDLE_MAP: [(usize, u16); MAX_LISTENERS] = [(0, 0); MAX_LISTENERS];
static mut HANDLE_MAP_LEN: usize = 0;

// ── Roller → container map (so the trampoline can resolve the parent) ──────
//
// Three rollers per picker (hour, minute, am_pm).

const MAX_ROLLERS: usize = MAX_PICKERS * 3;
static mut ROLLER_MAP: [(usize, usize); MAX_ROLLERS] = [(0, 0); MAX_ROLLERS];

unsafe extern "C" fn value_changed_cb(e: *mut lv_event_t) {
    let roller = unsafe { lv_event_get_target_obj(e) } as usize;
    unsafe {
        let mut container: usize = 0;
        for &(r, c) in &ROLLER_MAP[..] {
            if r == roller {
                container = c;
                break;
            }
        }
        if container == 0 {
            return;
        }
        // Drop programmatic events — they happen during set_is_24hour /
        // set_time round-trips and should not look like user input.
        for slot in &SLOTS[..] {
            if slot.container == container && slot.suppress {
                return;
            }
        }
        let next = (QUEUE_HEAD + 1) % QUEUE_SIZE;
        if next != QUEUE_TAIL {
            QUEUE[QUEUE_HEAD] = container;
            QUEUE_HEAD = next;
        }
    }
}

pub(in crate::system::picodroid::graphics) fn create() -> i32 {
    unsafe {
        let container = lv_obj_create(lifecycle::screen_ptr());
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

        let hour = lv_roller_create(container);
        // Default mode is 24-hour, infinite scroll feels right for 0..23.
        lv_roller_set_options(
            hour,
            HOURS_24.as_ptr() as *const c_char,
            LV_ROLLER_MODE_INFINITE,
        );
        lv_roller_set_visible_row_count(hour, VISIBLE_ROWS);
        lv_obj_add_event_cb(
            hour,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );

        let minute = lv_roller_create(container);
        lv_roller_set_options(
            minute,
            MINUTES_OPTIONS.as_ptr() as *const c_char,
            LV_ROLLER_MODE_INFINITE,
        );
        lv_roller_set_visible_row_count(minute, VISIBLE_ROWS);
        lv_obj_add_event_cb(
            minute,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );

        // AM/PM roller — NORMAL mode (wrapping over 2 options would be
        // jarring). Hidden initially since 24-hour is the default.
        let am_pm = lv_roller_create(container);
        lv_roller_set_options(
            am_pm,
            AM_PM_OPTIONS.as_ptr() as *const c_char,
            LV_ROLLER_MODE_NORMAL,
        );
        lv_roller_set_visible_row_count(am_pm, VISIBLE_ROWS);
        lv_obj_add_flag(am_pm, LV_OBJ_FLAG_HIDDEN);
        lv_obj_add_event_cb(
            am_pm,
            Some(value_changed_cb),
            LV_EVENT_VALUE_CHANGED,
            core::ptr::null_mut(),
        );

        register_picker(
            container as usize,
            hour as usize,
            minute as usize,
            am_pm as usize,
        );
        handle_table::register(container)
    }
}

pub(in crate::system::picodroid::graphics) fn set_time(id: i32, hour: i32, minute: i32) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let slot_idx = match find_slot_idx(container as usize) {
        Some(i) => i,
        None => return,
    };
    let m = minute.clamp(0, 59) as u32;
    let h24 = hour.clamp(0, 23);
    apply_hour_and_minute(slot_idx, h24, m);
}

pub(in crate::system::picodroid::graphics) fn get_hour(id: i32) -> i32 {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return 0;
    }
    let slot = match find_slot(container as usize) {
        Some(s) => s,
        None => return 0,
    };
    let hour_ptr = slot.hour as *mut lv_obj_t;
    if hour_ptr.is_null() {
        return 0;
    }
    let hour_idx = unsafe { lv_roller_get_selected(hour_ptr) } as i32;
    if slot.is_24hour {
        hour_idx
    } else {
        let am_pm_ptr = slot.am_pm as *mut lv_obj_t;
        let pm = !am_pm_ptr.is_null() && unsafe { lv_roller_get_selected(am_pm_ptr) } == 1;
        twelve_hour_to_24(hour_idx, pm)
    }
}

pub(in crate::system::picodroid::graphics) fn get_minute(id: i32) -> i32 {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return 0;
    }
    if let Some(slot) = find_slot(container as usize) {
        let m_ptr = slot.minute as *mut lv_obj_t;
        if !m_ptr.is_null() {
            return unsafe { lv_roller_get_selected(m_ptr) as i32 };
        }
    }
    0
}

/// `setIs24HourView(boolean)` — runtime toggle that preserves the currently
/// selected time across the switch.
pub(in crate::system::picodroid::graphics) fn set_is_24hour(id: i32, is_24hour: bool) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let slot_idx = match find_slot_idx(container as usize) {
        Some(i) => i,
        None => return,
    };
    let prev_24hour = unsafe { SLOTS[slot_idx].is_24hour };
    if prev_24hour == is_24hour {
        return;
    }
    // Snapshot the current 24-hour value BEFORE mutating mode/options so
    // the round-trip preserves what the user was looking at.
    let saved_h = read_24hour(slot_idx);
    let saved_m = unsafe {
        let m_ptr = SLOTS[slot_idx].minute as *mut lv_obj_t;
        if m_ptr.is_null() {
            0
        } else {
            lv_roller_get_selected(m_ptr) as i32
        }
    };

    unsafe {
        SLOTS[slot_idx].suppress = true;
        SLOTS[slot_idx].is_24hour = is_24hour;

        let hour_ptr = SLOTS[slot_idx].hour as *mut lv_obj_t;
        let am_pm_ptr = SLOTS[slot_idx].am_pm as *mut lv_obj_t;

        if is_24hour {
            // 24-hour: swap to the 0..23 list, hide AM/PM column.
            if !hour_ptr.is_null() {
                lv_roller_set_options(
                    hour_ptr,
                    HOURS_24.as_ptr() as *const c_char,
                    LV_ROLLER_MODE_INFINITE,
                );
            }
            if !am_pm_ptr.is_null() {
                lv_obj_add_flag(am_pm_ptr, LV_OBJ_FLAG_HIDDEN);
            }
        } else {
            // 12-hour: swap to 12,01..11 (NORMAL, no wrap), reveal AM/PM.
            if !hour_ptr.is_null() {
                lv_roller_set_options(
                    hour_ptr,
                    HOURS_12.as_ptr() as *const c_char,
                    LV_ROLLER_MODE_NORMAL,
                );
            }
            if !am_pm_ptr.is_null() {
                lv_obj_remove_flag(am_pm_ptr, LV_OBJ_FLAG_HIDDEN);
            }
        }
    }

    apply_hour_and_minute(slot_idx, saved_h, saved_m as u32);

    unsafe { SLOTS[slot_idx].suppress = false };
}

pub(in crate::system::picodroid::graphics) fn is_24hour(id: i32) -> bool {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return true;
    }
    find_slot(container as usize)
        .map(|s| s.is_24hour)
        .unwrap_or(true)
}

pub(in crate::system::picodroid::graphics) fn register_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        for entry in &mut HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == raw_ptr {
                entry.1 = obj_ref;
                return;
            }
        }
        if HANDLE_MAP_LEN < MAX_LISTENERS {
            HANDLE_MAP[HANDLE_MAP_LEN] = (raw_ptr, obj_ref);
            HANDLE_MAP_LEN += 1;
        }
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn drain_time_picker_queue() -> Option<usize> {
    unsafe {
        if QUEUE_TAIL == QUEUE_HEAD {
            return None;
        }
        let h = QUEUE[QUEUE_TAIL];
        QUEUE_TAIL = (QUEUE_TAIL + 1) % QUEUE_SIZE;
        Some(h)
    }
}

#[cfg_attr(feature = "sim", allow(dead_code))]
pub fn lookup_time_picker_obj(handle: usize) -> Option<u16> {
    unsafe {
        for entry in &HANDLE_MAP[..HANDLE_MAP_LEN] {
            if entry.0 == handle {
                return Some(entry.1);
            }
        }
    }
    None
}

pub fn reset_time_picker_state() {
    unsafe {
        for slot in &mut SLOTS[..] {
            *slot = EMPTY_SLOT;
        }
        for entry in &mut ROLLER_MAP[..] {
            *entry = (0, 0);
        }
        HANDLE_MAP_LEN = 0;
        QUEUE_HEAD = 0;
        QUEUE_TAIL = 0;
    }
}

// ── Internals ───────────────────────────────────────────────────────────────

fn register_picker(container: usize, hour: usize, minute: usize, am_pm: usize) {
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.container == 0 {
                *slot = PickerSlot {
                    container,
                    hour,
                    minute,
                    am_pm,
                    is_24hour: true,
                    suppress: false,
                };
                break;
            }
        }
        for &r in &[hour, minute, am_pm] {
            for entry in &mut ROLLER_MAP[..] {
                if entry.0 == 0 {
                    *entry = (r, container);
                    break;
                }
            }
        }
    }
}

fn find_slot(container: usize) -> Option<PickerSlot> {
    unsafe {
        for slot in &SLOTS[..] {
            if slot.container == container {
                return Some(*slot);
            }
        }
    }
    None
}

fn find_slot_idx(container: usize) -> Option<usize> {
    unsafe { SLOTS[..].iter().position(|s| s.container == container) }
}

/// Read the current 24-hour value from the rollers (independent of mode).
fn read_24hour(slot_idx: usize) -> i32 {
    unsafe {
        let slot = SLOTS[slot_idx];
        let h_ptr = slot.hour as *mut lv_obj_t;
        if h_ptr.is_null() {
            return 0;
        }
        let h_idx = lv_roller_get_selected(h_ptr) as i32;
        if slot.is_24hour {
            h_idx
        } else {
            let am_pm_ptr = slot.am_pm as *mut lv_obj_t;
            let pm = !am_pm_ptr.is_null() && lv_roller_get_selected(am_pm_ptr) == 1;
            twelve_hour_to_24(h_idx, pm)
        }
    }
}

/// Apply a 24-hour `(hour, minute)` to the rollers, translating into the
/// current display mode. Caller is responsible for setting/clearing the
/// `suppress` flag if the call shouldn't fire `fireTimeChanged`.
fn apply_hour_and_minute(slot_idx: usize, hour_24: i32, minute: u32) {
    unsafe {
        let slot = SLOTS[slot_idx];
        let h_ptr = slot.hour as *mut lv_obj_t;
        let m_ptr = slot.minute as *mut lv_obj_t;
        let am_pm_ptr = slot.am_pm as *mut lv_obj_t;

        if slot.is_24hour {
            if !h_ptr.is_null() {
                lv_roller_set_selected(h_ptr, hour_24 as u32, LV_ANIM_OFF);
            }
        } else {
            let (idx, pm) = twentyfour_to_12(hour_24);
            if !h_ptr.is_null() {
                lv_roller_set_selected(h_ptr, idx, LV_ANIM_OFF);
            }
            if !am_pm_ptr.is_null() {
                lv_roller_set_selected(am_pm_ptr, if pm { 1 } else { 0 }, LV_ANIM_OFF);
            }
        }
        if !m_ptr.is_null() {
            lv_roller_set_selected(m_ptr, minute, LV_ANIM_OFF);
        }
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
