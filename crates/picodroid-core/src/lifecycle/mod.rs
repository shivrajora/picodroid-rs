// SPDX-License-Identifier: GPL-3.0-only
//! Application and Activity lifecycle management.
//!
//! This module owns the Android-like lifecycle callbacks (onCreate, event loop)
//! for both Application and Activity entry points.  The JVM setup, class
//! loading, and shared heap management remain in `app.rs`.

#[cfg(not(test))]
use crate::shrink_names::m;
use pico_jvm::types::{JvmError, Value};
#[cfg(not(test))]
use pico_jvm::{Jvm, SharedJvmHeap};

#[cfg(not(test))]
use crate::dispatch_sites::{self, DISPATCH_SITES};

mod activity_stack;
// Alarm delivery exists only where several apps can be installed.
#[cfg(has_multi_app)]
mod alarm_events;
// `pub(crate)`: the GC-root registration names `input::visit_gc_roots` by the
// module that defines it, which is what the completeness scan matches on.
pub(crate) mod input;
mod widget_events;

// The children's items, so each reads its siblings through `use super::*`
// exactly as the single file did.
#[cfg(has_multi_app)]
use self::alarm_events::*;
use self::{activity_stack::*, input::*, widget_events::*};

// What the rest of the crate names: the back-stack control type
// (`service_lifecycle`), the developer switch (`native_handler`, the settings
// app), and the recycled-event reset (`boot`).
pub(crate) use self::activity_stack::LifecycleControl;
pub use self::activity_stack::{dont_keep_activities, set_dont_keep_activities};
pub use self::input::reset_dispatch_event_state;

/// Look up the shrunk framework class name for the dispatch site at `idx`
/// in [`DISPATCH_SITES`]. Zero-cost identity when no shrink map is active.
#[cfg(not(test))]
#[inline]
fn dispatch_class(idx: usize) -> &'static str {
    DISPATCH_SITES[idx].0
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

fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

// ── Slow-handler watchdog ────────────────────────────────────────────────────

/// Default slow-handler threshold: 50 ms ≈ 3 frames of the 16 ms UI tick.
/// Android's 5 s ANR is the wrong scale for a 16 ms MCU loop — a handler that
/// overruns a few frames is already a visible stutter.
const SLOW_HANDLER_DEFAULT_MS: u64 = 50;

/// Resolve the watchdog threshold in ms (0 disables it). The device uses the
/// compile-time default; the sim honors `PICODROID_SLOW_HANDLER_MS` so it can
/// be tuned or turned off without a rebuild.
fn slow_handler_threshold_ms() -> u64 {
    #[cfg(feature = "sim")]
    if let Ok(v) = std::env::var("PICODROID_SLOW_HANDLER_MS") {
        return v.trim().parse().unwrap_or(SLOW_HANDLER_DEFAULT_MS);
    }
    SLOW_HANDLER_DEFAULT_MS
}

/// Warn — rate-limited to once a second — when `span` ran for at least
/// `slow_ms` since `start_ms`. The main loop is single-threaded, so a slow
/// handler directly stalls the UI tick; surfacing it points at the freeze.
/// Two clock reads on the fast (not-slow) path. `last_warn_ms` carries the
/// rate-limit state between calls.
fn warn_if_slow(span: &str, start_ms: u64, slow_ms: u64, last_warn_ms: &mut u64) {
    if slow_ms == 0 {
        return;
    }
    let elapsed = now_ms().saturating_sub(start_ms);
    if elapsed < slow_ms {
        return;
    }
    let now = now_ms();
    if now.saturating_sub(*last_warn_ms) < 1000 {
        return;
    }
    *last_warn_ms = now;
    #[cfg(not(feature = "sim"))]
    defmt::warn!(
        "slow handler: {=str} took {=u64} ms (>= {=u64} ms) — stalls the UI tick",
        span,
        elapsed,
        slow_ms
    );
    #[cfg(feature = "sim")]
    eprintln!(
        "[sim] slow handler: {span} took {elapsed} ms (>= {slow_ms} ms) — stalls the UI tick"
    );
}

// ── Application lifecycle ────────────────────────────────────────────────────

