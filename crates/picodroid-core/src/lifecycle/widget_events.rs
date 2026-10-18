// SPDX-License-Identifier: GPL-3.0-only
//! The per-frame widget-event fan-out: one `dispatch_*` per listener kind,
//! each draining its queue of widget handles and firing the matching
//! `DISPATCH_SITES` row on the Java object the handle table binds to the
//! handle (`handle_table::bound_java`). A handle whose widget was deleted
//! since the event was queued — a click that finished its Activity — or
//! whose Java object nothing bound resolves to nothing, and the event is
//! dropped.

use super::*;

// ── Click dispatch ───────────────────────────────────────────────────────────

/// Drain the click queue and invoke `View.fireClick()` on each matching view.
#[cfg(not(test))]
pub(super) fn dispatch_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::{events, handle_table};
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_click_queue() {
        // Suppress this click if onTouch or a long-press consumed the gesture
        // (set earlier this tick by dispatch_touch_events / dispatch_long_clicks).
        if events::take_click_suppressed(handle) {
            continue;
        }
        if let Some(obj_ref) = handle_table::bound_java(handle) {
            // fireClick() is a package-private method on View that invokes
            // onClickListener.onClick(this); any view subclass with a
            // listener attached resolves it via field inheritance.
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::BUTTON),
                dispatch_method(dispatch_sites::BUTTON),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain completed animation end-actions (withEndAction Runnables) and run
/// each through the Executors bytecode bridge — lambda proxies only resolve
/// via that static dispatch, never a direct Runnable.run invoke.
#[cfg(not(test))]
pub(super) fn dispatch_animation_end_actions(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some(runnable_ref) = widgets::drain_completed_end_action() {
        let _ = jvm.invoke_static_with_args(
            dispatch_class(dispatch_sites::EXECUTORS_DISPATCH),
            dispatch_method(dispatch_sites::EXECUTORS_DISPATCH),
            &[Value::ObjectRef(runnable_ref)],
            heap,
            handler,
        );
    }
}

/// Drain the long-press queue and invoke `View.fireLongClick()` on each
/// matching view (LV_EVENT_LONG_PRESSED → OnLongClickListener). The boolean
/// return is consumed by D4's click-suppression; here it is ignored.
#[cfg(not(test))]
pub(super) fn dispatch_long_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::{events, handle_table};
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_long_click_queue() {
        if let Some(obj_ref) = handle_table::bound_java(handle) {
            // A consumed long click (fireLongClick returns true) suppresses
            // the subsequent click — LVGL fires CLICKED after LONG_PRESSED,
            // so this matches Android. An unhandled long press still clicks.
            let ret = jvm.invoke_instance_with_args_returning(
                dispatch_class(dispatch_sites::VIEW_LONG_CLICK),
                dispatch_method(dispatch_sites::VIEW_LONG_CLICK),
                obj_ref,
                &[],
                heap,
                handler,
            );
            if matches!(ret, Ok(Some(pico_jvm::types::Value::Int(v))) if v != 0) {
                events::set_click_suppressed(handle);
            }
        }
    }
}

// ── Checked-change dispatch ──────────────────────────────────────────────

/// Drain the checked-change queue and invoke `fireCheckedChanged()` on each
/// matching ToggleButton.
#[cfg(not(test))]
pub(super) fn dispatch_checked_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_checked_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::TOGGLE_BUTTON),
                dispatch_method(dispatch_sites::TOGGLE_BUTTON),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── Switch checked-change dispatch ──────────────────────────────────────────

/// Drain the switch checked-change queue and invoke `fireCheckedChanged()` on
/// each matching Switch.
#[cfg(not(test))]
pub(super) fn dispatch_switch_checked_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_sw_checked_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::SWITCH),
                dispatch_method(dispatch_sites::SWITCH),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── NumberPicker keypad-step dispatch ───────────────────────────────────────

/// Drain the NumberPicker step queue and invoke `fireStep(int direction)` on
/// each matching picker. Steps are queued by the keypad edit-mode filter
/// (events.rs) when PREV/NEXT are pressed while a picker is being edited; the
/// Java side owns clamping, label refresh, and the OnValueChangeListener.
#[cfg(not(test))]
pub(super) fn dispatch_number_picker_steps(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((handle, direction)) = widgets::drain_np_step_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::NUMBER_PICKER_STEP),
                dispatch_method(dispatch_sites::NUMBER_PICKER_STEP),
                obj_ref,
                &[Value::Int(direction)],
                heap,
                handler,
            );
        }
    }
}

