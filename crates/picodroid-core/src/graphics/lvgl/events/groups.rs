// SPDX-License-Identifier: GPL-3.0-only
//! Keypad focus groups: one LVGL group per Activity on the back stack, a
//! modal group above them, and the focus / focusable helpers views call.

use super::*;

// ── Per-Activity keypad focus groups ────────────────────────────────────────
//
// Android gives every Activity its own Window with an isolated focus scope: a
// backgrounded Activity's focus is retained untouched and restored on resume,
// and one Activity can never traverse into another's focus. We mirror that with
// one `lv_group` per Activity. While an Activity is on top its group is both
// the LVGL *default* group (so the Activity's group-def widgets — Button,
// EditText, … — auto-join IT, not a shared global) and the keypad indev's group
// (so PREV/NEXT navigation stays within it). Push creates the child's group;
// pop deletes it and reactivates the parent's, whose focus state is intact —
// no cross-Activity focus bleed, and resume-focus needs no special handling.
//
// The group stack is kept in lockstep with the framework Activity stack by
// `push_activity_group`/`pop_activity_group` calls from `lifecycle.rs` at the
// same points it pushes/pops Activities.

/// Upper bound on nested Activities, matching the documented range of the
/// `[jvm] activity_stack_depth` tunable (1..=32). The framework Activity stack
/// caps depth first, so this group stack never overflows.
#[cfg(has_buttons)]
pub(super) const MAX_ACTIVITY_GROUPS: usize = 32;

// Pin the "Activity stack caps depth first" claim: a board.toml raising
// `activity_stack_depth` past this table would make push_activity_group a
// silent no-op while pop_activity_group still decrements — desyncing the
// group stack from the Activity stack.
#[cfg(has_buttons)]
const _: () = assert!(crate::board_cfg::jvm_state::ACTIVITY_STACK_DEPTH <= MAX_ACTIVITY_GROUPS);

#[cfg(has_buttons)]
pub(super) static mut KEYPAD_INDEV: *mut lv_indev_t = core::ptr::null_mut();

#[cfg(has_buttons)]
pub(super) static mut ACTIVITY_GROUPS: [*mut lv_group_t; MAX_ACTIVITY_GROUPS] =
    [core::ptr::null_mut(); MAX_ACTIVITY_GROUPS];

#[cfg(has_buttons)]
pub(super) static mut ACTIVITY_GROUP_DEPTH: usize = 0;

/// The group a modal dialog's buttons live in while any dialog is shown
/// ([`enter_modal_group`]); null between dialogs.
#[cfg(has_buttons)]
pub(super) static mut MODAL_GROUP: *mut lv_group_t = core::ptr::null_mut();

/// Create a fresh focus group for a newly-launched Activity and make it the
/// active group (LVGL default + keypad indev). Called from the lifecycle
/// bootstrap/push paths *before* the Activity's `onCreate`, so its group-def
/// widgets auto-join this group.
#[cfg(has_buttons)]
pub fn push_activity_group() {
    unsafe {
        if ACTIVITY_GROUP_DEPTH >= MAX_ACTIVITY_GROUPS {
            return; // unreachable: the Activity stack caps depth first
        }
        let group = lv_group_create();
        lv_group_set_default(group);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, group);
        }
        ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH] = group;
        ACTIVITY_GROUP_DEPTH += 1;
    }
}

/// Tear down the top Activity's focus group and reactivate the parent's (or
/// none if the stack is now empty). Called from the lifecycle pop path *after*
/// the popped Activity's view tree is deleted. The parent group is reattached
/// to the indev before the child group is freed, so the indev never references
/// a deleted group.
#[cfg(has_buttons)]
pub fn pop_activity_group() {
    unsafe {
        if ACTIVITY_GROUP_DEPTH == 0 {
            return;
        }
        ACTIVITY_GROUP_DEPTH -= 1;
        let group = ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH];
        ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH] = core::ptr::null_mut();

        let parent = if ACTIVITY_GROUP_DEPTH > 0 {
            ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH - 1]
        } else {
            core::ptr::null_mut()
        };
        // A dialog still up on the parent keeps the keypad: its buttons are
        // in the modal group, not the parent's.
        let active = if MODAL_GROUP.is_null() {
            parent
        } else {
            MODAL_GROUP
        };
        lv_group_set_default(active);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, active);
        }
        if !group.is_null() {
            lv_group_delete(group);
        }
    }
}

/// The keypad group a modal dialog's buttons go in while any dialog is
/// shown. Created by the first `show`, it replaces the Activity's group as
/// LVGL's default and the keypad's, so NEXT and PREV cycle the dialog's own
/// buttons and cannot walk onto the rows behind the scrim — "down, down,
/// select" from an uninstall dialog used to confirm a *different* app. Null
/// when no Activity group is active, as `lv_group_get_default` was.
#[cfg(has_buttons)]
pub fn enter_modal_group() -> *mut lv_group_t {
    unsafe {
        if ACTIVITY_GROUP_DEPTH == 0 {
            return core::ptr::null_mut();
        }
        if MODAL_GROUP.is_null() {
            MODAL_GROUP = lv_group_create();
        }
        lv_group_set_default(MODAL_GROUP);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, MODAL_GROUP);
        }
        MODAL_GROUP
    }
}

/// The last dialog is gone: the top Activity's group takes the keypad back
/// and the modal group is freed. A no-op when no dialog had one.
#[cfg(has_buttons)]
pub fn leave_modal_group() {
    unsafe {
        if MODAL_GROUP.is_null() {
            return;
        }
        let top = if ACTIVITY_GROUP_DEPTH > 0 {
            ACTIVITY_GROUPS[ACTIVITY_GROUP_DEPTH - 1]
        } else {
            core::ptr::null_mut()
        };
        lv_group_set_default(top);
        if !KEYPAD_INDEV.is_null() {
            lv_indev_set_group(KEYPAD_INDEV, top);
        }
        lv_group_delete(MODAL_GROUP);
        MODAL_GROUP = core::ptr::null_mut();
    }
}

