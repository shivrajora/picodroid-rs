// SPDX-License-Identifier: GPL-3.0-only
//! Keypad focus groups: one LVGL group per Activity on the back stack, a
//! modal group above them, and the focus / focusable helpers views call.

use super::*;
#[cfg(has_buttons)]
use crate::util::local::Core0;
#[cfg(has_buttons)]
use core::cell::Cell;

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
pub(super) struct Groups {
    /// The keypad input device, created by `init_keypad`; `None` before.
    pub(super) keypad_indev: Cell<Option<Indev>>,
    /// One group per Activity on the stack: `activity[..depth]`, bottom first.
    /// Kept as `Group`s across natives because only this module deletes a
    /// group, and it takes the group out of here first.
    activity: [Cell<Option<Group>>; MAX_ACTIVITY_GROUPS],
    depth: Cell<usize>,
    /// The group a modal dialog's buttons live in while any dialog is shown
    /// ([`enter_modal_group`]); `None` between dialogs.
    modal: Cell<Option<Group>>,
}

// SAFETY: pushed and popped by the lifecycle and dialog natives, read by
// the keypad indev's read callback, all on core-0 JVM tasks.
#[cfg(has_buttons)]
pub(super) static GROUPS: Core0<Groups> = unsafe {
    Core0::new(Groups {
        keypad_indev: Cell::new(None),
        activity: [const { Cell::new(None) }; MAX_ACTIVITY_GROUPS],
        depth: Cell::new(0),
        modal: Cell::new(None),
    })
};

/// The top Activity's group, or `None` with no Activity on the stack.
#[cfg(has_buttons)]
fn top_activity_group() -> Option<Group> {
    match GROUPS.depth.get() {
        0 => None,
        depth => GROUPS.activity[depth - 1].get(),
    }
}

/// Make `group` LVGL's default and the keypad's.
#[cfg(has_buttons)]
fn activate(group: Option<Group>) {
    lv_group_set_default(group);
    if let Some(indev) = GROUPS.keypad_indev.get() {
        lv_indev_set_group(indev, group);
        // The key that moved the keypad to this group is still down; its
        // release must not land on whatever this group focuses (a dialog
        // picked on SELECT's press used to be reopened by SELECT's release
        // clicking the row beneath it).
        lv_indev_wait_release(indev);
    }
}

/// Create a fresh focus group for a newly-launched Activity and make it the
/// active group (LVGL default + keypad indev). Called from the lifecycle
/// bootstrap/push paths *before* the Activity's `onCreate`, so its group-def
/// widgets auto-join this group.
#[cfg(has_buttons)]
pub fn push_activity_group() {
    let depth = GROUPS.depth.get();
    if depth >= MAX_ACTIVITY_GROUPS {
        return; // unreachable: the Activity stack caps depth first
    }
    // SAFETY: Activities run only inside `lifecycle::run_activity`, which
    // brings LVGL up (`display::get_instance`) before the first push.
    let group = unsafe { lv_group_create() };
    activate(group);
    GROUPS.activity[depth].set(group);
    GROUPS.depth.set(depth + 1);
}

/// Tear down the top Activity's focus group and reactivate the parent's (or
/// none if the stack is now empty). Called from the lifecycle pop path *after*
/// the popped Activity's view tree is deleted. The parent group is reattached
/// to the indev before the child group is freed, so the indev never references
/// a deleted group.
#[cfg(has_buttons)]
pub fn pop_activity_group() {
    let depth = GROUPS.depth.get();
    if depth == 0 {
        return;
    }
    GROUPS.depth.set(depth - 1);
    let group = GROUPS.activity[depth - 1].replace(None);
    // A dialog still up on the parent keeps the keypad: its buttons are
    // in the modal group, not the parent's.
    activate(GROUPS.modal.get().or_else(top_activity_group));
    if let Some(group) = group {
        // SAFETY: the group is out of the stack, and neither LVGL's default
        // nor the keypad points at it any more (`activate` above).
        unsafe { lv_group_delete(group) };
    }
}