/// Run an Application-based app: allocate the Application object, call
/// `onCreate()`, then enter the activity loop with whichever Activity (if
/// any) the application's `onCreate` queued via `startActivity`.
///
/// The Application goes through [`instantiate_component`] like every other
/// framework-owned component: field defaults, `<init>` (so instance
/// initializers run — Android-faithful), then `@Inject` member injection,
/// all before `onCreate`.
#[cfg(not(test))]
pub(crate) fn run_application(
    jvm: &mut Jvm,
    application_class: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::native_handler::PendingActivityOp;

    let obj_ref = match instantiate_component(jvm, application_class, heap, handler) {
        Some(r) => r,
        None => {
            log_error!("failed to instantiate Application {}", application_class);
            return;
        }
    };

    // This task owns the UI from here on, Activity or not: until it is
    // recorded, every task passes for the UI task — a background-pool job
    // in an Activity-less app saw `Thread.currentThread()` named "main"
    // (QA 2026-09-13). The activity loop records it again on entry.
    crate::ui_thread::note_ui_task();
    match jvm.invoke_instance(application_class, m::onCreate, obj_ref, heap, handler) {
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
    use crate::native_handler::PendingOp;
    let mut activity_push: Option<(&'static str, Option<u16>)> = None;
    while let Some(op) = handler.take_next_pending_op() {
        match op {
            PendingOp::Activity(PendingActivityOp::Push {
                class_name,
                intent_ref,
                // The boot Activity is never launched for-result, so its
                // request_code/caller_ref are ignored.
                ..
            }) => {
                activity_push = Some((class_name, intent_ref));
                break;
            }
            PendingOp::Activity(PendingActivityOp::Pop { .. })
            | PendingOp::Activity(PendingActivityOp::Recreate { .. }) => {
                // No stack yet — nothing to pop or re-create.
            }
            PendingOp::Activity(PendingActivityOp::Launch) => {
                // Leaving before any Activity ran: nothing to drive, so the
                // service-only teardown below runs and the app returns.
                break;
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

/// Allocate a fresh framework-owned component (Application, Activity or
/// Service), run its no-arg `<init>`, then inject its `@Inject` members.
/// Returns the new ObjectRef, or `None` if allocation or initialization
/// failed (allocation OOM, missing class, constructor faulted, or a
/// cooperative stop). Field defaults are applied per JVMS before the
/// constructor runs.
#[cfg(not(test))]
pub(crate) fn instantiate_component(
    jvm: &mut Jvm,
    class_name: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    let obj_ref = heap
        .objects
        .alloc_with_defaults(class_name, jvm.classes())?;
    // Run <init> via the leaf class — invokespecial chains up to super.<init>.
    // If the class doesn't declare <init> (e.g. inherits the implicit default
    // from the superclass), ignore MethodNotFound: field defaults are already
    // applied and the implicit super-chain is a no-op.
    match jvm.invoke_instance(class_name, "<init>", obj_ref, heap, handler) {
        Ok(()) | Err(JvmError::MethodNotFound) => {
            inject_members(jvm, class_name, obj_ref, heap, handler)?;
            Some(obj_ref)
        }
        Err(JvmError::Interrupted) => None,
        Err(e) => {
            log_error!("<init> failed: {}", e);
            Some(obj_ref)
        }
    }
}

/// Hilt-style member injection for framework-owned components. Probes for
/// the `@Inject` annotation processor's generated
/// `<Class>_MembersInjector.injectMembers(instance)` — the leaf class's
/// injector already covers inherited members, so exactly one probe per
/// component — and calls it. Absence (`MethodNotFound`) is the common case
/// and costs one linear class-table scan. `$` becomes `_` to mirror the
/// processor's nested-class naming (`Outer$Inner` →
/// `Outer_Inner_MembersInjector`). See
/// docs/designs/inject-annotations-2026-08.md.
///
/// Returns `None` only on a cooperative stop; an injector that faults is
/// logged and the component is still handed back, matching `<init>`.
#[cfg(not(test))]
fn inject_members(
    jvm: &mut Jvm,
    class_name: &str,
    obj_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<()> {
    const SUFFIX: &str = "_MembersInjector";
    // One short String per component creation (rare: Activity push, Service
    // start, app boot) — not a hot path.
    let mut name = alloc::string::String::with_capacity(class_name.len() + SUFFIX.len());
    for c in class_name.chars() {
        name.push(if c == '$' { '_' } else { c });
    }
    name.push_str(SUFFIX);
    match jvm.invoke_static_with_args(
        &name,
        "injectMembers",
        &[Value::ObjectRef(obj_ref)],
        heap,
        handler,
    ) {
        Ok(()) | Err(JvmError::MethodNotFound) => Some(()),
        Err(JvmError::Interrupted) => None,
        Err(e) => {
            log_error!("injectMembers failed: {}", e);
            Some(())
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
///
/// Unless memory runs short (or "don't keep activities" is on): then a
/// covered Activity is destroyed with its state saved and re-created from
/// that Bundle when it is uncovered — see [`reclaim_covered_activities`].
#[cfg(not(test))]
pub(crate) fn run_activity(
    jvm: &mut Jvm,
    initial_class: &'static str,
    initial_ref: u16,
    initial_intent: Option<u16>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::executors::main_queue::{self, MainTask};
    use crate::graphics::display;
    use crate::graphics::lvgl::with_gfx;
    use pico_jvm::NativeMethodHandler;

    // Initialise the unified main-thread FIFO; harmless if the module was
    // already initialised on a prior activity launch.
    main_queue::init();
    // This task owns the widget tree from here on (`lvgl::with_gfx` warns
    // any other task that touches it — see `ui_thread`).
    crate::ui_thread::note_ui_task();

    // Bring up LVGL + allocate the Display singleton before the Activity
    // can run. `Display.setContentView` reads `g.screen()` unconditionally,
    // so an Activity that calls it without first touching `getDisplay()`
    // would otherwise segfault on an uninitialized graphics backend.
    // `display::get_instance` is idempotent — second-and-later activity
    // launches just return the cached singleton.
    let _ = display::get_instance(&mut heap.objects);

    init_dont_keep_activities();

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

    // Growth sentinels arm once onCreate has completed — construction-time
    // heap growth (class loading, widget trees) is legitimate; everything
    // past this point is steady state as far as leak detection is concerned.
    #[cfg(feature = "mem-diag")]
    crate::mem_diag::arm();

    // Framework event loop — pure dispatcher, mirroring Android's Looper.
    // The 16 ms LVGL cadence is provided by `tick_source` (a separate
    // FreeRTOS software timer on device, std::thread on sim) which posts
    // `MainTask::LvglTick` to the same queue. Posters of user Runnables
    // wake `recv_blocking` directly via the queue's send semantics.
    crate::executors::tick_source::start();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut last_input_ms: u64 = now_ms();
    #[cfg(all(not(feature = "sim"), has_buttons))]
    let mut sleeping: bool = false;

    // Slow-handler watchdog: a handler that overruns the threshold stalls the
    // single-threaded UI tick. Resolve the threshold once; track the last-warn
    // time for the 1/s rate limit.
    let slow_handler_ms = slow_handler_threshold_ms();
    let mut last_slow_warn_ms: u64 = 0;

    // Idle GC: the interpreter only collects when the allocation counter
    // crosses GC_THRESHOLD at a safepoint, so garbage from a churn burst
    // that ends short of the threshold sits unreclaimed for as long as the
    // app stays idle — which matters exactly for heap-capped apps parking
    // right after the burst. Mirror Android's idle-time GC: after 2 s with a
    // non-zero but unchanging allocation count, run one collection (which
    // zeroes the counter, naturally latching this off until allocations
    // resume). Measured on the clock, not in ticks: a slow app's ticks are
    // few and coalesced, and 125 of them can be a lot more than 2 s.
    const IDLE_GC_MS: u64 = 2_000;
    let mut idle_since_ms: Option<u64> = None;
    let mut idle_alloc_count: u16 = 0;

    // The package directory generation the alarm poll last saw; a change
    // forces a poll (see `dispatch_alarms`). Starts unseen so the first
    // tick polls once.
    #[cfg(has_multi_app)]
    let mut alarm_directory_seen: u32 = u32::MAX;

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
            crate::executors::tick_source::resume();
            crate::hardware::sensors::sampler::resume();
            sleeping = false;
            last_input_ms = now_ms();
            continue;
        }

        // Block until the tick source posts an LvglTick or a poster
        // submits a Runnable. Sub-ms wake on Runnable post.
        match main_queue::recv_blocking() {
            MainTask::LvglTick => {
                with_gfx(|g| g.tick(crate::executors::tick_source::step_ms()));
                crate::graphics::lvgl::fps_overlay::update();
                // Control-channel package verbs run here, on the JVM task,
                // so the directory keeps one writer.
                #[cfg(feature = "sim")]
                crate::hal::sim::app_region::service_requests();
                // Watch only the Java dispatch, not g.tick's render above —
                // rendering legitimately varies and would be a false positive.
                let span_start = now_ms();
                dispatch_widget_events(jvm, heap, handler);
                warn_if_slow(
                    "widget events",
                    span_start,
                    slow_handler_ms,
                    &mut last_slow_warn_ms,
                );

                // Tone segment boundaries, before anything that runs Java:
                // a late boundary is audible, and `dispatch_alarms` below can
                // spend a whole frame in an app's callback. Cheap — the
                // sequencer answers nothing unless a note actually ended.
                #[cfg(has_audio)]
                crate::media::on_tick();

                // Alarms due this tick. After widget dispatch, so an alarm
                // never lands mid-frame between a widget's callbacks; the
                // op it queues is drained by this same loop below.
                #[cfg(has_multi_app)]
                dispatch_alarms(jvm, heap, handler, &mut alarm_directory_seen);

                // Memory monitor window cadence — after widget dispatch so
                // each sample observes a settled frame.
                #[cfg(feature = "mem-diag")]
                crate::mem_diag::on_tick(jvm, heap, handler);

                // Idle GC (see IDLE_GC_MS above): sub-threshold garbage is
                // collected once allocations have stopped for 2 s.
                let ac = heap.gc_state.alloc_count;
                if ac != 0 && ac == idle_alloc_count {
                    let now = now_ms();
                    let since = *idle_since_ms.get_or_insert(now);
                    if now.saturating_sub(since) >= IDLE_GC_MS {
                        heap.collect_now(handler);
                        idle_since_ms = None;
                        idle_alloc_count = 0;
                    }
                } else {
                    idle_since_ms = None;
                    idle_alloc_count = ac;
                }

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
                            // The tick source is about to stop, and it is what
                            // advances a tone — anything still sounding would
                            // sound forever. Silence it on the way down.
                            #[cfg(has_audio)]
                            crate::media::stop();
                            crate::executors::tick_source::pause();
                            crate::hardware::sensors::sampler::pause();
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
                let span_start = now_ms();
                let dispatched = jvm.invoke_static_with_args(
                    dispatch_class(dispatch_sites::EXECUTORS_DISPATCH),
                    dispatch_method(dispatch_sites::EXECUTORS_DISPATCH),
                    &[pico_jvm::types::Value::ObjectRef(r)],
                    heap,
                    handler,
                );
                if let Err(e) = dispatched {
                    // A non-Java error skipped javac's `monitorexit`
                    // handlers; the UI task lives on, so anything it still
                    // holds would block every worker forever.
                    crate::monitor_store::release_all_held_by_current();
                    // An exception out of a main-queue Runnable used to vanish
                    // here — a chain of posted steps just stopped, with
                    // nothing in the log (QA 2026-09-13, qa_life on the
                    // RP2350). Android crashes the app for it; picodroid says
                    // what was thrown and carries on.
                    log_error!("mainExecutor Runnable error: {}", e);
                }
                warn_if_slow(
                    "Runnable",
                    span_start,
                    slow_handler_ms,
                    &mut last_slow_warn_ms,
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
        let span_start = now_ms();
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
        // onCreate building a large UI is the classic startup freeze; the drain
        // is where that work runs.
        warn_if_slow(
            "pending-op drain",
            span_start,
            slow_handler_ms,
            &mut last_slow_warn_ms,
        );
        if should_exit {
            break;
        }
    }

    crate::executors::tick_source::stop();
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
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    // The bootstrap Activity is never launched for-result.
    if !handler.push_activity(initial_ref, initial_class, initial_intent, None, 0) {
        log_error!("activity stack overflow on bootstrap: {}", initial_class);
        return LifecycleControl::Break;
    }
    // Give this Activity its own keypad focus group before onCreate so its
    // focusable widgets join the right group (see events::push_activity_group).
    crate::graphics::lvgl::events::push_activity_group();
    start_activity_instance(jvm, initial_ref, None, None, heap, handler)
}

/// Drive a new Activity instance to the foreground: `onCreate(saved)` →
/// `onStart` → (`onRestoreInstanceState(saved)`) → (`onActivityResult`) →
/// `onResume`. `saved` is the previous instance's Bundle on a re-creation and
/// `None` on a fresh launch, which hands `onCreate` a null and skips the
/// restore callback — Android's contract for both. `result` is a for-result
/// child's answer to a caller that was reclaimed meanwhile: the new instance
/// gets it where Android delivers it, after the restore and before
/// `onResume`.
#[cfg(not(test))]
#[allow(clippy::too_many_arguments)]
fn start_activity_instance(
    jvm: &mut Jvm,
    obj_ref: u16,
    saved: Option<u16>,
    result: Option<ActivityResult>,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use pico_jvm::types::Value;
    let saved_arg = saved.map_or(Value::Null, Value::ObjectRef);
    if invoke_trampoline(
        jvm,
        dispatch_sites::ACTIVITY_ON_CREATE,
        obj_ref,
        saved_arg,
        heap,
        handler,
    )
    .is_break()
        || invoke_lifecycle(
            jvm,
            dispatch_sites::ACTIVITY_ON_START,
            obj_ref,
            heap,
            handler,
        )
        .is_break()
    {
        return LifecycleControl::Break;
    }
    if saved.is_some()
        && invoke_trampoline(
            jvm,
            dispatch_sites::ACTIVITY_RESTORE_INSTANCE_STATE,
            obj_ref,
            saved_arg,
            heap,
            handler,
        )
        .is_break()
    {
        return LifecycleControl::Break;
    }
    if let Some(result) = result {
        if deliver_result(jvm, obj_ref, result, heap, handler).is_break() {
            return LifecycleControl::Break;
        }
    }
    invoke_lifecycle(
        jvm,
        dispatch_sites::ACTIVITY_ON_RESUME,
        obj_ref,
        heap,
        handler,
    )
}

/// `(request_code, result_code, result Intent)` for `onActivityResult`.
#[cfg(not(test))]
type ActivityResult = (i32, i32, Option<u16>);

/// Call `onActivityResult` on the Activity that asked for `result`.
#[cfg(not(test))]
fn deliver_result(
    jvm: &mut Jvm,
    obj_ref: u16,
    (request_code, result_code, result_intent): ActivityResult,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    use pico_jvm::types::Value;
    invoke_lifecycle_with_args(
        jvm,
        dispatch_sites::ACTIVITY_ON_ACTIVITY_RESULT,
        obj_ref,
        &[
            Value::Int(request_code),
            Value::Int(result_code),
            result_intent.map_or(Value::Null, Value::ObjectRef),
        ],
        heap,
        handler,
    )
}

/// Invoke one of Activity's `final` trampolines (`performCreate`,
/// `performStart`, …) with its arguments. Looked up on
/// `picodroid/app/Activity` itself — nothing can override a final method —
/// and the trampoline's invokevirtual then finds the app's override wherever
/// in the hierarchy it is declared, or Activity's default if there is none.
/// A by-name call on the app's class would do neither: the native lookup is
/// flat, so an override on an app's base Activity was silently never called.
#[cfg(not(test))]
fn invoke_lifecycle_with_args(
    jvm: &mut Jvm,
    site: usize,
    obj_ref: u16,
    args: &[pico_jvm::types::Value],
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    match jvm.invoke_instance_with_args_returning(
        dispatch_class(site),
        dispatch_method(site),
        obj_ref,
        args,
        heap,
        handler,
    ) {
        Ok(_) => LifecycleControl::Continue,
        Err(JvmError::Interrupted) => LifecycleControl::Break,
        Err(e) => {
            log_error!("Activity lifecycle error: {}", e);
            LifecycleControl::Continue
        }
    }
}

/// [`invoke_lifecycle_with_args`] for a callback that takes none.
#[cfg(not(test))]
fn invoke_lifecycle(
    jvm: &mut Jvm,
    site: usize,
    obj_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    invoke_lifecycle_with_args(jvm, site, obj_ref, &[], heap, handler)
}

/// [`invoke_lifecycle_with_args`] for the one-argument Bundle callbacks.
#[cfg(not(test))]
fn invoke_trampoline(
    jvm: &mut Jvm,
    site: usize,
    obj_ref: u16,
    arg: pico_jvm::types::Value,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> LifecycleControl {
    invoke_lifecycle_with_args(jvm, site, obj_ref, &[arg], heap, handler)
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
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::graphics::gfx::Handle;
    use crate::graphics::lvgl::with_gfx;

    let mut popped = 0usize;
    loop {
        // A reclaimed entry already had its onDestroy, and has no instance
        // to call anything on.
        let reclaimed = handler.top_activity_destroyed();
        let Some((act_ref, _, root)) = handler.pop_activity() else {
            break;
        };
        popped += 1;
        if !reclaimed {
            for site in [
                dispatch_sites::ACTIVITY_ON_PAUSE,
                dispatch_sites::ACTIVITY_ON_STOP,
                dispatch_sites::ACTIVITY_ON_DESTROY,
            ] {
                let _ = invoke_lifecycle(jvm, site, act_ref, heap, handler);
            }
        }
        // Free the saved root for parked entries. The topmost entry's view
        // is in CURRENT_ROOT_ID rather than its slot (it's still visible
        // until its onPause snapshot, which the teardown loop bypasses) —
        // free that one explicitly after the loop.
        if root != 0 {
            with_gfx(|g| g.delete(Handle::from_java(root)));
        }
    }
    let visible_root = crate::graphics::display::current_root_id();
    if visible_root != 0 {
        with_gfx(|g| g.delete(Handle::from_java(visible_root)));
    }
    crate::graphics::display::set_current_root_id(0);
    // Mirror handle_pop_op: every pushed Activity owns a keypad focus group,
    // and only the pop path freed them — a clean app exit left the group
    // stack at its high-water mark until the next boot's reset. Done after
    // the view trees are deleted so the groups are empty.
    for _ in 0..popped {
        crate::graphics::lvgl::events::pop_activity_group();
    }
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
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    // Android order: onTouch precedes onClick. Touch + long-press run first
    // so a consumed gesture can suppress the same-tick synthetic click (LVGL
    // enqueues the UP touch, the LONG_PRESSED, and the CLICKED in one g.tick).
    dispatch_touch_events(jvm, heap, handler);
    dispatch_long_clicks(jvm, heap, handler);
    dispatch_clicks(jvm, heap, handler);
    dispatch_checked_changes(jvm, heap, handler);
    dispatch_switch_checked_changes(jvm, heap, handler);
    dispatch_number_picker_steps(jvm, heap, handler);
    dispatch_seek_bar_changes(jvm, heap, handler);
    dispatch_seek_bar_tracking(jvm, heap, handler);
    dispatch_edit_text_changes(jvm, heap, handler);
    dispatch_checkbox_changes(jvm, heap, handler);
    dispatch_radio_button_changes(jvm, heap, handler);
    dispatch_spinner_changes(jvm, heap, handler);
    dispatch_list_view_item_clicks(jvm, heap, handler);
    dispatch_alert_dialog_clicks(jvm, heap, handler);
    dispatch_alert_dialog_item_clicks(jvm, heap, handler);
    dispatch_snackbar_action_clicks(jvm, heap, handler);
    dispatch_date_picker_changes(jvm, heap, handler);
    dispatch_time_picker_changes(jvm, heap, handler);
    dispatch_swipe_events(jvm, heap, handler);
    dispatch_view_focus_changes(jvm, heap, handler);
    dispatch_swipe_refresh(jvm, heap, handler);
    dispatch_keyboard_ready(jvm, heap, handler);
    dispatch_editor_actions(jvm, heap, handler);
    dispatch_animation_end_actions(jvm, heap, handler);
    dispatch_key_events(jvm, heap, handler);
    crate::hardware::sensors::drain_sensor_events(jvm, heap, handler);
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
use crate::shrink_names::c;
use log_error;
