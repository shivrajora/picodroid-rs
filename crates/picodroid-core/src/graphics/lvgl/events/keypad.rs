// SPDX-License-Identifier: GPL-3.0-only
//! The Java-visible key event queue, keypad edit mode (NumberPicker
//! stepping) and `keypad_read_cb`, the indev callback that feeds both the
//! LVGL keypad and that queue from one GPIO event.

use super::*;

// ── Java-visible key event queue (parallel to LVGL's internal queue) ────────

// Matches the deepened HAL GPIO edge queue: one indev read pass can forward
// a whole stall's worth of batched PREV/NEXT edges here before the Java
// dispatch drains them.
pub(super) const KEY_EVENT_QUEUE_SIZE: usize = 64;
pub(super) static mut KEY_EVENT_QUEUE: [KeyEventRaw; KEY_EVENT_QUEUE_SIZE] = [KeyEventRaw {
    pin: 0,
    rising: false,
};
    KEY_EVENT_QUEUE_SIZE];
pub(super) static mut KEY_EVENT_QUEUE_HEAD: usize = 0;
pub(super) static mut KEY_EVENT_QUEUE_TAIL: usize = 0;

/// Press-state filter — drops the phantom rising-edge IRQs that fire at boot
/// when `enable_edge_irq` arms the GPIO peripheral on pins that were in an
/// indeterminate state during init (observed on Pico Enviro+ Pack: every
/// button GP12-15 fires a phantom release within the first 50 ms, which
/// dispatched BACK and finished the activity before sensors could deliver a
/// second event). Pure logic lives in `super::super::key_filter`.
#[cfg(has_buttons)]
pub(super) static mut KEY_PRESS_FILTER: super::super::key_filter::KeyPressFilter =
    super::super::key_filter::KeyPressFilter::new();

/// Contact debounce — collapses each mechanical chatter burst to its first
/// edge, comparing ISR-captured `GpioEvent::t_us` values. Runs in
/// `keypad_read_cb` directly after the drain, ahead of the edit-mode filter
/// and both fan-out paths (LVGL indev + Java queue), so every consumer sees
/// the same debounced stream. Window rationale and tuning data live in
/// `super::super::key_debounce`. Edges discarded by the lifecycle transition flush
/// bypass this (they are dropped wholesale anyway; a chatter tail surviving
/// a flush is bounded by the phantom-release filter).
#[cfg(has_buttons)]
pub(super) static mut KEY_DEBOUNCE: super::super::key_debounce::KeyDebounce =
    super::super::key_debounce::KeyDebounce::new();

#[cfg(has_buttons)]
pub(super) fn push_key_event_raw(pin: u8, rising: bool) {
    unsafe {
        // `&raw mut` then deref: forming `&mut KEY_PRESS_FILTER` directly trips
        // the `static_mut_refs` lint (Rust 2024 compat). See handle_table.rs /
        // socket_table.rs for the same idiom.
        let filter = &raw mut KEY_PRESS_FILTER;
        if !(*filter).observe(pin, rising) {
            return;
        }
        let head = KEY_EVENT_QUEUE_HEAD;
        let next = (head + 1) % KEY_EVENT_QUEUE_SIZE;
        if next != KEY_EVENT_QUEUE_TAIL {
            KEY_EVENT_QUEUE[head] = KeyEventRaw { pin, rising };
            KEY_EVENT_QUEUE_HEAD = next;
        } else {
            crate::pd_warn!("key: java queue full, dropped pin {} edge", pin);
        }
    }
}

// ── Keypad edit mode (NumberPicker stepping) ────────────────────────────────
//
// One EditMode instance filters every key edge *before* it fans out to the
// LVGL indev and the Java queue below — the single interception point that
// keeps both paths consistent (see edit_mode.rs for the protocol).

#[cfg(has_buttons)]
pub(super) static mut EDIT_MODE: super::super::edit_mode::EditMode =
    super::super::edit_mode::EditMode::new();

/// Mirror an edit-mode transition onto the widget: LV_STATE_EDITED drives the
/// theme-matching secondary outline. The carried pointer is the currently
/// focused widget, so it is live.
#[cfg(has_buttons)]
pub(super) fn apply_edit_transition(t: super::super::edit_mode::Transition) {
    use super::super::edit_mode::Transition;
    match t {
        Transition::None => {}
        Transition::Entered(obj) => unsafe {
            lv_obj_add_state(obj as *mut lv_obj_t, LV_STATE_EDITED);
        },
        Transition::Exited(obj) => unsafe {
            lv_obj_remove_state(obj as *mut lv_obj_t, LV_STATE_EDITED);
        },
    }
}

