// SPDX-License-Identifier: GPL-3.0-only
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

// `IDLE_TIMEOUT_MS: Option<u64>` — idle period (ms) after which the display
// is put to sleep, or `None` to disable. Resolved from board.toml's
// top-level `idle_timeout_ms` (default 60_000, `0` means disabled) by
// `build.rs`. Gated on `has_buttons` because the wake path blocks on a
// button IRQ — touch-only boards would never wake.
#[cfg(all(not(feature = "sim"), has_buttons))]
include!(concat!(env!("OUT_DIR"), "/sleep_config.rs"));

#[cfg(all(not(feature = "sim"), has_buttons))]
fn now_ms() -> u64 {
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

    // Drain any service ops queued during onCreate (start/bind/foreground)
    // and look for the first Activity push to drive. Service-only apps
    // never push an Activity — drained ops still run, then we tear down
    // surviving services and exit cleanly.
    use crate::system::native_handler::PendingOp;
    let mut activity_push: Option<(&'static str, Option<u16>)> = None;
    while let Some(op) = handler.take_next_pending_op() {
        match op {
            PendingOp::Activity(PendingActivityOp::Push {
                class_name,
                intent_ref,
            }) => {
                activity_push = Some((class_name, intent_ref));
                break;
            }
            PendingOp::Activity(PendingActivityOp::Pop) => {
                // No stack yet — nothing to pop.
            }
            PendingOp::Service(s) => {
                let _ = crate::service_lifecycle::process_pending_service_op(jvm, s, heap, handler);
            }
        }
    }

    if let Some((act_class, act_intent)) = activity_push {
        let act_ref = match instantiate_component(jvm, act_class, heap, handler) {
            Some(r) => r,
            None => {
                log_error!("failed to instantiate initial Activity {}", act_class);
                crate::service_lifecycle::destroy_all(jvm, heap, handler);
                return;
            }
        };
        run_activity(jvm, act_class, act_ref, act_intent, heap, handler);
    } else {
        // Service-only app or app that did nothing in onCreate — process
        // any further queued ops, then run final teardown so live Services
        // (started or bound) get an onDestroy.
        while let Some(op) = handler.take_next_pending_op() {
            if let PendingOp::Service(s) = op {
                let _ = crate::service_lifecycle::process_pending_service_op(jvm, s, heap, handler);
            }
        }
        crate::service_lifecycle::destroy_all(jvm, heap, handler);
    }
}

/// Allocate a fresh component (Activity or Service) and run its no-arg
/// `<init>`. Returns the new ObjectRef, or `None` if allocation or
/// initialization failed (allocation OOM, missing class, constructor
/// faulted). Field defaults are applied per JVMS before the constructor
/// runs.
#[cfg(not(test))]
pub(crate) fn instantiate_component(
    jvm: &mut Jvm,
    class_name: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    let obj_ref = heap
        .objects
        .alloc_with_defaults(class_name, jvm.classes())?;
    // Run <init> via the leaf class — invokespecial chains up to super.<init>.
    // If the class doesn't declare <init> (e.g. inherits the implicit default
    // from the superclass), ignore MethodNotFound: field defaults are already
    // applied and the implicit super-chain is a no-op.
    match jvm.invoke_instance(class_name, "<init>", obj_ref, heap, handler) {
        Ok(()) => Some(obj_ref),
        Err(JvmError::MethodNotFound) => Some(obj_ref),
        Err(JvmError::Interrupted) => None,
        Err(e) => {
            log_error!("<init> failed: {}", e);
            Some(obj_ref)
        }
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
/// Activity content views ARE preserved across pause: when B is pushed
/// over A, A's content view is hidden (set to `Visibility::Gone`) and
/// snapshotted into A's stack entry; when B finishes, A's saved view is
/// restored before `onStart`/`onResume`. Apps can build UIs in `onCreate`
/// and need not rebuild from `onResume`.
#[cfg(not(test))]
pub(crate) fn run_activity(
    jvm: &mut Jvm,
    initial_class: &'static str,
    initial_ref: u16,
    initial_intent: Option<u16>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::executors::main_queue::{self, MainTask};
    use crate::system::picodroid::graphics::display;
    use crate::system::picodroid::graphics::lvgl::with_gfx;
    use pico_jvm::NativeMethodHandler;

    // Initialise the unified main-thread FIFO; harmless if the module was
    // already initialised on a prior activity launch.
    main_queue::init();

    // Bring up LVGL + allocate the Display singleton before the Activity
    // can run. `Display.setContentView` reads `g.screen()` unconditionally,
    // so an Activity that calls it without first touching `getDisplay()`
    // would otherwise segfault on an uninitialized graphics backend.
    // `display::get_instance` is idempotent — second-and-later activity
    // launches just return the cached singleton.
    let _ = display::get_instance(&mut heap.objects);

    if bootstrap_activity(
        jvm,
        initial_class,
        initial_ref,
        initial_intent,
        heap,
        handler,
    )
    .is_break()
    {
        return;
    }

    // Framework event loop — pure dispatcher, mirroring Android's Looper.
    // The 16 ms LVGL cadence is provided by `tick_source` (a separate
    // FreeRTOS software timer on device, std::thread on sim) which posts
    // `MainTask::LvglTick` to the same queue. Posters of user Runnables
    // wake `recv_blocking` directly via the queue's send semantics.
    crate::system::executors::tick_source::start();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut last_input_ms: u64 = now_ms();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut sleeping: bool = false;

    loop {
        if handler.interrupted() {
            break;
        }

        // Low-power sleep state: pause the tick source so the chip can
        // enter deeper idle, and block on the GPIO wake semaphore until
        // the next button edge IRQ.
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
            crate::system::executors::tick_source::resume();
            sleeping = false;
            last_input_ms = now_ms();
            continue;
        }

        // Block until the tick source posts an LvglTick or a poster
        // submits a Runnable. Sub-ms wake on Runnable post.
        match main_queue::recv_blocking() {
            MainTask::LvglTick => {
                with_gfx(|g| g.tick(16));
                crate::system::picodroid::graphics::lvgl::fps_overlay::update();
                dispatch_widget_events(jvm, heap, handler);

                crate::hal::display::update_window();
                if !crate::hal::display::is_window_open() {
                    break;
                }

                #[cfg(all(not(feature = "sim"), has_buttons))]
                {
                    if crate::hal::gpio::has_pending_event() {
                        last_input_ms = now_ms();
                    }
                    if let Some(timeout) = IDLE_TIMEOUT_MS {
                        if now_ms() - last_input_ms >= timeout {
                            crate::system::executors::tick_source::pause();
                            with_gfx(|g| g.sleep());
                            sleeping = true;
                        }
                    }
                }
            }
            MainTask::Runnable(r) => {
                // Route through the `Executors.dispatchRunnable` bytecode
                // bridge so the interpreter's invokeinterface path resolves
                // lambda-proxy targets stored in Rust-side LambdaProxy
                // metadata. Calling Runnable.run directly from Rust finds
                // the abstract interface method with no bytecode and
                // silently no-ops.
                let _ = jvm.invoke_static_with_args(
                    dispatch_class(dispatch_sites::EXECUTORS_DISPATCH),
                    dispatch_method(dispatch_sites::EXECUTORS_DISPATCH),
                    &[pico_jvm::types::Value::ObjectRef(r)],
                    heap,
                    handler,
                );
            }
            MainTask::Wake => {
                // Cross-task nudge — fall through to the interrupt /
                // pending-op drain below without doing tick or runnable work.
            }
        }

        // Drain any lifecycle transitions queued by Java during the
        // dispatch above (a button click handler called startActivity,
        // a Runnable called finish(), etc.).
        let mut should_exit = false;
        while let Some(op) = handler.take_next_pending_op() {
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
    }

    crate::system::executors::tick_source::stop();
    teardown_activity(jvm, heap, handler);
}

/// Push the initial Activity and drive its onCreate → onStart → onResume
/// sequence. Returns `Break` if any callback hit a JVM interrupt; the
/// caller then unwinds without entering the event loop.
#[cfg(not(test))]
fn bootstrap_activity(
    jvm: &mut Jvm,
    initial_class: &'static str,
    initial_ref: u16,
    initial_intent: Option<u16>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    if !handler.push_activity(initial_ref, initial_class, initial_intent) {
        log_error!("activity stack overflow on bootstrap: {}", initial_class);
        return LifecycleControl::Break;
    }
    // Give this Activity its own keypad focus group before onCreate so its
    // focusable widgets join the right group (see events::push_activity_group).
    crate::system::picodroid::graphics::lvgl::events::push_activity_group();
    for site in [
        dispatch_sites::ACTIVITY_ON_CREATE,
        dispatch_sites::ACTIVITY_ON_START,
        dispatch_sites::ACTIVITY_ON_RESUME,
    ] {
        if invoke_lifecycle(jvm, initial_class, site, initial_ref, heap, handler).is_break() {
            return LifecycleControl::Break;
        }
    }
    LifecycleControl::Continue
}

/// Walk the activity stack top-down on shutdown, invoking the full Android
/// teardown sequence (onPause → onStop → onDestroy) on every Activity, then
/// free any leftover view trees and surviving Services.
///
/// Used after the main loop exits (window closed, JVM interrupt, or last
/// `finish()`). For the last-finish case the stack is already empty and the
/// while-let body is a no-op; the view + service cleanup still runs.
#[cfg(not(test))]
fn teardown_activity(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::gfx::Handle;
    use crate::system::picodroid::graphics::lvgl::with_gfx;

    while let Some((act_ref, act_class, root)) = handler.pop_activity() {
        for site in [
            dispatch_sites::ACTIVITY_ON_PAUSE,
            dispatch_sites::ACTIVITY_ON_STOP,
            dispatch_sites::ACTIVITY_ON_DESTROY,
        ] {
            let _ = invoke_lifecycle(jvm, act_class, site, act_ref, heap, handler);
        }
        // Free the saved root for parked entries. The topmost entry's view
        // is in CURRENT_ROOT_ID rather than its slot (it's still visible
        // until its onPause snapshot, which the teardown loop bypasses) —
        // free that one explicitly after the loop.
        if root != 0 {
            with_gfx(|g| g.delete(Handle::from_java(root)));
        }
    }
    let visible_root = crate::system::picodroid::graphics::display::current_root_id();
    if visible_root != 0 {
        with_gfx(|g| g.delete(Handle::from_java(visible_root)));
    }
    crate::system::picodroid::graphics::display::set_current_root_id(0);
    // Tear down any Services still alive. Foreground/started/bound — all
    // get a final onDestroy and have their banners cleared.
    crate::service_lifecycle::destroy_all(jvm, heap, handler);
}

/// Fan-out for every widget / sensor / input dispatcher invoked from the
/// LvglTick branch. Each call drains its own queue; the order matches the
/// pre-refactor inline sequence in [`run_activity`] and is significant for
/// dialog-then-click cases.
#[cfg(not(test))]
fn dispatch_widget_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    dispatch_clicks(jvm, heap, handler);
    dispatch_checked_changes(jvm, heap, handler);
    dispatch_switch_checked_changes(jvm, heap, handler);
    dispatch_number_picker_steps(jvm, heap, handler);
    dispatch_seek_bar_changes(jvm, heap, handler);
    dispatch_seek_bar_tracking(jvm, heap, handler);
    dispatch_checkbox_changes(jvm, heap, handler);
    dispatch_spinner_changes(jvm, heap, handler);
    dispatch_list_view_item_clicks(jvm, heap, handler);
    dispatch_alert_dialog_clicks(jvm, heap, handler);
    dispatch_snackbar_action_clicks(jvm, heap, handler);
    dispatch_date_picker_changes(jvm, heap, handler);
    dispatch_time_picker_changes(jvm, heap, handler);
    dispatch_swipe_events(jvm, heap, handler);
    dispatch_view_focus_changes(jvm, heap, handler);
    dispatch_swipe_refresh(jvm, heap, handler);
    dispatch_keyboard_ready(jvm, heap, handler);
    dispatch_editor_actions(jvm, heap, handler);
    dispatch_touch_events(jvm, heap, handler);
    dispatch_key_events(jvm, heap, handler);
    crate::system::picodroid::hardware::sensors::drain_sensor_events(jvm, heap, handler);
}

// ── Lifecycle invocation + transition processing ─────────────────────────────

/// Result of a lifecycle invocation, used by [`run_activity`] to decide
/// whether to continue the loop or unwind.
#[cfg(not(test))]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum LifecycleControl {
    /// Method ran (or was a no-op fallback); continue the loop.
    Continue,
    /// JVM cooperative interrupt — caller should return immediately.
    Break,
}

#[cfg(not(test))]
impl LifecycleControl {
    pub(crate) fn is_break(self) -> bool {
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

/// Park the current top Activity's content view: hide it and snapshot the
/// handle into its stack entry, then clear CURRENT_ROOT_ID so the next
/// `setContentView` lands on a clean slate.
#[cfg(not(test))]
fn park_top_view(handler: &mut crate::system::native_handler::PicodroidNativeHandler) {
    use crate::system::picodroid::graphics::display;
    use crate::system::picodroid::graphics::gfx::{Handle, Visibility};
    use crate::system::picodroid::graphics::lvgl::with_gfx;

    let prev_root = display::current_root_id();
    if prev_root != 0 {
        with_gfx(|g| g.set_visibility(Handle::from_java(prev_root), Visibility::Gone));
        handler.set_current_root_handle(prev_root);
        display::set_current_root_id(0);
    }
    // Dismiss any dialog the now-covered Activity left on screen. Its modal
    // scrim is parented to the screen (not the root hidden above), so it would
    // otherwise float over the incoming child Activity AND, as the topmost
    // "shown" dialog, steal the child's BACK. At push time every shown dialog
    // belongs to the Activity being covered, so dismissing all is correct —
    // the mirror of handle_pop_op's finish cleanup. See
    // project_picoenvmon_alertdialog_leak.
    while crate::system::picodroid::graphics::widgets::dismiss_topmost_dialog() {}
}

/// Inverse of [`park_top_view`]: restore the top Activity's saved view
/// handle back into CURRENT_ROOT_ID and make it visible again. Used both
/// after a Pop uncovers the parent and as the rollback path when a Push
/// hits the stack-overflow cap.
#[cfg(not(test))]
fn restore_top_view(handler: &mut crate::system::native_handler::PicodroidNativeHandler) {
    use crate::system::picodroid::graphics::display;
    use crate::system::picodroid::graphics::gfx::{Handle, Visibility};
    use crate::system::picodroid::graphics::lvgl::with_gfx;

    let saved = handler.current_root_handle();
    if saved != 0 {
        with_gfx(|g| g.set_visibility(Handle::from_java(saved), Visibility::Visible));
        display::set_current_root_id(saved);
        handler.set_current_root_handle(0);
    }
}

/// Handle `PendingActivityOp::Push` — pause + park the current top (if
/// any), push the new Activity, drive onCreate→onStart→onResume on it, then
/// trailing onStop on the previous top per Android ordering.
#[cfg(not(test))]
fn handle_push_op(
    jvm: &mut Jvm,
    new_class: &'static str,
    new_intent: Option<u16>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    // Framework owns instantiation: allocate the Activity and run its
    // no-arg constructor before the lifecycle callbacks.
    let new_ref = match instantiate_component(jvm, new_class, heap, handler) {
        Some(r) => r,
        None => {
            log_error!("failed to instantiate Activity {}", new_class);
            return LifecycleControl::Continue;
        }
    };
    // Capture the previous top before pushing — needed for the trailing
    // onStop call after the new top is fully resumed.
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
        park_top_view(handler);
    }
    if !handler.push_activity(new_ref, new_class, new_intent) {
        log_error!("activity stack overflow on push: {}", new_class);
        // Rollback: unpark prev's view so it isn't left hidden forever.
        if prev.is_some() {
            restore_top_view(handler);
        }
        return LifecycleControl::Continue;
    }
    // New top gets its own keypad focus group before onCreate, isolating its
    // focus from the parent's (which is parked with its focus intact).
    crate::system::picodroid::graphics::lvgl::events::push_activity_group();
    for site in [
        dispatch_sites::ACTIVITY_ON_CREATE,
        dispatch_sites::ACTIVITY_ON_START,
        dispatch_sites::ACTIVITY_ON_RESUME,
    ] {
        if invoke_lifecycle(jvm, new_class, site, new_ref, heap, handler).is_break() {
            return LifecycleControl::Break;
        }
    }
    // New top is now fully resumed — stop the previous one. Order matches
    // Android: `prev.onStop` lands AFTER `new.onResume`.
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

/// Handle `PendingActivityOp::Pop` — drive onPause→onStop→onDestroy on the
/// top, free its content view, auto-unbind any Service connections it
/// owned, then uncover the parent and restore its parked view.
#[cfg(not(test))]
fn handle_pop_op(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::system::picodroid::graphics::display;
    use crate::system::picodroid::graphics::gfx::Handle;
    use crate::system::picodroid::graphics::lvgl::with_gfx;

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
    // Free the destroyed activity's content view BEFORE popping the entry,
    // so the popped entry's saved root_handle (which tracks CURRENT_ROOT_ID
    // for the topmost) is consumed cleanly.
    let top_root = display::current_root_id();
    if top_root != 0 {
        with_gfx(|g| g.delete(Handle::from_java(top_root)));
        display::set_current_root_id(0);
    }
    // Tear down any AlertDialog the finishing Activity left on screen. The
    // dialog's modal scrim is parented to the screen, not the content view
    // deleted above, so it would otherwise outlive the Activity and leak onto
    // the one beneath as an input-absorbing modal — Android dismisses an
    // Activity's dialogs on destroy ("leaked window" prevention).
    while crate::system::picodroid::graphics::widgets::dismiss_topmost_dialog() {}
    // Auto-unbind any Service connections this Activity owned — mirrors
    // Android's behaviour for an Activity destroyed while holding bindings.
    // Runs after the Activity's own onDestroy so the Activity can still call
    // unbindService itself.
    crate::service_lifecycle::unbind_owned_by(top_ref, jvm, heap, handler);
    handler.pop_activity();
    // Tear down the popped Activity's keypad focus group and reactivate the
    // parent's (with its focus intact) — done after its view tree was deleted
    // above so the group is empty. No-op on boards without buttons.
    crate::system::picodroid::graphics::lvgl::events::pop_activity_group();
    // Restore the resumed parent's parked view, if any. Apps that build UI
    // in onCreate get their tree back without rebuilding; apps that rebuild
    // in onResume will replace it (the saved root will be deleted by
    // set_content_view's prev-delete branch).
    if let Some((new_top_ref, new_top_class)) = handler.current_activity() {
        restore_top_view(handler);
        // Android's stopped->foreground edge: onRestart precedes onStart when
        // returning after a child Activity finished above this one.
        for site in [
            dispatch_sites::ACTIVITY_ON_RESTART,
            dispatch_sites::ACTIVITY_ON_START,
            dispatch_sites::ACTIVITY_ON_RESUME,
        ] {
            if invoke_lifecycle(jvm, new_top_class, site, new_top_ref, heap, handler).is_break() {
                return LifecycleControl::Break;
            }
        }
    }
    LifecycleControl::Continue
}

/// Process a single Activity or Service transition, invoking the canonical
/// Android lifecycle callback sequence. See the doc comment on
/// [`run_activity`] for the v1 view-preservation caveat.
#[cfg(not(test))]
fn process_pending_op(
    jvm: &mut Jvm,
    op: crate::system::native_handler::PendingOp,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use crate::system::native_handler::{PendingActivityOp, PendingOp};

    match op {
        PendingOp::Service(s) => {
            crate::service_lifecycle::process_pending_service_op(jvm, s, heap, handler)
        }
        PendingOp::Activity(PendingActivityOp::Push {
            class_name,
            intent_ref,
        }) => handle_push_op(jvm, class_name, intent_ref, heap, handler),
        PendingOp::Activity(PendingActivityOp::Pop) => handle_pop_op(jvm, heap, handler),
    }
}

// ── Click dispatch ───────────────────────────────────────────────────────────

/// Drain the click queue and invoke `View.fireClick()` on each matching view.
#[cfg(not(test))]
fn dispatch_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_click_queue() {
        if let Some(obj_ref) = widgets::lookup_button_obj(handle) {
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

// ── NumberPicker keypad-step dispatch ───────────────────────────────────────

/// Drain the NumberPicker step queue and invoke `fireStep(int direction)` on
/// each matching picker. Steps are queued by the keypad edit-mode filter
/// (events.rs) when PREV/NEXT are pressed while a picker is being edited; the
/// Java side owns clamping, label refresh, and the OnValueChangeListener.
#[cfg(not(test))]
fn dispatch_number_picker_steps(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((handle, direction)) = widgets::drain_np_step_queue() {
        if let Some(obj_ref) = widgets::lookup_picker_obj(handle) {
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

/// Drain the Snackbar action-click queue and invoke `fireActionClick()` on
/// each matching Snackbar. The Java side runs the registered Runnable then
/// dismisses the snackbar.
#[cfg(not(test))]
fn dispatch_snackbar_action_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_snackbar_click_queue() {
        if let Some(obj_ref) = widgets::lookup_snackbar_obj(handle) {
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

/// Drain DatePicker selection events and invoke `fireDateChanged()`.
#[cfg(not(test))]
fn dispatch_date_picker_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_date_picker_queue() {
        if let Some(obj_ref) = widgets::lookup_date_picker_obj(handle) {
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
fn dispatch_swipe_refresh(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_refresh_queue() {
        if let Some(obj_ref) = widgets::lookup_refresh_obj(handle) {
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
fn dispatch_swipe_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::lvgl::events;
    use pico_jvm::types::Value;

    while let Some(rec) = events::drain_swipe_event() {
        if let Some(obj_ref) = events::lookup_swipe_view_obj(rec.view_handle) {
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
/// position)` on the registered listener for each event. A row activated by
/// ENTER (keypad) or a touch tap fires `LV_EVENT_CLICKED`; the Java side
/// resolves the row View / item id from the adapter and calls the
/// `AdapterView.OnItemClickListener`.
#[cfg(not(test))]
fn dispatch_list_view_item_clicks(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some(row) = widgets::drain_item_click_queue() {
        if let Some((obj_ref, position)) = widgets::lookup_item_click(row) {
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
fn dispatch_view_focus_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::lvgl::events;
    use pico_jvm::types::Value;

    while let Some(rec) = events::drain_focus_change_event() {
        if let Some(obj_ref) = events::lookup_focus_view_obj(rec.view_handle) {
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
fn dispatch_time_picker_changes(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_time_picker_queue() {
        if let Some(obj_ref) = widgets::lookup_time_picker_obj(handle) {
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

// ── Touch-event dispatch ────────────────────────────────────────────────────

/// Drain the touch-event queue and invoke `View.fireTouch(MotionEvent)` on
/// the registered listener for each event. Each record carries action
/// (DOWN / UP / LONG_PRESS), display-pixel position, and a tick-clock
/// millisecond timestamp consumed by `GestureDetector` for fling velocity.
#[cfg(not(test))]
fn dispatch_touch_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::lvgl::events;
    use pico_jvm::types::Value;

    while let Some(rec) = events::drain_touch_event() {
        let view_ref = match events::lookup_touch_view_obj(rec.view_handle) {
            Some(r) => r,
            None => continue,
        };

        let event_obj = match heap.objects.alloc(crate::shrink_names::shrink_class(
            "picodroid/view/MotionEvent",
        )) {
            Some(o) => o,
            None => continue,
        };
        let mut all_fields_set = true;
        for (slot, value) in [
            (
                crate::system::picodroid::graphics::fields::motion_event::ACTION,
                Value::Int(rec.action),
            ),
            (
                crate::system::picodroid::graphics::fields::motion_event::X,
                Value::Int(rec.x),
            ),
            (
                crate::system::picodroid::graphics::fields::motion_event::Y,
                Value::Int(rec.y),
            ),
            (
                crate::system::picodroid::graphics::fields::motion_event::EVENT_TIME,
                Value::Long(rec.time_ms as i64),
            ),
        ] {
            if heap.objects.set_field(event_obj, slot, value).is_none() {
                all_fields_set = false;
                break;
            }
        }
        if !all_fields_set {
            continue;
        }

        let _ = jvm.invoke_instance_with_args(
            dispatch_class(dispatch_sites::VIEW_TOUCH),
            dispatch_method(dispatch_sites::VIEW_TOUCH),
            view_ref,
            &[Value::ObjectRef(event_obj)],
            heap,
            handler,
        );
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

        // 1) BACK release first tries to dismiss the system soft keyboard
        //    if it's visible. Consumed if so — Activity stays on screen.
        if keycode == KEYCODE_BACK && action == ACTION_UP {
            use crate::system::picodroid::graphics::lvgl::widgets::keyboard;
            if keyboard::hide_system() {
                continue;
            }
        }

        // 1b) BACK then dismisses a showing AlertDialog (Android's cancelable
        //     default) before reaching the focused View or onBackPressed. This
        //     is also the only way to dismiss a dialog on a keypad-only board
        //     with no touch — and it stops the modal scrim from outliving its
        //     Activity. See project_picoenvmon_alertdialog_leak.
        if keycode == KEYCODE_BACK && action == ACTION_UP {
            use crate::system::picodroid::graphics::widgets;
            if widgets::has_shown_dialog() {
                widgets::dismiss_topmost_dialog();
                continue;
            }
        }

        // 2) Dispatch to the focused View's OnKeyListener, if any. Capture
        //    fireKey's `boolean` return so an un-consumed BACK release can
        //    fall through to onBackPressed below.
        let consumed = match events::focused_view_obj() {
            Some(view_ref) => fire_view_key(jvm, view_ref, keycode, action, heap, handler),
            None => false,
        };

        // 3) Default BACK handler: invoke `Activity.onBackPressed` on the
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

        // If this key initiated an Activity transition (startActivity /
        // finish), stop draining the batch. The remaining queued keys belong
        // to the *next* top Activity and must wait until the transition is
        // applied (between frames). Without this, a fast burst whose key
        // events land in a single tick — before the push/pop is processed — is
        // delivered entirely to the *departing* Activity, e.g. double-launching
        // it; combined with a deferred service bind that then mutates the
        // first instance's freed views, that was the History-screen segfault.
        // See project_picoenvmon_history_segfault.
        if handler.has_pending_activity_transition() {
            break;
        }
    }
}

// ── Keyboard READY-event dispatch ──────────────────────────────────────────

/// Drain the per-instance Keyboard READY ring buffer and invoke
/// `Keyboard.fireReady()` on each matching Java object. The system
/// keyboard does *not* go through here — it self-hides on its own
/// READY callback in [`crate::system::picodroid::graphics::lvgl::widgets::keyboard`].
#[cfg(not(test))]
fn dispatch_keyboard_ready(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;

    while let Some(handle) = widgets::drain_keyboard_ready_queue() {
        if let Some(obj_ref) = widgets::lookup_keyboard_obj(handle) {
            let _ = jvm.invoke_instance(
                dispatch_class(dispatch_sites::KEYBOARD_READY),
                dispatch_method(dispatch_sites::KEYBOARD_READY),
                obj_ref,
                heap,
                handler,
            );
        }
    }
}

// ── EditText editor-action dispatch ────────────────────────────────────────

/// Drain the system keyboard's pending editor-action and invoke
/// `EditText.fireEditorAction(int)` on the bound Java object. Set by
/// the system keyboard's OK callback in
/// [`crate::system::picodroid::graphics::lvgl::widgets::keyboard`].
#[cfg(not(test))]
fn dispatch_editor_actions(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;
    use pico_jvm::types::Value;

    if let Some(rec) = widgets::drain_editor_action() {
        let _ = jvm.invoke_instance_with_args(
            dispatch_class(dispatch_sites::EDIT_TEXT_EDITOR_ACTION),
            dispatch_method(dispatch_sites::EDIT_TEXT_EDITOR_ACTION),
            rec.edit_text_ref,
            &[Value::Int(rec.action_id)],
            heap,
            handler,
        );
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

    let event_obj = match heap
        .objects
        .alloc(crate::shrink_names::shrink_class("picodroid/view/KeyEvent"))
    {
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

/// Drain the seek bar press/release queue and invoke `fireTrackingTouch(boolean)`
/// on each matching SeekBar — onStartTrackingTouch / onStopTrackingTouch.
#[cfg(not(test))]
fn dispatch_seek_bar_tracking(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::system::native_handler::PicodroidNativeHandler,
) {
    use crate::system::picodroid::graphics::widgets;
    use pico_jvm::types::Value;

    while let Some((handle, started)) = widgets::drain_seek_tracking_queue() {
        if let Some(obj_ref) = widgets::lookup_seek_bar_obj(handle) {
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
