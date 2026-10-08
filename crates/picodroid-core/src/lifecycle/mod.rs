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
#[cfg(has_network)]
mod net_events;
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

fn now_ms() -> u64 {
    crate::hal::system_clock::elapsed_realtime_nanos() as u64 / 1_000_000
}

/// Once per tick: apply what the display's idle timer decided (power.rs) and
/// say whether the panel is dark. A doze turns the panel and backlight off
/// and silences a tone (its sequencer is driven by the LVGL tick, which
/// stops). A wake swallows what woke it — the queued edges, the touch ring,
/// the press LVGL would otherwise land — and repaints.
///
/// While dozing the input rings are not drained (LVGL is not ticking), so a
/// pending edge or a finger on the glass is visible here as the wake. The
/// sampler task keeps reading the GT911; the XPT2046 is polled inline.
fn service_display_power() -> bool {
    use crate::power::{DozeCause, Transition, WakeCause};
    // A soft key counts as a key: `KEYCODE_WAKEUP` from `input keyevent`, or
    // the soft-nav control, must wake a board that has no pin to press.
    let key_pending =
        crate::hal::gpio::has_pending_event() || crate::input_inject::soft_key_pending();
    // The panel is probed only while dozing, when LVGL is not reading it:
    // awake, the touch path stamps activity itself, and a second reader on
    // an XPT2046 would eat the "unsettled first reading" the sampler
    // discards and cost a bus transfer per tick.
    let touch_pending =
        !crate::power::is_interactive() && crate::hal::touch_sampler::latest().is_some();
    match crate::power::poll(key_pending, touch_pending) {
        Transition::None => !crate::power::is_interactive(),
        Transition::Doze(cause) => {
            #[cfg(has_audio)]
            crate::media::stop();
            crate::graphics::lvgl::with_gfx(|g| g.sleep());
            let n = crate::power::doze_number();
            match cause {
                DozeCause::Idle(ms) => crate::pd_info!("display: doze #{} after {} ms idle", n, ms),
                DozeCause::SleepKey => crate::pd_info!("display: doze #{} (sleep key)", n),
                DozeCause::PowerKey => crate::pd_info!("display: doze #{} (power key)", n),
            }
            true
        }
        Transition::Wake(cause) => {
            crate::graphics::lvgl::with_gfx(|g| g.wake());
            let n = crate::power::doze_number();
            match cause {
                WakeCause::Key => crate::pd_info!("display: wake #{} (key)", n),
                WakeCause::Touch => crate::pd_info!("display: wake #{} (touch)", n),
                WakeCause::Request => crate::pd_info!("display: wake #{} (request)", n),
            }
            false
        }
    }
}

