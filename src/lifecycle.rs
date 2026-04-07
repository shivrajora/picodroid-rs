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
///
/// On real hardware the full Activity display lifecycle runs if one was
/// requested.  In sim mode the Activity launch is noted but skipped (no LVGL
/// display hardware).
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
        #[cfg(feature = "sim")]
        {
            let _ = activity_ref;
            eprintln!(
                "[sim] startActivity('{}') called; skipping display lifecycle (no display in sim)",
                activity_class
            );
        }
        #[cfg(not(feature = "sim"))]
        {
            run_activity(jvm, activity_class, activity_ref, heap, handler);
        }
    }
}

// ── Activity lifecycle ───────────────────────────────────────────────────────

/// Run an Activity-based app: call `onCreate()` on the pre-allocated Activity,
/// then enter the framework event loop (tick LVGL + dispatch click events).
///
/// In sim mode LVGL blocks on `lv_display_create` (no real display hardware),
/// so we skip the full lifecycle and just verify the Activity class loads.
#[cfg(all(not(test), feature = "sim"))]
pub(crate) fn run_activity(
    _jvm: &mut Jvm,
    activity_class: &'static str,
    obj_ref: u16,
    _heap: &mut SharedJvmHeap,
    _handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    eprintln!(
        "[sim] Activity '{}' loaded (obj_ref={}); skipping onCreate + event loop (no display in sim)",
        activity_class, obj_ref
    );
}

/// Run an Activity-based app on real hardware: call `onCreate()` on the
/// pre-allocated Activity, then enter the framework event loop.
#[cfg(not(any(test, feature = "sim")))]
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
        engine::tick(16);
        dispatch_clicks(jvm, heap, handler);
        dispatch_checked_changes(jvm, heap, handler);
        pacer.pace(16);

        if handler.interrupted() {
            break;
        }
    }
}

// ── Click dispatch ───────────────────────────────────────────────────────────

/// Drain the click queue and invoke `fireClick()` on each matching Button.
#[cfg(not(any(test, feature = "sim")))]
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
#[cfg(not(any(test, feature = "sim")))]
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

// ── Logging helper ───────────────────────────────────────────────────────────

/// Unified error logging macro: uses `defmt::error!` on hardware, `eprintln!`
/// in sim mode.
macro_rules! log_error {
    ($fmt:literal, $val:expr) => {
        #[cfg(feature = "sim")]
        eprintln!(concat!("[sim] ", $fmt), format_args!("{:?}", $val));
        #[cfg(not(feature = "sim"))]
        defmt::error!($fmt, defmt::Debug2Format(&$val));
    };
}
use log_error;