/// The keypad group a modal dialog's buttons go in while any dialog is
/// shown. Created by the first `show`, it replaces the Activity's group as
/// LVGL's default and the keypad's, so NEXT and PREV cycle the dialog's own
/// buttons and cannot walk onto the rows behind the scrim — "down, down,
/// select" from an uninstall dialog used to confirm a *different* app. `None`
/// when no Activity group is active, as `lv_group_get_default` was.
#[cfg(has_buttons)]
pub fn enter_modal_group() -> Option<Group> {
    if GROUPS.depth.get() == 0 {
        return None;
    }
    if GROUPS.modal.get().is_none() {
        // SAFETY: an Activity group exists (checked above), so LVGL is up
        // (`push_activity_group`).
        GROUPS.modal.set(unsafe { lv_group_create() });
    }
    let modal = GROUPS.modal.get();
    activate(modal);
    modal
}

/// The last dialog is gone: the top Activity's group takes the keypad back
/// and the modal group is freed. A no-op when no dialog had one.
#[cfg(has_buttons)]
pub fn leave_modal_group() {
    let Some(modal) = GROUPS.modal.get() else {
        return;
    };
    activate(top_activity_group());
    GROUPS.modal.set(None);
    // SAFETY: the modal group is out of `GROUPS`, and neither LVGL's default
    // nor the keypad points at it any more (`activate` above).
    unsafe { lv_group_delete(modal) };
}

/// Delete every remaining Activity focus group and reset the stack. Called from
/// the between-app-run reset path so a fresh app starts with a clean keypad.
#[cfg(has_buttons)]
pub fn reset_activity_groups() {
    let depth = GROUPS.depth.get();
    let had_groups = depth > 0;
    // Only touch LVGL when groups actually existed. This runs at app start
    // before `init_keypad`, so on the very first run LVGL isn't initialized
    // yet and the keypad indev is `None`; a non-zero depth implies a prior
    // run on the persistent graphics singleton, where these groups are live.
    if had_groups {
        activate(None);
    }
    for slot in &GROUPS.activity[..depth] {
        if let Some(g) = slot.replace(None) {
            // SAFETY: out of the stack, and nothing points at it any more
            // (`activate(None)` above).
            unsafe { lv_group_delete(g) };
        }
    }
    GROUPS.depth.set(0);
    if let Some(modal) = GROUPS.modal.replace(None) {
        // SAFETY: as for the Activity groups.
        unsafe { lv_group_delete(modal) };
    }
}

/// Whether a dialog holds the keypad: its list and buttons are what the keys
/// walk, so the keypad reads a focused button matrix as the dialog's list.
#[cfg(has_buttons)]
pub fn in_modal_group() -> bool {
    GROUPS.modal.get().is_some()
}

// No-button boards have no keypad indev — the group machinery is inert, but the
// lifecycle still calls these so they exist as no-ops.
#[cfg(not(has_buttons))]
pub fn push_activity_group() {}
#[cfg(not(has_buttons))]
pub fn in_modal_group() -> bool {
    false
}
#[cfg(not(has_buttons))]
pub fn pop_activity_group() {}
#[cfg(not(has_buttons))]
pub fn reset_activity_groups() {}
#[cfg(not(has_buttons))]
pub fn enter_modal_group() -> Option<Group> {
    None
}
#[cfg(not(has_buttons))]
pub fn leave_modal_group() {}

