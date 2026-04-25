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
/// `onCreate()`, then enter the activity loop with whichever Activity (if
/// any) the application's `onCreate` queued via `startActivity`.
#[cfg(not(test))]
pub(crate) fn run_application(
    jvm: &mut Jvm,
    application_class: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::native_handler::PendingActivityOp;

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

    // If onCreate() called startActivity(), enter the Activity loop with
    // that initial push. Anything else (e.g. Pop with empty stack) is a
    // no-op: nothing to drive.
    if let Some(PendingActivityOp::Push {
        obj_ref: act_ref,
        class_name: act_class,
    }) = handler.take_pending_op()
    {
        run_activity(jvm, act_class, act_ref, heap, handler);
    }
}

// ── Activity lifecycle ───────────────────────────────────────────────────────

/// Run the Activity stack starting with `(initial_class, initial_ref)`.
///
/// Owns:
/// - Pushing the initial Activity onto the handler stack and driving its
///   `onCreate` → `onStart` → `onResume`.
/// - The frame-budget event loop (LVGL tick + widget callback dispatch).
/// - Processing pending push/pop transitions queued by Java
///   (`startActivity`, `finish()`) between frames.
/// - Graceful teardown on exit (window close / interrupt / final `finish()`):
///   `onPause` → `onStop` → `onDestroy` are invoked on every still-live
///   stack entry, top-down.
///
/// **v1 caveat**: Activity content views are NOT preserved across pause.
/// When B is pushed over A, A's content view is freed by B's
/// `setContentView` call. When B finishes and A resumes, A must re-call
/// `setContentView` from its `onResume` to put its UI back. Activity-level
/// view preservation is a planned follow-up.
#[cfg(not(test))]
pub(crate) fn run_activity(
    jvm: &mut Jvm,
    initial_class: &'static str,
    initial_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::executors::main_queue::{self, MainTask};
    use crate::system::picodroid::graphics::lvgl::with_gfx;
    use pico_jvm::NativeMethodHandler;

    // Initialise the unified main-thread FIFO; harmless if the module was
    // already initialised on a prior activity launch.
    main_queue::init();

    // Bootstrap: push the initial Activity, then drive its onCreate ->
    // onStart -> onResume. From this point on the stack is non-empty for
    // the rest of the loop's lifetime.
    if !handler.push_activity(initial_ref, initial_class) {
        log_error!("activity stack overflow on bootstrap: {}", initial_class);
        return;
    }
    if invoke_lifecycle(
        jvm,
        initial_class,
        dispatch_sites::ACTIVITY_ON_CREATE,
        initial_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return;
    }
    if invoke_lifecycle(
        jvm,
        initial_class,
        dispatch_sites::ACTIVITY_ON_START,
        initial_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return;
    }
    if invoke_lifecycle(
        jvm,
        initial_class,
        dispatch_sites::ACTIVITY_ON_RESUME,
        initial_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return;
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
                    dispatch_alert_dialog_clicks(jvm, heap, handler);
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

        // Process any lifecycle transitions queued by Java during this
        // frame's dispatches (a button click handler called startActivity,
        // a key handler called finish(), etc.). At most one transition per
        // frame is the documented contract, but we drain in a `while` so
        // back-to-back queues collapse cleanly.
        let mut should_exit = false;
        while let Some(op) = handler.take_pending_op() {
            if process_pending_op(jvm, op, heap, handler).is_break() {
                should_exit = true;
                break;
            }
            if handler.current_activity().is_none() {
                // Last activity finish()ed — exit the loop and run teardown.
                should_exit = true;
                break;
            }
        }
        if should_exit {
            break;
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

    // ── Graceful teardown ─────────────────────────────────────────────
    // Reasons we can land here: window closed, JVM interrupt, or the last
    // activity was finish()ed via process_pending_op (in which case the
    // stack is already empty and the while-let body is a no-op). For
    // window-close / interrupt the stack still holds live entries; tear
    // them down top-down so each Activity sees the full Android contract.
    while let Some((act_ref, act_class)) = handler.pop_activity() {
        let _ = invoke_lifecycle(
            jvm,
            act_class,
            dispatch_sites::ACTIVITY_ON_PAUSE,
            act_ref,
            heap,
            handler,
        );
        let _ = invoke_lifecycle(
            jvm,
            act_class,
            dispatch_sites::ACTIVITY_ON_STOP,
            act_ref,
            heap,
            handler,
        );
        let _ = invoke_lifecycle(
            jvm,
            act_class,
            dispatch_sites::ACTIVITY_ON_DESTROY,
            act_ref,
            heap,
            handler,
        );
    }
    crate::system::picodroid::graphics::display::clear_content_view();
}

// ── Lifecycle invocation + transition processing ─────────────────────────────

/// Result of a lifecycle invocation, used by [`run_activity`] to decide
/// whether to continue the loop or unwind.
#[cfg(not(test))]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum LifecycleControl {
    /// Method ran (or was a no-op fallback); continue the loop.
    Continue,
    /// JVM cooperative interrupt — caller should return immediately.
    Break,
}

#[cfg(not(test))]
impl LifecycleControl {
    fn is_break(self) -> bool {
        matches!(self, LifecycleControl::Break)
    }
}

/// Invoke a lifecycle method on `subclass`, falling back to the default
/// no-op declared on `picodroid/app/Activity` if the subclass doesn't
/// override it.
///
/// `fallback_idx` is the [`DISPATCH_SITES`] index for the
/// `(picodroid/app/Activity, methodName)` pair — used both as the
/// fallback class for the second attempt AND as the source of the
/// method-name string for the first attempt.
#[cfg(not(test))]
fn invoke_lifecycle(
    jvm: &mut Jvm,
    subclass: &'static str,
    fallback_idx: usize,
    obj_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    let method = dispatch_method(fallback_idx);
    // First attempt: the receiver's runtime subclass. find_method_by_name
    // is a flat lookup, so this only succeeds if the subclass declares
    // the method itself (i.e. overrides the framework default).
    match jvm.invoke_instance(subclass, method, obj_ref, heap, handler) {
        Ok(()) => return LifecycleControl::Continue,
        Err(JvmError::MethodNotFound) => { /* fall through to the framework default */ }
        Err(JvmError::Interrupted) => return LifecycleControl::Break,
        Err(e) => {
            log_error!("Activity lifecycle error: {}", e);
            return LifecycleControl::Continue;
        }
    }
    let fallback_class = dispatch_class(fallback_idx);
    match jvm.invoke_instance(fallback_class, method, obj_ref, heap, handler) {
        Ok(()) => LifecycleControl::Continue,
        Err(JvmError::Interrupted) => LifecycleControl::Break,
        Err(e) => {
            log_error!("Activity lifecycle fallback error: {}", e);
            LifecycleControl::Continue
        }
    }
}

/// Process a single push or pop, invoking the canonical Android lifecycle
/// callback sequence on the activities involved. See the doc comment on
/// [`run_activity`] for the v1 view-preservation caveat.
#[cfg(not(test))]
fn process_pending_op(
    jvm: &mut Jvm,
    op: crate::system::native_handler::PendingActivityOp,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::system::native_handler::PendingActivityOp;

    match op {
        PendingActivityOp::Push {
            obj_ref: new_ref,
            class_name: new_class,
        } => {
            // Capture the previous top before pushing — needed for the
            // trailing onStop call after the new top is fully resumed.
            let prev = handler.current_activity();
            if let Some((prev_ref, prev_class)) = prev {
                if invoke_lifecycle(
                    jvm,
                    prev_class,
                    dispatch_sites::ACTIVITY_ON_PAUSE,
                    prev_ref,
                    heap,
                    handler,
                )
                .is_break()
                {
                    return LifecycleControl::Break;
                }
            }
            if !handler.push_activity(new_ref, new_class) {
                log_error!("activity stack overflow on push: {}", new_class);
                return LifecycleControl::Continue;
            }
            for site in [
                dispatch_sites::ACTIVITY_ON_CREATE,
                dispatch_sites::ACTIVITY_ON_START,
                dispatch_sites::ACTIVITY_ON_RESUME,
            ] {
                if invoke_lifecycle(jvm, new_class, site, new_ref, heap, handler).is_break() {
                    return LifecycleControl::Break;
                }
            }
            // New top is now fully resumed — stop the previous one. Order
            // matches Android: `prev.onStop` lands AFTER `new.onResume`.
            if let Some((prev_ref, prev_class)) = prev {
                if invoke_lifecycle(
                    jvm,
                    prev_class,
                    dispatch_sites::ACTIVITY_ON_STOP,
                    prev_ref,
                    heap,
                    handler,
                )
                .is_break()
                {
                    return LifecycleControl::Break;
                }
            }
            LifecycleControl::Continue
        }
        PendingActivityOp::Pop => {
            let (top_ref, top_class) = match handler.current_activity() {
                Some(t) => t,
                None => return LifecycleControl::Continue, // already empty
            };
            for site in [
                dispatch_sites::ACTIVITY_ON_PAUSE,
                dispatch_sites::ACTIVITY_ON_STOP,
                dispatch_sites::ACTIVITY_ON_DESTROY,
            ] {
                if invoke_lifecycle(jvm, top_class, site, top_ref, heap, handler).is_break() {
                    return LifecycleControl::Break;
                }
            }
            handler.pop_activity();
            // Free the destroyed activity's content view BEFORE the resumed
            // parent's onResume calls setContentView again — otherwise that
            // call's "delete prev" path would double-free.
            crate::system::picodroid::graphics::display::clear_content_view();
            if let Some((new_top_ref, new_top_class)) = handler.current_activity() {
                for site in [
                    dispatch_sites::ACTIVITY_ON_START,
                    dispatch_sites::ACTIVITY_ON_RESUME,
                ] {
                    if invoke_lifecycle(jvm, new_top_class, site, new_top_ref, heap, handler)
                        .is_break()
                    {
                        return LifecycleControl::Break;
                    }
                }
            }
            LifecycleControl::Continue
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

// ── AlertDialog button-click dispatch ──────────────────────────────────────

/// Drain the AlertDialog button-click queue and invoke `fireButtonClick(int)`
/// on each matching dialog Java object. The `which` value (0=positive,
/// 1=negative) is passed straight through; `AlertDialog.fireButtonClick`
/// routes to the correct Runnable on the Java side and dismisses the dialog.
#[cfg(not(test))]
fn dispatch_alert_dialog_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((dialog_handle, which)) = widgets::drain_dialog_click_queue() {
        if let Some(obj_ref) = widgets::lookup_dialog_obj(dialog_handle) {
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

// ── Hardware key-event dispatch ─────────────────────────────────────────────

/// Drain the hardware key-event queue, dispatch each event to the focused
/// View's `OnKeyListener`, and route un-consumed BACK releases to the top
/// Activity's `onBackPressed` (which defaults to `finish()`).
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

    /// Mirrors `KeyEvent.ACTION_UP` and `KeyEvent.KEYCODE_BACK` on the
    /// Java side. Hard-coded because there's no enum bridge from Java to
    /// Rust for these constants.
    const ACTION_UP: i32 = 1;
    const KEYCODE_BACK: i32 = 4;

    while let Some(raw) = events::drain_key_event() {
        let keycode = match events::pin_to_keycode(raw.pin) {
            Some(k) => k,
            None => continue,
        };
        let action = if raw.rising { 1 } else { 0 }; // ACTION_UP : ACTION_DOWN

        // 1) Dispatch to the focused View's OnKeyListener, if any. Capture
        //    fireKey's `boolean` return so an un-consumed BACK release can
        //    fall through to onBackPressed below.
        let consumed = match events::focused_view_obj() {
            Some(view_ref) => fire_view_key(jvm, view_ref, keycode, action, heap, handler),
            None => false,
        };

        // 2) Default BACK handler: invoke `Activity.onBackPressed` on the
        //    top activity when no View consumed the BACK release. Apps
        //    that want to suppress finish() can override `onBackPressed`
        //    to a no-op (or show a confirm dialog).
        if !consumed && keycode == KEYCODE_BACK && action == ACTION_UP {
            if let Some((act_ref, act_class)) = handler.current_activity() {
                let _ = invoke_lifecycle(
                    jvm,
                    act_class,
                    dispatch_sites::ACTIVITY_ON_BACK_PRESSED,
                    act_ref,
                    heap,
                    handler,
                );
            }
        }
    }
}

/// Build a `KeyEvent`, invoke `View.fireKey`, and return `true` if the
/// listener consumed the event (`fireKey` returned non-zero). Helper for
/// [`dispatch_key_events`] — keeps the BACK-routing logic readable.
#[cfg(not(test))]
fn fire_view_key(
    jvm: &mut Jvm,
    view_ref: u16,
    keycode: i32,
    action: i32,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> bool {
    use pico_jvm::types::Value;

    let event_obj = match heap.objects.alloc("picodroid/view/KeyEvent") {
        Some(o) => o,
        None => return false,
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
        return false;
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
        return false;
    }

    let ret = jvm.invoke_instance_with_args_returning(
        dispatch_class(dispatch_sites::VIEW_KEY),
        dispatch_method(dispatch_sites::VIEW_KEY),
        view_ref,
        &[Value::ObjectRef(event_obj)],
        heap,
        handler,
    );
    matches!(ret, Ok(Some(Value::Int(v))) if v != 0)
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
