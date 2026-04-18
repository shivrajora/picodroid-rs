//! Application and Activity lifecycle management.
//!
//! This module owns the Android-like lifecycle callbacks (onCreate, event loop)
//! for both Application and Activity entry points.  The JVM setup, class
//! loading, and shared heap management remain in `app.rs`.

#[cfg(not(test))]
use pico_jvm::types::JvmError;
#[cfg(not(test))]
use pico_jvm::{Jvm, SharedJvmHeap};

// ── Application lifecycle ────────────────────────────────────────────────────

/// Run an Application-based app: allocate the Application object, call
/// `onCreate()`, then launch a pending Activity if `startActivity()` was called.
#[cfg(not(test))]
pub(crate) fn run_application(
    jvm: &mut Jvm,
    application_class: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    let obj_ref = heap
        .objects
        .alloc(application_class)
        .expect("OOM allocating Application");

    match jvm.invoke_instance(application_class, "onCreate", obj_ref, heap, handler) {
        Ok(()) => {}
        Err(JvmError::Interrupted) => return,
        Err(e) => {
            log_error!("Application.onCreate error: {}", e);
            return;
        }
    }

    // If onCreate() called startActivity(), launch the Activity lifecycle.
    if let Some((activity_ref, activity_class)) = handler.take_pending_activity() {
        run_activity(jvm, activity_class, activity_ref, heap, handler);
    }
}

// ── Activity lifecycle ───────────────────────────────────────────────────────

/// Run an Activity-based app: call `onCreate()` on the pre-allocated Activity,
/// then enter the framework event loop (tick LVGL + dispatch click events).
///
/// Works on both real hardware and the host simulator (minifb window).
#[cfg(not(test))]
pub(crate) fn run_activity(
    jvm: &mut Jvm,
    activity_class: &'static str,
    obj_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::engine;
    use pico_jvm::NativeMethodHandler;

    // Call the subclass's onCreate() -- this builds the UI tree and calls
    // setContentView().  Virtual dispatch picks up the override automatically.
    match jvm.invoke_instance(activity_class, "onCreate", obj_ref, heap, handler) {
        Ok(()) => {}
        Err(JvmError::Interrupted) => return,
        Err(e) => {
            log_error!("Activity.onCreate error: {}", e);
            return;
        }
    }

    // Framework event loop -- tick LVGL and dispatch click callbacks.
    let mut pacer = crate::hal::system_clock::FramePacer::new();
    loop {
        // Check before the potentially-blocking LVGL render.
        if handler.interrupted() {
            break;
        }

        engine::tick(16);
        crate::system::picodroid::graphics::fps_overlay::update();
        dispatch_clicks(jvm, heap, handler);
        dispatch_checked_changes(jvm, heap, handler);
        dispatch_switch_checked_changes(jvm, heap, handler);
        dispatch_seek_bar_changes(jvm, heap, handler);
        dispatch_checkbox_changes(jvm, heap, handler);
        dispatch_spinner_changes(jvm, heap, handler);
        dispatch_key_events(jvm, heap, handler);

        crate::system::picodroid::hardware::sensors::drain_sensor_events(jvm, heap, handler);

        crate::hal::display::update_window();
        if !crate::hal::display::is_window_open() {
            break;
        }

        pacer.pace(16);
    }
}

// ── Click dispatch ───────────────────────────────────────────────────────────