/// Put `raw` in `group` unless it is there already. LVGL's `lv_group_add_obj`
/// is not idempotent: it removes the object from its group (moving the focus
/// on if it held it) and appends it at the tail, so calling it on a member
/// reorders the focus ring — a `requestFocus()` on the second of four rows
/// used to make "down" from that row wrap to the first.
pub(super) fn ensure_in_group(group: Group, raw: Obj) {
    if lv_obj_get_group(raw) != Some(group) {
        lv_group_add_obj(group, raw);
    }
    // Keypad focus has to be visible on a board whose only input is four
    // buttons. The theme's focus outline is drawn *outside* the object and
    // is clipped away for the common list shape — a full-width row in a
    // zero-padding column — so a focusable view also gets a border, drawn
    // inside its bounds. Both states: keypad navigation sets FOCUS_KEY on
    // top of FOCUSED. Set whether or not the view was in the group already:
    // LVGL puts a button, checkbox, switch, slider, dropdown, roller or
    // textarea in the default group as it is created, so for every stock
    // widget the add above is skipped — and the border with it, when it
    // sat inside that branch: a focused Button was a flat button (QA F5).
    // Setting a local style property again is idempotent. Only where a key
    // moves the focus: a board whose buttons are BACK and HOME alone has a
    // keypad indev and a default group, so a focusable view lands in a
    // ring here too, but nothing walks it, and a ring drawn on the first
    // row of a touch-only screen reads as a selection (the launcher on the
    // touch kit, QA F6). `View.isInTouchMode()` is the same fact.
    if crate::board_cfg::input::HAS_NAV_KEYS {
        // A checkbox (a RadioButton is one too) sizes itself from its text
        // plus its own border, so a border on it grew it by the border's
        // width all round while it was focused and its siblings shifted
        // (QA round 2, R3). Its box, a fixed size, carries the border
        // instead.
        let part = if super::super::is_class(raw, &raw const lv_checkbox_class) {
            LV_PART_INDICATOR
        } else {
            LV_PART_MAIN
        };
        for state in [LV_STATE_FOCUSED, LV_STATE_FOCUS_KEY] {
            let sel = part | state;
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
    let Some(raw) = super::super::handle_table::get(id) else {
        return;
    };
    let Some(group) = lv_group_get_default() else {
        return;
    };
    if on {
        ensure_in_group(group, raw);
    } else {
        lv_group_remove_obj(raw);
    }
}

/// `View.requestFocus()` backing: ensure the view is in the active group and
/// make it the focused widget. Returns whether it actually became focused —
/// false when the handle is null or there is no active group, matching
/// Android's "a view that can't take focus returns false".
pub fn request_view_focus(id: i32) -> bool {
    let Some(raw) = super::super::handle_table::get(id) else {
        return false;
    };
    let Some(group) = lv_group_get_default() else {
        return false;
    };
    ensure_in_group(group, raw);
    lv_group_focus_obj(raw);
    lv_group_get_focused(group) == Some(raw)
}

/// The system keyboard is showing: put it in the active focus group and
/// focus it, so the keypad drives its key grid (`keyboard::keypad_remap`),
/// with `first_key` selected — the layout's first character, so the walk
/// starts on a letter rather than on the mode switch at key 0. The
/// FOCUS_KEY state is what the button matrix draws its selected key with;
/// the group sets it only when a keypad indev is mid-read, which this
/// (a quiet indev pass) is not. `kb` must be the system keyboard.
#[cfg(has_buttons)]
pub fn focus_system_keyboard(kb: Obj, first_key: u32) {
    let Some(group) = lv_group_get_default() else {
        return;
    };
    if lv_obj_get_group(kb) != Some(group) {
        lv_group_add_obj(group, kb);
    }
    lv_group_focus_obj(kb);
    lv_obj_add_state(kb, LV_STATE_FOCUS_KEY);
    // SAFETY: `kb` is the system keyboard (`keyboard::take_pending_focus`,
    // the only caller), an `lv_keyboard` and so an `lv_buttonmatrix`.
    unsafe { lv_buttonmatrix_set_selected_button(kb, first_key) };
    lv_obj_invalidate(kb);
}

/// The system keyboard is hidden: out of the group (it outlives every
/// Activity, so it must not stay in one Activity's ring), and the field it
/// typed into takes the focus back.
#[cfg(has_buttons)]
pub fn unfocus_system_keyboard(kb: Obj, ta: Option<Obj>) {
    lv_obj_remove_state(kb, LV_STATE_FOCUS_KEY);
    if lv_obj_get_group(kb).is_some() {
        lv_group_remove_obj(kb);
    }
    let group = lv_group_get_default();
    if let Some(ta) = ta {
        if group.is_some() && lv_obj_get_group(ta) == group {
            lv_group_focus_obj(ta);
        }
    }
}

#[cfg(not(has_buttons))]
pub fn focus_system_keyboard(_kb: Obj, _first_key: u32) {}
#[cfg(not(has_buttons))]
pub fn unfocus_system_keyboard(_kb: Obj, _ta: Option<Obj>) {}

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