/// Render-time counter for `parity-metrics` device builds: the LVGL tick's
/// render-plus-flush time, summed per second and printed as a `render:` line
/// when it exceeds a millisecond. The tick is not a timed span (rendering
/// legitimately varies), so this is the only place its cost shows; it is the
/// figure docs/designs/sram-hotpath-2026-09.md compares images by.
#[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
mod render_probe {
    use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
    static N: AtomicU32 = AtomicU32::new(0);
    static SUM_US: AtomicU32 = AtomicU32::new(0);
    static MAX_US: AtomicU32 = AtomicU32::new(0);
    static LAST_MS: AtomicU32 = AtomicU32::new(0);
    pub fn record(ns: u64) {
        let us = (ns / 1000) as u32;
        N.fetch_add(1, Relaxed);
        SUM_US.fetch_add(us, Relaxed);
        MAX_US.fetch_max(us, Relaxed);
        let now = super::now_ms() as u32;
        let last = LAST_MS.load(Relaxed);
        if last == 0 {
            LAST_MS.store(now, Relaxed);
            return;
        }
        if now.wrapping_sub(last) >= 1000 {
            let (n, sum, max) = (
                N.swap(0, Relaxed),
                SUM_US.swap(0, Relaxed),
                MAX_US.swap(0, Relaxed),
            );
            if sum >= 1000 {
                defmt::info!(
                    "render: {=u32} ticks {=u32} us max {=u32} us in {=u32} ms",
                    n,
                    sum,
                    max,
                    now.wrapping_sub(last)
                );
            }
            LAST_MS.store(now, Relaxed);
        }
    }
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

/// Where a timed main-loop span began: its clock reading and, in a
/// `parity-metrics` build, the JVM's work counters, so a slow-handler
/// warning can say how many bytecodes and cold resolutions the span ran —
/// the two numbers that turn "took 60 ms" into "interpreted 18,000
/// bytecodes", which is what a device's budget is really spent on.
#[derive(Clone, Copy)]
struct SpanStart {
    ms: u64,
    #[cfg(feature = "parity-metrics")]
    insns: usize,
    #[cfg(feature = "parity-metrics")]
    resolves: usize,
    #[cfg(feature = "parity-metrics")]
    find_class: usize,
    #[cfg(feature = "parity-metrics")]
    declines: usize,
    #[cfg(feature = "parity-metrics")]
    native_us: usize,
    #[cfg(feature = "parity-metrics")]
    native_calls: usize,
    #[cfg(feature = "parity-metrics")]
    resolve_us: usize,
    #[cfg(feature = "parity-metrics")]
    clinit_us: usize,
    #[cfg(feature = "parity-metrics")]
    invoke_us: usize,
    #[cfg(feature = "parity-metrics")]
    invokes: usize,
    #[cfg(feature = "parity-metrics")]
    frame_us: usize,
    #[cfg(feature = "parity-metrics")]
    fields_us: usize,
    #[cfg(feature = "parity-metrics")]
    new_us: usize,
    #[cfg(feature = "parity-metrics")]
    other_us: usize,
    #[cfg(feature = "parity-metrics")]
    field_ops: usize,
    #[cfg(feature = "parity-metrics")]
    parses: usize,
    #[cfg(feature = "parity-metrics")]
    parse_us: usize,
    /// Cost of two back-to-back clock reads, so a reader can subtract the
    /// measurement itself from the per-op columns.
    #[cfg(feature = "parity-metrics")]
    clock_ns: u64,
    #[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
    cpu_us: u32,
}

fn span_start() -> SpanStart {
    #[cfg(feature = "parity-metrics")]
    pico_jvm::parity::reset_slowest_native();
    #[cfg(feature = "parity-metrics")]
    let clock_ns = {
        let a = crate::hal::system_clock::elapsed_realtime_nanos();
        let _ = crate::hal::system_clock::elapsed_realtime_nanos();
        let c = crate::hal::system_clock::elapsed_realtime_nanos();
        (c - a).max(0) as u64 / 2
    };
    SpanStart {
        ms: now_ms(),
        #[cfg(feature = "parity-metrics")]
        insns: pico_jvm::parity::insns(),
        #[cfg(feature = "parity-metrics")]
        resolves: pico_jvm::parity::resolves(),
        #[cfg(feature = "parity-metrics")]
        find_class: pico_jvm::parity::find_class_calls(),
        #[cfg(feature = "parity-metrics")]
        declines: pico_jvm::parity::cache_declines(),
        #[cfg(feature = "parity-metrics")]
        native_us: pico_jvm::parity::native_us(),
        #[cfg(feature = "parity-metrics")]
        native_calls: pico_jvm::parity::native_calls(),
        #[cfg(feature = "parity-metrics")]
        resolve_us: pico_jvm::parity::resolve_us(),
        #[cfg(feature = "parity-metrics")]
        clinit_us: pico_jvm::parity::clinit_us(),
        #[cfg(feature = "parity-metrics")]
        invoke_us: pico_jvm::parity::invoke_us(),
        #[cfg(feature = "parity-metrics")]
        invokes: pico_jvm::parity::invokes(),
        #[cfg(feature = "parity-metrics")]
        frame_us: pico_jvm::parity::frame_us(),
        #[cfg(feature = "parity-metrics")]
        fields_us: pico_jvm::parity::fields_us(),
        #[cfg(feature = "parity-metrics")]
        new_us: pico_jvm::parity::new_us(),
        #[cfg(feature = "parity-metrics")]
        other_us: pico_jvm::parity::other_us(),
        #[cfg(feature = "parity-metrics")]
        field_ops: pico_jvm::parity::field_ops(),
        #[cfg(feature = "parity-metrics")]
        parses: pico_jvm::parity::parses(),
        #[cfg(feature = "parity-metrics")]
        parse_us: pico_jvm::parity::parse_us(),
        #[cfg(feature = "parity-metrics")]
        clock_ns,
        #[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
        cpu_us: crate::rtos::freertos::current_task_runtime_counter(),
    }
}

/// Whether the simulator prints every Runnable span, not just the slow
/// ones: `PICODROID_TRACE_SPANS=1`. Read once. Only meaningful with the
/// `parity-metrics` counters compiled in, which is where the bytecode and
/// resolution columns come from.
/// Running totals of the work a packer could take over — `ldc` (and how
/// many produced a `String`), `invokedynamic`, `checkcast` / `instanceof`,
/// `new` — on its own line so the parity line keeps its shape
/// (scripts/parity-bench.sh greps it whole). Totals, not deltas: subtract
/// two lines for a span.
#[cfg(feature = "parity-metrics")]
pub(crate) fn log_packtime() {
    let (ldcs, ldc_strings, ldc_us) = pico_jvm::parity::ldc_stats();
    let (indys, indy_us) = pico_jvm::parity::indy_stats();
    let (type_checks, type_check_us) = pico_jvm::parity::type_check_stats();
    let (copies, copy_bytes) = pico_jvm::parity::literal_copy_stats();
    crate::pd_info!(
        "packtime: ldc={} ldc_str={} ldc_us={} indy={} indy_us={} typechk={} typechk_us={} new={} new_us={} litcopy={} litcopy_bytes={}",
        ldcs,
        ldc_strings,
        ldc_us,
        indys,
        indy_us,
        type_checks,
        type_check_us,
        pico_jvm::parity::news(),
        pico_jvm::parity::new_us(),
        copies,
        copy_bytes
    );
}

#[cfg(feature = "sim")]
fn trace_spans() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PICODROID_TRACE_SPANS").is_ok_and(|v| v.trim() == "1"))
}