// ── CheckBox checked-change dispatch ────────────────────────────────────────

/// Drain the checkbox checked-change queue and invoke `fireCheckedChanged()` on
/// each matching CheckBox.
#[cfg(not(test))]
pub(super) fn dispatch_checkbox_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_cb_checked_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::CHECKBOX),
                dispatch_method(dispatch_sites::CHECKBOX),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain the checkbox checked-change queue and invoke `fireCheckedChanged()` on
/// each matching RadioButton.
#[cfg(not(test))]
pub(super) fn dispatch_radio_button_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_rb_checked_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::RADIO_BUTTON),
                dispatch_method(dispatch_sites::RADIO_BUTTON),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── Spinner item-selected dispatch ──────────────────────────────────────────

/// Drain the spinner value-changed queue and invoke `fireItemSelected()` on
/// each matching Spinner.
#[cfg(not(test))]
pub(super) fn dispatch_spinner_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_spinner_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::SPINNER),
                dispatch_method(dispatch_sites::SPINNER),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── AlertDialog button-click dispatch ──────────────────────────────────────

/// Drain the AlertDialog button-click queue and invoke `fireButtonClick(int)`
/// on each matching dialog Java object. The `which` value (0=positive,
/// 1=negative) is passed straight through; `AlertDialog.fireButtonClick`
/// routes to the correct Runnable on the Java side and dismisses the dialog.
#[cfg(not(test))]
pub(super) fn dispatch_alert_dialog_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((dialog, which)) = widgets::drain_dialog_click_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(dialog) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::ALERT_DIALOG),
                dispatch_method(dispatch_sites::ALERT_DIALOG),
                obj_ref,
                &[Value::Int(which)],
                heap,
                handler,
            );
        }
    }
}

/// Drain list-dialog item clicks and invoke `fireItemClick(position, checked)`
/// on each matching AlertDialog (plain items dismiss in Java; choice lists do
/// not). The dialog's Java object is held by `AlertDialog`'s showing list.
#[cfg(not(test))]
pub(super) fn dispatch_alert_dialog_item_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((dialog, position, checked)) = widgets::drain_dialog_item_click_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(dialog) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::ALERT_DIALOG_ITEM),
                dispatch_method(dispatch_sites::ALERT_DIALOG_ITEM),
                obj_ref,
                &[Value::Int(position), Value::Int(checked as i32)],
                heap,
                handler,
            );
        }
    }
}

/// Drain the dialogs the framework dismissed itself — BACK, in
/// `input::route_key`; an Activity's teardown or covering — and invoke
/// `fireDismiss()` on each, which runs the Java `OnDismissListener`, turns
/// `isShowing()` false and lets the Java side's showing list go of it. The
/// object ref is rooted by the queue until here (`visit_dialog_obj_roots`).
#[cfg(not(test))]
pub(super) fn dispatch_alert_dialog_dismissals(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(obj_ref) = widgets::drain_dialog_dismiss_queue() {
        let _ = jvm.invoke_instance(
            dispatch_class(dispatch_sites::ALERT_DIALOG_DISMISS),
            dispatch_method(dispatch_sites::ALERT_DIALOG_DISMISS),
            obj_ref,
            heap,
            handler,
        );
    }
}

/// Drain the Snackbar action-click queue and invoke `fireActionClick()` on
/// each matching Snackbar. The Java side runs the registered Runnable then
/// dismisses the snackbar.
#[cfg(not(test))]
pub(super) fn dispatch_snackbar_action_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_snackbar_click_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::SNACKBAR),
                dispatch_method(dispatch_sites::SNACKBAR),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain the snackbars whose timeout the frame tick dismissed and invoke
/// `fireDismiss()` on each, so the Java side's showing list lets go of a
/// bar that is already gone. The object ref is rooted by the queue until
/// here (`visit_snackbar_dismiss_roots`).
#[cfg(not(test))]
pub(super) fn dispatch_snackbar_dismissals(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(obj_ref) = widgets::drain_snackbar_dismiss_queue() {
        let _ = jvm.invoke_instance(
            dispatch_class(dispatch_sites::SNACKBAR_DISMISS),
            dispatch_method(dispatch_sites::SNACKBAR_DISMISS),
            obj_ref,
            heap,
            handler,
        );
    }
}

/// Drain DatePicker selection events and invoke `fireDateChanged()`.
#[cfg(not(test))]
pub(super) fn dispatch_date_picker_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_date_picker_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::DATE_PICKER),
                dispatch_method(dispatch_sites::DATE_PICKER),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain SwipeRefreshLayout pull-down events and invoke `fireRefresh()`.