/// Drain the click queue and invoke `fireClick()` on each matching Button.
#[cfg(not(test))]
fn dispatch_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_click_queue() {
        if let Some(obj_ref) = widgets::lookup_button_obj(handle) {
            // fireClick() is a Java method on Button that calls onClickListener.run().
            let _ = jvm.invoke_instance(
                "picodroid/widget/Button",
                "fireClick",
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── Checked-change dispatch ──────────────────────────────────────────────

/// Drain the checked-change queue and invoke `fireCheckedChanged()` on each
/// matching ToggleButton.
#[cfg(not(test))]
fn dispatch_checked_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_checked_change_queue() {
        if let Some(obj_ref) = widgets::lookup_checked_change_obj(handle) {
            let _ = jvm.invoke_instance(
                "picodroid/widget/ToggleButton",
                "fireCheckedChanged",
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
fn dispatch_switch_checked_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_sw_checked_change_queue() {
        if let Some(obj_ref) = widgets::lookup_sw_checked_change_obj(handle) {
            let _ = jvm.invoke_instance(
                "picodroid/widget/Switch",
                "fireCheckedChanged",
                obj_ref,
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
fn dispatch_checkbox_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_cb_checked_change_queue() {
        if let Some(obj_ref) = widgets::lookup_cb_checked_change_obj(handle) {
            let _ = jvm.invoke_instance(
                "picodroid/widget/CheckBox",
                "fireCheckedChanged",
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
fn dispatch_spinner_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_spinner_change_queue() {
        if let Some(obj_ref) = widgets::lookup_spinner_obj(handle) {
            let _ = jvm.invoke_instance(
                "picodroid/widget/Spinner",
                "fireItemSelected",
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── Hardware key-event dispatch ─────────────────────────────────────────────

/// Drain the hardware key-event queue and invoke `View.fireKey()` on the
/// Java View corresponding to LVGL's currently focused widget. If no widget
/// is focused or the focused widget has no registered OnKeyListener, the
/// event is silently dropped — LVGL has already consumed it for focus
/// navigation via the same `keypad_read_cb`.
///
/// Note: `hal::sim::gpio::drain_gpio_event` always returns `None` in sim
/// builds, so this dispatcher never fires events on the host. End-to-end
/// verification requires hardware.
#[cfg(not(test))]
fn dispatch_key_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::engine;
    use pico_jvm::types::Value;

    while let Some(raw) = engine::drain_key_event() {
        let view_ref = match engine::focused_view_obj() {
            Some(r) => r,
            None => continue,
        };
        let keycode = match engine::pin_to_keycode(raw.pin) {
            Some(k) => k,
            None => continue,
        };
        let action = if raw.rising { 1 } else { 0 }; // ACTION_UP : ACTION_DOWN

        let event_obj = match heap.objects.alloc("picodroid/view/KeyEvent") {
            Some(o) => o,
            None => continue,
        };
        if heap
            .objects
            .set_field(
                event_obj,
                crate::system::picodroid::graphics::fields::key_event::ACTION,
                Value::Int(action),
            )
            .is_none()
        {
            continue;
        }
        if heap
            .objects
            .set_field(
                event_obj,
                crate::system::picodroid::graphics::fields::key_event::KEY_CODE,
                Value::Int(keycode),
            )
            .is_none()
        {
            continue;
        }

        let _ = jvm.invoke_instance_with_args(
            "picodroid/view/View",
            "fireKey",
            view_ref,
            &[Value::ObjectRef(event_obj)],
            heap,
            handler,
        );
    }
}

// ── SeekBar value-changed dispatch ──────────────────────────────────────────

/// Drain the seek bar value-changed queue and invoke `fireProgressChanged()` on
/// each matching SeekBar.
#[cfg(not(test))]
fn dispatch_seek_bar_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_seek_change_queue() {
        if let Some(obj_ref) = widgets::lookup_seek_bar_obj(handle) {
            let _ = jvm.invoke_instance(
                "picodroid/widget/SeekBar",
                "fireProgressChanged",
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── Logging helper ───────────────────────────────────────────────────────────

/// Unified error logging macro: uses `defmt::error!` on hardware, `eprintln!`
/// in sim mode.
macro_rules! log_error {
    ($fmt:literal, $val:expr) => {
        #[cfg(feature = "sim")]
        eprintln!(concat!("[sim] ", $fmt), $val);
        #[cfg(not(feature = "sim"))]
        defmt::error!($fmt, defmt::Display2Format(&$val));
    };
}
use log_error;