/// Warn — rate-limited to once a second — when `span` ran for at least
/// `slow_ms` since `start`. The main loop is single-threaded, so a slow
/// handler directly stalls the UI tick; surfacing it points at the freeze.
/// Two clock reads on the fast (not-slow) path. `last_warn_ms` carries the
/// rate-limit state between calls.
fn warn_if_slow(span: &str, start: SpanStart, slow_ms: u64, last_warn_ms: &mut u64) {
    #[cfg(feature = "sim")]
    if trace_spans() {
        let elapsed = now_ms().saturating_sub(start.ms);
        #[cfg(feature = "parity-metrics")]
        eprintln!(
            "[sim] span: {span} {elapsed} ms insns={} resolves={} find_class={} declines={} native={} us/{} calls resolve={} us clinit={} us invoke={} us/{} frame={} us parsed={}/{} us",
            pico_jvm::parity::insns().wrapping_sub(start.insns),
            pico_jvm::parity::resolves().wrapping_sub(start.resolves),
            pico_jvm::parity::find_class_calls().wrapping_sub(start.find_class),
            pico_jvm::parity::cache_declines().wrapping_sub(start.declines),
            pico_jvm::parity::native_us().wrapping_sub(start.native_us),
            pico_jvm::parity::native_calls().wrapping_sub(start.native_calls),
            pico_jvm::parity::resolve_us().wrapping_sub(start.resolve_us),
            pico_jvm::parity::clinit_us().wrapping_sub(start.clinit_us),
            pico_jvm::parity::invoke_us().wrapping_sub(start.invoke_us),
            pico_jvm::parity::invokes().wrapping_sub(start.invokes),
            pico_jvm::parity::frame_us().wrapping_sub(start.frame_us),
            pico_jvm::parity::parses().wrapping_sub(start.parses),
            pico_jvm::parity::parse_us().wrapping_sub(start.parse_us),
        );
        #[cfg(not(feature = "parity-metrics"))]
        eprintln!("[sim] span: {span} {elapsed} ms");
        #[cfg(feature = "parity-metrics")]
        log_packtime();
    }
    if slow_ms == 0 {
        return;
    }
    let elapsed = now_ms().saturating_sub(start.ms);
    if elapsed < slow_ms {
        return;
    }
    let now = now_ms();
    // A counters build is a measurement build: report every slow span,
    // not one a second, or a page turn's ten steps show as one line.
    if !cfg!(feature = "parity-metrics") && now.saturating_sub(*last_warn_ms) < 1000 {
        return;
    }
    *last_warn_ms = now;
    #[cfg(feature = "parity-metrics")]
    log_packtime();
    #[cfg(feature = "parity-metrics")]
    let (insns, resolves, declines, native_us, native_calls, resolve_us, clinit_us) = (
        pico_jvm::parity::insns().wrapping_sub(start.insns),
        pico_jvm::parity::resolves().wrapping_sub(start.resolves),
        pico_jvm::parity::cache_declines().wrapping_sub(start.declines),
        pico_jvm::parity::native_us().wrapping_sub(start.native_us),
        pico_jvm::parity::native_calls().wrapping_sub(start.native_calls),
        pico_jvm::parity::resolve_us().wrapping_sub(start.resolve_us),
        pico_jvm::parity::clinit_us().wrapping_sub(start.clinit_us),
    );
    #[cfg(feature = "parity-metrics")]
    let (invoke_us, invokes, frame_us, fastest_us, fields_us, new_us, other_us) = (
        pico_jvm::parity::invoke_us().wrapping_sub(start.invoke_us),
        pico_jvm::parity::invokes().wrapping_sub(start.invokes),
        pico_jvm::parity::frame_us().wrapping_sub(start.frame_us),
        pico_jvm::parity::fastest_native_us(),
        pico_jvm::parity::fields_us().wrapping_sub(start.fields_us),
        pico_jvm::parity::new_us().wrapping_sub(start.new_us),
        pico_jvm::parity::other_us().wrapping_sub(start.other_us),
    );
    #[cfg(feature = "parity-metrics")]
    let field_ops = pico_jvm::parity::field_ops().wrapping_sub(start.field_ops);
    #[cfg(feature = "parity-metrics")]
    let (parses, parse_us) = (
        pico_jvm::parity::parses().wrapping_sub(start.parses),
        pico_jvm::parity::parse_us().wrapping_sub(start.parse_us),
    );
    #[cfg(feature = "parity-metrics")]
    let clock_ns = start.clock_ns;
    #[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
    let cpu_us = crate::rtos::freertos::current_task_runtime_counter().wrapping_sub(start.cpu_us);
    #[cfg(feature = "parity-metrics")]
    let (slowest_us, slowest_class, slowest_method) = pico_jvm::parity::slowest_native();
    #[cfg(all(not(feature = "sim"), not(feature = "parity-metrics")))]
    defmt::warn!(
        "slow handler: {=str} took {=u64} ms (>= {=u64} ms) — stalls the UI tick",
        span,
        elapsed,
        slow_ms
    );
    #[cfg(all(not(feature = "sim"), feature = "parity-metrics"))]
    defmt::warn!(
        "slow handler: {=str} took {=u64} ms (>= {=u64} ms) — stalls the UI tick; cpu={=u32} us native={=usize} us/{=usize} calls resolve={=usize} us clinit={=usize} us invoke={=usize} us/{=usize} frame={=usize} us fields={=usize} us/{=usize} new={=usize} us other={=usize} us insns={=usize} resolves={=usize} declines={=usize} parsed={=usize}/{=usize} us slowest={=str}.{=str} {=usize} us fastest={=usize} us clock={=u64} ns",
        span,
        elapsed,
        slow_ms,
        cpu_us,
        native_us,
        native_calls,
        resolve_us,
        clinit_us,
        invoke_us,
        invokes,
        frame_us,
        fields_us,
        field_ops,
        new_us,
        other_us,
        insns,
        resolves,
        declines,
        parses,
        parse_us,
        slowest_class,
        slowest_method,
        slowest_us,
        fastest_us,
        clock_ns
    );
    #[cfg(all(feature = "sim", not(feature = "parity-metrics")))]
    eprintln!(
        "[sim] slow handler: {span} took {elapsed} ms (>= {slow_ms} ms) — stalls the UI tick"
    );
    #[cfg(all(feature = "sim", feature = "parity-metrics"))]
    eprintln!(
        "[sim] slow handler: {span} took {elapsed} ms (>= {slow_ms} ms) — stalls the UI tick; native={native_us} us/{native_calls} calls resolve={resolve_us} us clinit={clinit_us} us invoke={invoke_us} us/{invokes} frame={frame_us} us fields={fields_us} us/{field_ops} new={new_us} us other={other_us} us insns={insns} resolves={resolves} declines={declines} parsed={parses}/{parse_us} us slowest={slowest_class}.{slowest_method} {slowest_us} us fastest={fastest_us} us clock={clock_ns} ns"
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
    // No loop yet to come back for a held connect (an Application that
    // binds in its onCreate): release before every take, so it is
    // delivered here and the Activity push behind it is still found.
    while let Some(op) = {
        handler.release_held_ops();
        handler.take_next_pending_op()
    } {
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
        // (started or bound) get an onDestroy. There is no main loop to
        // come back for a held connect, so each round releases what the
        // previous one queued, until a round finds nothing.
        loop {
            handler.release_held_ops();
            let mut any = false;
            while let Some(op) = handler.take_next_pending_op() {
                any = true;
                if let PendingOp::Service(s) = op {
                    let _ =
                        crate::service_lifecycle::process_pending_service_op(jvm, s, heap, handler);
                }
            }
            if !any {
                break;
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
    crate::executors::scheduled::init();
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
    // The display's idle timer runs from here (power.rs): the first app of
    // a boot, or the next after a hand-over, starts with a lit panel.
    crate::power::user_activity();

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

    // The link-change generation the connectivity dispatch last saw. Starts
    // at the current one: a link already up is adopted by Java the first
    // time an app asks (`ConnectivityManager.syncActive`), so no push.
    #[cfg(has_network)]
    let mut link_generation_seen: u32 = net_events::link_generation();
    // Likewise the WiFi event generation: what happened before the loop
    // started is read on demand (`WifiManager.getConnectionInfo`).
    #[cfg(network_link_wifi)]
    let mut wifi_generation_seen: u32 = net_events::wifi_generation();

    loop {
        if handler.interrupted() {
            break;
        }

        // Block until the tick source posts an LvglTick or a poster
        // submits a Runnable. Sub-ms wake on Runnable post.
        match main_queue::recv_blocking() {
            MainTask::LvglTick => {
                // Doze (power.rs; docs/designs/app-portability-2026-10.md
                // K5): with the panel dark LVGL does not tick, so nothing
                // renders and no input is dispatched — a pending edge or a
                // finger on the glass is the wake instead. Everything else
                // in this arm keeps running: Runnables, alarms, scheduled
                // tasks, the network and the sensors do not know the panel
                // is off. The step clock runs on, so the first awake tick
                // feeds LVGL the whole dark interval and its clocks catch up.
                let dozing = service_display_power();
                if !dozing {
                    #[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
                    let tick_t0 = crate::hal::system_clock::elapsed_realtime_nanos();
                    with_gfx(|g| g.tick(crate::executors::tick_source::step_ms()));
                    #[cfg(all(feature = "parity-metrics", not(feature = "sim")))]
                    render_probe::record(
                        crate::hal::system_clock::elapsed_realtime_nanos().wrapping_sub(tick_t0)
                            as u64,
                    );
                    crate::graphics::lvgl::fps_overlay::update();
                    // One `[layout] fit ok` / `overflow` line per
                    // setContentView, now that the tick laid the root out.
                    #[cfg(any(feature = "sim", debug_assertions))]
                    crate::graphics::display::fit_check_after_tick();
                }
                // Control-channel package verbs run here, on the JVM task,
                // so the directory keeps one writer.
                #[cfg(feature = "sim")]
                crate::hal::sim::app_region::service_requests();
                #[cfg(feature = "sim")]
                crate::hal::sim::display::service_input_text();
                // Watch only the Java dispatch, not g.tick's render above —
                // rendering legitimately varies and would be a false positive.
                if !dozing {
                    let span_start = span_start();
                    dispatch_widget_events(jvm, heap, handler);
                    warn_if_slow(
                        "widget events",
                        span_start,
                        slow_handler_ms,
                        &mut last_slow_warn_ms,
                    );
                }

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

                // ScheduledExecutorService tasks due this tick: posted to
                // this queue, so each runs as a Runnable turn of its own.
                crate::executors::scheduled::fire_due();

                // The link came up or dropped since the last tick: tell
                // `ConnectivityManager`, whose callbacks then run here, on
                // the main thread between frames like every other callback.
                #[cfg(has_network)]
                net_events::dispatch_connectivity(jvm, heap, handler, &mut link_generation_seen);
                // A scan finished or the station's state moved: `WifiManager`
                // fans it out here too.
                #[cfg(network_link_wifi)]
                net_events::dispatch_wifi_events(jvm, heap, handler, &mut wifi_generation_seen);

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
            }
            MainTask::Runnable(r) => {
                // Route through the `Executors.dispatchRunnable` bytecode
                // bridge so the interpreter's invokeinterface path resolves
                // lambda-proxy targets stored in Rust-side LambdaProxy
                // metadata. Calling Runnable.run directly from Rust finds
                // the abstract interface method with no bytecode and
                // silently no-ops.
                let span_start = span_start();
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
        // a Runnable called finish(), etc.). A connect callback that a
        // bind queues in this drain is held for the next one, which the
        // bind's wake makes the very next turn: its span is its own.
        let mut should_exit = false;
        let span_start = span_start();
        handler.release_held_ops();
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