#[cfg(not(test))]
pub(super) fn dispatch_swipe_refresh(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_refresh_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::SWIPE_REFRESH),
                dispatch_method(dispatch_sites::SWIPE_REFRESH),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain swipe-gesture events and invoke `View.fireSwipe(int direction)`
/// on the registered listener for each event. The direction is the same
/// `lv_dir_t` bitmask LVGL produced (LEFT=1, RIGHT=2, TOP=4, BOTTOM=8).
#[cfg(not(test))]
pub(super) fn dispatch_swipe_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::{events, handle_table};
    use pico_jvm::types::Value;

    while let Some(rec) = events::drain_swipe_event() {
        if let Some(obj_ref) = handle_table::bound_java(rec.view) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::VIEW_SWIPE),
                dispatch_method(dispatch_sites::VIEW_SWIPE),
                obj_ref,
                &[Value::Int(rec.direction)],
                heap,
                handler,
            );
        }
    }
}

/// Drain ListView item-click events and invoke `ListView.fireItemClick(int
/// position)` on the Java `ListView` bound to each event's list. A row
/// activated by ENTER (keypad) or a touch tap fires `LV_EVENT_CLICKED`; the
/// Java side resolves the row View / item id from the adapter and calls the
/// `AdapterView.OnItemClickListener`.
#[cfg(not(test))]
pub(super) fn dispatch_list_view_item_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((list, position)) = widgets::drain_item_click_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(list) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::LIST_VIEW_ITEM_CLICK),
                dispatch_method(dispatch_sites::LIST_VIEW_ITEM_CLICK),
                obj_ref,
                &[Value::Int(position)],
                heap,
                handler,
            );
        }
    }
}

/// Drain view focus-change events and invoke `View.fireFocusChange(boolean
/// hasFocus)` on the registered listener for each event (boolean passed as
/// 0/1 — the JVM stack representation of `Z`).
#[cfg(not(test))]
pub(super) fn dispatch_view_focus_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::lvgl::{events, handle_table};
    use pico_jvm::types::Value;

    while let Some(rec) = events::drain_focus_change_event() {
        if let Some(obj_ref) = handle_table::bound_java(rec.view) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::VIEW_FOCUS_CHANGE),
                dispatch_method(dispatch_sites::VIEW_FOCUS_CHANGE),
                obj_ref,
                &[Value::Int(if rec.has_focus { 1 } else { 0 })],
                heap,
                handler,
            );
        }
    }
}

/// Drain TimePicker selection events and invoke `fireTimeChanged()`.
#[cfg(not(test))]
pub(super) fn dispatch_time_picker_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_time_picker_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::TIME_PICKER),
                dispatch_method(dispatch_sites::TIME_PICKER),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── SeekBar value-changed dispatch ──────────────────────────────────────────

/// Drain the seek bar value-changed queue and invoke `fireProgressChanged()` on
/// each matching SeekBar.
#[cfg(not(test))]
pub(super) fn dispatch_seek_bar_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_seek_change_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::SEEK_BAR),
                dispatch_method(dispatch_sites::SEEK_BAR),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain the textarea content-change queue and invoke `fireTextChanged()` on
/// each matching EditText — TextWatcher.afterTextChanged. The queue carries
/// only handles; the Java side re-reads the final text once per dispatch.
#[cfg(not(test))]
pub(super) fn dispatch_edit_text_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;

    while let Some(handle) = widgets::drain_text_changed_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::EDIT_TEXT_TEXT_CHANGED),
                dispatch_method(dispatch_sites::EDIT_TEXT_TEXT_CHANGED),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

/// Drain the seek bar press/release queue and invoke `fireTrackingTouch(boolean)`
/// on each matching SeekBar — onStartTrackingTouch / onStopTrackingTouch.
#[cfg(not(test))]
pub(super) fn dispatch_seek_bar_tracking(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((handle, started)) = widgets::drain_seek_tracking_queue() {
        if let Some(obj_ref) = crate::graphics::lvgl::handle_table::bound_java(handle) {
            let _ = jvm.invoke_instance_with_args(
                dispatch_class(dispatch_sites::SEEK_BAR_TRACKING),
                dispatch_method(dispatch_sites::SEEK_BAR_TRACKING),
                obj_ref,
                &[Value::Int(started as i32)],
                heap,
                handler,
            );
        }
    }
}
