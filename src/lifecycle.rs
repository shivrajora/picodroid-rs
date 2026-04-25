//! Application and Activity lifecycle management.
//!
//! This module owns the Android-like lifecycle callbacks (onCreate, event loop)
//! for both Application and Activity entry points.  The JVM setup, class
//! loading, and shared heap management remain in `app.rs`.

#[cfg(not(test))]
use pico_jvm::types::JvmError;
#[cfg(not(test))]
use pico_jvm::{Jvm, SharedJvmHeap};

#[cfg(not(test))]
use crate::dispatch_sites::{self, DISPATCH_SITES};

/// Look up the shrunk framework class name for the dispatch site at `idx`
/// in [`DISPATCH_SITES`]. Zero-cost identity when no shrink map is active.
#[cfg(not(test))]
#[inline]
fn dispatch_class(idx: usize) -> &'static str {
    crate::shrink_names::shrink_class(DISPATCH_SITES[idx].0)
}

/// The `fire*` method name for the dispatch site at `idx`.
#[cfg(not(test))]
#[inline]
fn dispatch_method(idx: usize) -> &'static str {
    DISPATCH_SITES[idx].1
}

/// Idle period after which the display is put to sleep. Reset by any GPIO
/// button event. Gated on `has_buttons` because the wake path blocks on a
/// button IRQ — touch-only boards would never wake.
#[cfg(all(not(feature = "sim"), has_buttons))]
const IDLE_TIMEOUT_MS: u64 = 60_000;

#[cfg(all(not(feature = "sim"), has_buttons))]
fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

/// Millisecond clock reader used by the frame-budget loop regardless of
/// platform/feature flags. Kept separate from [`now_ms`] (which is gated on
/// `has_buttons`) so the event loop can query it on every board/sim combo.
#[cfg(not(test))]
fn now_ms_any() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

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
    use crate::system::executors::main_queue::{self, MainTask};
    use crate::system::picodroid::graphics::lvgl::with_gfx;
    use pico_jvm::NativeMethodHandler;

    // Initialise the unified main-thread FIFO; harmless if the module was
    // already initialised on a prior activity launch.
    main_queue::init();

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

    // Framework event loop -- unified FIFO drain interleaving LVGL ticks
    // and user-submitted Runnables for the main-thread executor.
    let mut pacer = crate::hal::system_clock::FramePacer::new();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut last_input_ms: u64 = now_ms();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut sleeping: bool = false;

    loop {
        // Check before the potentially-blocking LVGL render.
        if handler.interrupted() {
            break;
        }

        // Low-power sleep state: skip LVGL tick + dispatches and block on the
        // GPIO wake semaphore until the next button edge IRQ.
        #[cfg(all(not(feature = "sim"), has_buttons))]
        if sleeping {
            crate::hal::gpio::wait_for_button_event();
            if !crate::hal::gpio::has_pending_event() {
                // Stale signal latched during the awake phase — re-block.
                continue;
            }
            // Discard the wake press AND its release edge so it doesn't reach
            // LVGL focus navigation or Java OnKeyListener.
            while crate::hal::gpio::drain_gpio_event().is_some() {}
            with_gfx(|g| g.wake());
            sleeping = false;
            last_input_ms = now_ms();
            continue;
        }

        // Reset the idle timer if a button was pressed since the last frame.
        // `keypad_read_cb` will still drain & dispatch the event normally.
        #[cfg(all(not(feature = "sim"), has_buttons))]
        if crate::hal::gpio::has_pending_event() {
            last_input_ms = now_ms();
        }

        // Post one LvglTick at each frame boundary. Coalescing inside
        // main_queue ensures slow Runnables cannot cause ticks to pile up.
        main_queue::enqueue_tick();

        // Drain FIFO items until the queue is empty or we've used up the
        // 16 ms frame budget — whichever comes first. Remaining items spill
        // to the next frame. pacer.pace() below re-aligns the frame clock.
        let budget_end = now_ms_any() + 16;
        while now_ms_any() < budget_end {
            match main_queue::try_recv() {
                Some(MainTask::LvglTick) => {
                    with_gfx(|g| g.tick(16));
                    crate::system::picodroid::graphics::lvgl::fps_overlay::update();
                    dispatch_clicks(jvm, heap, handler);
                    dispatch_checked_changes(jvm, heap, handler);
                    dispatch_switch_checked_changes(jvm, heap, handler);
                    dispatch_seek_bar_changes(jvm, heap, handler);
                    dispatch_checkbox_changes(jvm, heap, handler);
                    dispatch_spinner_changes(jvm, heap, handler);
                    dispatch_key_events(jvm, heap, handler);
                    crate::system::picodroid::hardware::sensors::drain_sensor_events(
                        jvm, heap, handler,
                    );
                }
                Some(MainTask::Runnable(r)) => {
                    // Route through the `Executors.dispatchRunnable` bytecode
                    // bridge so the interpreter's invokeinterface path
                    // resolves lambda-proxy targets stored in Rust-side
                    // LambdaProxy metadata. Calling Runnable.run directly
                    // from Rust finds the abstract interface method with no
                    // bytecode and silently no-ops.
                    let _ = jvm.invoke_static_with_args(
                        dispatch_class(dispatch_sites::EXECUTORS_DISPATCH),
                        dispatch_method(dispatch_sites::EXECUTORS_DISPATCH),
                        &[pico_jvm::types::Value::ObjectRef(r)],
                        heap,
                        handler,
                    );
                }
                None => break,
            }
        }

        crate::hal::display::update_window();
        if !crate::hal::display::is_window_open() {
            break;
        }

        pacer.pace(16);

        #[cfg(all(not(feature = "sim"), has_buttons))]
        if now_ms() - last_input_ms >= IDLE_TIMEOUT_MS {
            with_gfx(|g| g.sleep());
            sleeping = true;
        }
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
                dispatch_class(dispatch_sites::BUTTON),
                dispatch_method(dispatch_sites::BUTTON),
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
fn dispatch_switch_checked_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_sw_checked_change_queue() {
        if let Some(obj_ref) = widgets::lookup_sw_checked_change_obj(handle) {
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
                dispatch_class(dispatch_sites::CHECKBOX),
                dispatch_method(dispatch_sites::CHECKBOX),
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
                dispatch_class(dispatch_sites::SPINNER),
                dispatch_method(dispatch_sites::SPINNER),
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
    use crate::system::picodroid::graphics::lvgl::events;
    use pico_jvm::types::Value;

    while let Some(raw) = events::drain_key_event() {
        let view_ref = match events::focused_view_obj() {
            Some(r) => r,
            None => continue,
        };
        let keycode = match events::pin_to_keycode(raw.pin) {
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
            dispatch_class(dispatch_sites::VIEW_KEY),
            dispatch_method(dispatch_sites::VIEW_KEY),
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
                dispatch_class(dispatch_sites::SEEK_BAR),
                dispatch_method(dispatch_sites::SEEK_BAR),
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
