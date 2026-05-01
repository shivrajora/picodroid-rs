//! LVGL impl of `TimePicker` — two side-by-side `lv_roller`s (hour + minute)
//! parented to a flex-row container.
//!
//! The container's `lv_obj_t*` is what we register in `handle_table` and
//! return as the Java `nativeHandle`. The hour and minute roller pointers
//! are tracked in a small static slot table keyed by the container pointer,
//! so `getHour`/`getMinute`/`setTime` can resolve the children without a
//! second handle table.
//!
//! Either roller emitting `LV_EVENT_VALUE_CHANGED` queues the *container*
//! pointer for dispatch, so a single `fireTimeChanged` callback covers
//! both axes.

use crate::lvgl_ffi::*;
use core::ffi::c_char;

use super::super::handle_table;
use super::super::lifecycle;

const HOURS_OPTIONS: &[u8] = b"00\n01\n02\n03\n04\n05\n06\n07\n08\n09\n10\n11\n\
12\n13\n14\n15\n16\n17\n18\n19\n20\n21\n22\n23\0";

const MINUTES_OPTIONS: &[u8] = b"00\n01\n02\n03\n04\n05\n06\n07\n08\n09\n\
10\n11\n12\n13\n14\n15\n16\n17\n18\n19\n\
20\n21\n22\n23\n24\n25\n26\n27\n28\n29\n\
30\n31\n32\n33\n34\n35\n36\n37\n38\n39\n\
40\n41\n42\n43\n44\n45\n46\n47\n48\n49\n\
50\n51\n52\n53\n54\n55\n56\n57\n58\n59\0";

const VISIBLE_ROWS: u32 = 3;

const MAX_PICKERS: usize = 4;

#[derive(Copy, Clone)]
struct PickerSlot {
    container: usize,
    hour: usize,
    minute: usize,
}

const EMPTY_SLOT: PickerSlot = PickerSlot {
    container: 0,
    hour: 0,
    minute: 0,
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

const MAX_ROLLERS: usize = MAX_PICKERS * 2;
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
        lv_obj_set_size(container, 160, 100);
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
        lv_roller_set_options(
            hour,
            HOURS_OPTIONS.as_ptr() as *const c_char,
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

        register_picker(container as usize, hour as usize, minute as usize);
        handle_table::register(container)
    }
}

pub(in crate::system::picodroid::graphics) fn set_time(id: i32, hour: i32, minute: i32) {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return;
    }
    let (h_ptr, m_ptr) = match find_rollers(container as usize) {
        Some(v) => v,
        None => return,
    };
    let h = hour.clamp(0, 23) as u32;
    let m = minute.clamp(0, 59) as u32;
    unsafe {
        if !h_ptr.is_null() {
            lv_roller_set_selected(h_ptr, h, LV_ANIM_OFF);
        }
        if !m_ptr.is_null() {
            lv_roller_set_selected(m_ptr, m, LV_ANIM_OFF);
        }
    }
}

pub(in crate::system::picodroid::graphics) fn get_hour(id: i32) -> i32 {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return 0;
    }
    if let Some((h_ptr, _)) = find_rollers(container as usize) {
        if !h_ptr.is_null() {
            return unsafe { lv_roller_get_selected(h_ptr) as i32 };
        }
    }
    0
}

pub(in crate::system::picodroid::graphics) fn get_minute(id: i32) -> i32 {
    let container = handle_table::lookup(id);
    if container.is_null() {
        return 0;
    }
    if let Some((_, m_ptr)) = find_rollers(container as usize) {
        if !m_ptr.is_null() {
            return unsafe { lv_roller_get_selected(m_ptr) as i32 };
        }
    }
    0
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

fn register_picker(container: usize, hour: usize, minute: usize) {
    unsafe {
        for slot in &mut SLOTS[..] {
            if slot.container == 0 {
                *slot = PickerSlot {
                    container,
                    hour,
                    minute,
                };
                break;
            }
        }
        for &r in &[hour, minute] {
            for entry in &mut ROLLER_MAP[..] {
                if entry.0 == 0 {
                    *entry = (r, container);
                    break;
                }
            }
        }
    }
}

fn find_rollers(container: usize) -> Option<(*mut lv_obj_t, *mut lv_obj_t)> {
    unsafe {
        for slot in &SLOTS[..] {
            if slot.container == container {
                return Some((slot.hour as *mut lv_obj_t, slot.minute as *mut lv_obj_t));
            }
        }
    }
    None
}