/// Called from the NumberPicker DEFOCUSED/DELETE trampolines: abandon keypad
/// edit mode if `raw_obj` is the widget being edited. The trampoline clears
/// LV_STATE_EDITED itself (the object is live there); this only drops the
/// filter state so PREV/NEXT go back to navigating.
#[cfg(has_buttons)]
pub fn notify_picker_gone(raw_obj: usize) {
    unsafe {
        let em = &raw mut EDIT_MODE;
        (*em).notify_gone(raw_obj);
    }
}

#[cfg(not(has_buttons))]
pub fn notify_picker_gone(_raw_obj: usize) {}

/// Clear edit-mode state between app runs. Called from the `app.rs` reset
/// block alongside the other widget-state resets.
#[cfg(has_buttons)]
pub fn reset_edit_mode() {
    unsafe {
        let em = &raw mut EDIT_MODE;
        *em = super::super::edit_mode::EditMode::new();
    }
}

#[cfg(not(has_buttons))]
pub fn reset_edit_mode() {}

/// The active group's focused widget as a raw pointer (0 if none), plus
/// whether it is a registered NumberPicker — the edit-mode filter's inputs.
#[cfg(has_buttons)]
pub(super) fn focused_obj_for_edit_mode() -> (usize, bool) {
    unsafe {
        let group = lv_group_get_default();
        if group.is_null() {
            return (0, false);
        }
        let focused = lv_group_get_focused(group) as usize;
        (
            focused,
            super::super::widgets::number_picker::is_number_picker(focused),
        )
    }
}

#[cfg(has_buttons)]
pub(super) unsafe extern "C" fn keypad_read_cb(
    _indev: *mut lv_indev_t,
    data: *mut lv_indev_data_t,
) {
    let d = unsafe { &mut *data };
    // Drain until the first edge that survives the contact debounce; chatter
    // edges are consumed and discarded without ending the read pass.
    let debounced = loop {
        match hal::gpio::drain_gpio_event() {
            Some(ev) => {
                let accepted = unsafe {
                    let deb = &raw mut KEY_DEBOUNCE;
                    (*deb).accept(ev.pin, ev.t_us)
                };
                if accepted {
                    break Some(ev);
                }
            }
            None => break None,
        }
    };
    if let Some(event) = debounced {
        let key = BUTTONS
            .iter()
            .find(|&&(p, _, _)| p == event.pin)
            .map(|&(_, k, _)| k)
            // `lv_key = "NONE"` in board.toml emits 0, which is not an LVGL
            // key. Dropping it here is what makes a system key like HOME
            // Java-only: it reaches the app as a keycode without also stepping
            // the focus ring or activating the focused widget on the way.
            .filter(|&k| k != 0);

        // Run mapped keys through the edit-mode filter; unmapped pins keep
        // the historical behavior (Java queue only, nothing for the indev).
        let decision = match key {
            Some(k) => {
                let (focused, is_picker) = focused_obj_for_edit_mode();
                let (decision, transition) = unsafe {
                    let em = &raw mut EDIT_MODE;
                    (*em).filter(k, !event.rising, focused, is_picker)
                };
                apply_edit_transition(transition);
                decision
            }
            None => super::super::edit_mode::Decision {
                lvgl_key: None,
                forward_java: true,
                step: None,
            },
        };

        if decision.forward_java {
            push_key_event_raw(event.pin, event.rising);
        }
        if let Some((obj, direction)) = decision.step {
            super::super::widgets::number_picker::push_step(obj, direction);
        }
        if let Some(k) = decision.lvgl_key {
            d.key = k;
            d.state = if event.rising {
                LV_INDEV_STATE_RELEASED
            } else {
                LV_INDEV_STATE_PRESSED
            };
        }
        // An ENTER or ESC edge can activate a widget or trigger BACK, and the
        // resulting Activity push/pop (with its keypad-group swap) only runs
        // in the lifecycle drain *after* this read pass. Stop the pass at
        // such an edge so the remaining queued edges are read next tick,
        // against the screen the activation actually produced. Without this
        // barrier, input arriving faster than a screen transition is consumed
        // by the OLD screen's focus group (2026-07-23 stress-run PEM-1: a
        // whole Settings choreography eaten by the hub). PREV/NEXT edges are
        // screen-local and still batch freely.
        let activation = matches!(key, Some(LV_KEY_ENTER) | Some(LV_KEY_ESC));
        d.continue_reading = hal::gpio::has_pending_event() && !activation;
    } else {
        d.state = LV_INDEV_STATE_RELEASED;
        d.continue_reading = false;
    }
}