/// Delete every remaining Activity focus group and reset the stack. Called from
/// the between-app-run reset path so a fresh app starts with a clean keypad.
#[cfg(has_buttons)]
pub fn reset_activity_groups() {
    unsafe {
        // Slice idiom (matching `&mut VIEW_KEY_MAP[..]` elsewhere in this file)
        // rather than an index range — keeps clippy's needless_range_loop quiet
        // without taking a `&mut` to the whole static (static_mut_refs).
        let had_groups = ACTIVITY_GROUP_DEPTH > 0;
        for slot in &mut ACTIVITY_GROUPS[..ACTIVITY_GROUP_DEPTH] {
            let g = *slot;
            *slot = core::ptr::null_mut();
            if !g.is_null() {
                lv_group_delete(g);
            }
        }
        ACTIVITY_GROUP_DEPTH = 0;
        if !MODAL_GROUP.is_null() {
            lv_group_delete(MODAL_GROUP);
            MODAL_GROUP = core::ptr::null_mut();
        }
        // Only touch LVGL when groups actually existed. This runs at app start
        // before `init_keypad`, so on the very first run LVGL isn't initialized
        // yet and KEYPAD_INDEV is null; a non-zero depth implies a prior run on
        // the persistent graphics singleton, where these pointers are live.
        if had_groups {
            lv_group_set_default(core::ptr::null_mut());
            if !KEYPAD_INDEV.is_null() {
                lv_indev_set_group(KEYPAD_INDEV, core::ptr::null_mut());
            }
        }
    }
}

// No-button boards have no keypad indev — the group machinery is inert, but the
// lifecycle still calls these so they exist as no-ops.
#[cfg(not(has_buttons))]
pub fn push_activity_group() {}
#[cfg(not(has_buttons))]
pub fn pop_activity_group() {}
#[cfg(not(has_buttons))]
pub fn reset_activity_groups() {}
#[cfg(not(has_buttons))]
pub fn enter_modal_group() -> *mut lv_group_t {
    core::ptr::null_mut()
}
#[cfg(not(has_buttons))]
pub fn leave_modal_group() {}

/// Put `raw` in `group` unless it is there already. LVGL's `lv_group_add_obj`
/// is not idempotent: it removes the object from its group (moving the focus
/// on if it held it) and appends it at the tail, so calling it on a member
/// reorders the focus ring — a `requestFocus()` on the second of four rows
/// used to make "down" from that row wrap to the first.
///
/// # Safety
/// `raw` and `group` must be live LVGL objects.
pub(super) unsafe fn ensure_in_group(group: *mut lv_group_t, raw: *mut lv_obj_t) {
    if lv_obj_get_group(raw) != group {
        lv_group_add_obj(group, raw);
        // Keypad focus has to be visible on a board whose only input is four
        // buttons. The theme's focus outline is drawn *outside* the object
        // and is clipped away for the common list shape — a full-width row
        // in a zero-padding column — so a focusable view also gets a border,
        // drawn inside its bounds. Both states: keypad navigation sets
        // FOCUS_KEY on top of FOCUSED.
        for state in [LV_STATE_FOCUSED, LV_STATE_FOCUS_KEY] {
            let sel = LV_PART_MAIN | state;
            lv_obj_set_style_border_width(raw, FOCUS_BORDER_PX, sel);
            lv_obj_set_style_border_color(raw, lv_color_hex(FOCUS_BORDER_RGB), sel);
            lv_obj_set_style_border_opa(raw, LV_OPA_COVER, sel);
        }
    }
    lv_obj_add_flag(raw, LV_OBJ_FLAG_SCROLL_ON_FOCUS);
}

/// The focus border a focusable view gets: light, so it reads on the dark
/// theme and on a tinted header band alike.
pub(super) const FOCUS_BORDER_RGB: u32 = 0x00E8_F0F0;
pub(super) const FOCUS_BORDER_PX: i32 = 2;

/// `View.setFocusable(boolean)` backing: add this view to — or remove it from —
/// the active Activity's keypad focus group. A focusable view also scrolls
/// into view when it takes focus, as a child of Android's ScrollView does.
/// No-op when the handle is null or no group is active (non-button boards,
/// or before the first Activity launches).
pub fn set_view_focusable(id: i32, on: bool) {
    let raw = super::super::handle_table::lookup(id);
    if raw.is_null() {
        return;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return;
        }
        if on {
            ensure_in_group(group, raw);
        } else {
            lv_group_remove_obj(raw);
        }
    }
}

/// `View.requestFocus()` backing: ensure the view is in the active group and
/// make it the focused widget. Returns whether it actually became focused —
/// false when the handle is null or there is no active group, matching
/// Android's "a view that can't take focus returns false".
pub fn request_view_focus(id: i32) -> bool {
    let raw = super::super::handle_table::lookup(id);
    if raw.is_null() {
        return false;
    }
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return false;
        }
        ensure_in_group(group, raw);
        lv_group_focus_obj(raw);
        lv_group_get_focused(group) == raw
    }
}

#[cfg(has_buttons)]
pub(super) fn init_button_pins() {
    for &(pin, _, _) in BUTTONS {
        hal::gpio::set_input(pin, hal::gpio::Pull::Up);
        hal::gpio::enable_edge_irq(pin, hal::gpio::EdgeTrigger::Both);
    }
    hal::gpio::init_gpio_irq();
}

#[cfg(not(has_buttons))]
pub(super) fn init_button_pins() {}
