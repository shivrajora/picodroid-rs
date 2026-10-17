// SPDX-License-Identifier: GPL-3.0-only
//! The UI build budget: what the main loop says about a Java span that held
//! the UI tick (docs/designs/ui-build-budget-2026-10.md).
//!
//! The loop is single-threaded, so every Java callback it runs stalls the
//! 16 ms tick for as long as it takes. Android's rule is the one applied
//! here: a screen build (an Activity starting, a page being bound) may take
//! a slow first frame, and the framework objects only to *sustained* jank
//! or to a single handler that is plainly stuck. Three classes of span:
//!
//! - **Build** — an Activity's start or resume, `Application.onCreate`, a
//!   Service op, and any other span that registered [`BUILD_VIEWS_MIN`]
//!   widgets or more (a Fragment commit, a page bind, a `setContentView`
//!   from a Runnable). Warned about once it exceeds the build budget.
//! - **Steady** — widget events, posted Runnables, network and alarm
//!   callbacks. One at [`SLOW_HANDLER_DEFAULT_MS`] or more is *counted*
//!   toward jank, not logged; one at the build budget or more is warned
//!   about on its own, since nothing excuses it.
//! - **Jank** — the steady spans' lost frames summed over a one-second
//!   window; [`JANK_LOST_FRAMES`] in a window is reported once.
//!
//! Every span ends in [`end_span`], which is the one place the running
//! totals ([`stats`]) are kept: a future `sysmon` line, a parity column or
//! a memory-monitor-style window report reads them from there.
//!
//! All state is atomics, read and written by the UI task alone — plain
//! loads and stores, since the RP2040 has no atomic read-modify-write and
//! there is one writer; `u32` milliseconds wrap after 49 days and every
//! subtraction is wrapping.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

/// `a += n` for a counter only the UI task touches.
fn add(a: &AtomicU32, n: u32) {
    a.store(a.load(Relaxed).wrapping_add(n), Relaxed);
}

/// `a = max(a, v)` for a counter only the UI task touches.
fn raise(a: &AtomicU32, v: u32) {
    if v > a.load(Relaxed) {
        a.store(v, Relaxed);
    }
}

/// Read and zero a counter only the UI task touches.
fn take(a: &AtomicU32) -> u32 {
    let v = a.load(Relaxed);
    a.store(0, Relaxed);
    v
}

use crate::executors::tick_source::TICK_PERIOD_MS;
use crate::graphics::lvgl::handle_table;

/// A steady-state handler at this many ms or more counts toward jank:
/// ≈ 3 frames of the 16 ms tick, the point a stutter is visible.
pub(super) const SLOW_HANDLER_DEFAULT_MS: u32 = 50;

/// A screen build may hold the tick this long before it is reported:
/// 15 frames, between the old 50 ms line and Android's 500 ms "skipped
/// frames" warning; the touch sampler's ring covers it on every board.
/// A steady handler this long is reported whatever it did.
pub(super) const BUILD_BUDGET_DEFAULT_MS: u32 = 250;

/// A steady span that registered this many widgets was building a screen
/// and is judged as a build. An input handler makes none; an
/// `AsyncLayoutInflater` slice a few; a page or a Settings column dozens.
pub(super) const BUILD_VIEWS_MIN: u32 = 8;

/// Frames lost to slow steady handlers within one window before the `jank`
/// line: ≈ 240 ms of a second spent in handlers of 50 ms or more.
pub(super) const JANK_LOST_FRAMES: u32 = 15;

pub(super) const JANK_WINDOW_MS: u32 = 1_000;

/// What a span was for; `end_span` promotes a steady span that built views.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SpanKind {
    Build,
    Steady,
}

// ── Configuration (resolved once per app run) ────────────────────────────────

static SLOW_MS: AtomicU32 = AtomicU32::new(SLOW_HANDLER_DEFAULT_MS);
static BUILD_MS: AtomicU32 = AtomicU32::new(BUILD_BUDGET_DEFAULT_MS);
/// Print every slow steady span on its own line (StrictMode's shape): on
/// when the simulator sets `PICODROID_SLOW_HANDLER_MS` explicitly.
static STRICT: AtomicBool = AtomicBool::new(false);

/// Resolve the thresholds. The device uses the compile-time defaults; the
/// simulator honours `PICODROID_SLOW_HANDLER_MS` (0 turns counting and the
/// jank line off; any explicit value also prints each slow steady span) and
/// `PICODROID_BUILD_BUDGET_MS` (0 turns the build line off), so either can
/// be tuned without a rebuild.
pub(super) fn init() {
    #[cfg(feature = "sim")]
    {
        if let Ok(v) = std::env::var("PICODROID_SLOW_HANDLER_MS") {
            SLOW_MS.store(v.trim().parse().unwrap_or(SLOW_HANDLER_DEFAULT_MS), Relaxed);
            STRICT.store(true, Relaxed);
        }
        if let Ok(v) = std::env::var("PICODROID_BUILD_BUDGET_MS") {
            BUILD_MS.store(v.trim().parse().unwrap_or(BUILD_BUDGET_DEFAULT_MS), Relaxed);
        }
    }
    let now = now_ms();
    WINDOW_START_MS.store(now, Relaxed);
    // The first warning may come at once: the boot screen's build is the
    // one most worth seeing, and the clock has barely left zero.
    LAST_WARN_MS.store(now.wrapping_sub(1000), Relaxed);
}

// ── Running totals: the aggregation hook ─────────────────────────────────────

/// Per-kind totals since the app started.
pub(crate) struct KindStats {
    pub spans: AtomicU32,
    /// Spans at or over their threshold (the build budget for a build, the
    /// slow-handler line for a steady span).
    pub slow: AtomicU32,
    pub total_ms: AtomicU32,
    pub max_ms: AtomicU32,
    /// Whole 16 ms frames the slow spans cost.
    pub lost_frames: AtomicU32,
}

impl KindStats {
    const fn new() -> Self {
        Self {
            spans: AtomicU32::new(0),
            slow: AtomicU32::new(0),
            total_ms: AtomicU32::new(0),
            max_ms: AtomicU32::new(0),
            lost_frames: AtomicU32::new(0),
        }
    }

    #[inline(never)]
    fn note(&self, elapsed_ms: u32, slow: bool, frames: u32) {
        add(&self.spans, 1);
        add(&self.total_ms, elapsed_ms);
        raise(&self.max_ms, elapsed_ms);
        if slow {
            add(&self.slow, 1);
            add(&self.lost_frames, frames);
        }
    }
}

/// The totals, by kind. Read them anywhere on the UI task.
pub(crate) struct SpanStats {
    pub build: KindStats,
    pub steady: KindStats,
}

static STATS: SpanStats = SpanStats {
    build: KindStats::new(),
    steady: KindStats::new(),
};

pub(crate) fn stats() -> &'static SpanStats {
    &STATS
}

/// The totals in one line, logged when the app's loop ends: the figure a
/// soak or a sim row reads back.
pub(super) fn log_summary() {
    let (b, s) = (&stats().build, &stats().steady);
    crate::pd_info!(
        "ui budget: {} builds ({} over budget, max {} ms); {} handlers ({} slow, {} frames lost, max {} ms)",
        b.spans.load(Relaxed),
        b.slow.load(Relaxed),
        b.max_ms.load(Relaxed),
        s.spans.load(Relaxed),
        s.slow.load(Relaxed),
        s.lost_frames.load(Relaxed),
        s.max_ms.load(Relaxed)
    );
}

// ── The jank window and the warning rate limit ──────────────────────────────

static WINDOW_START_MS: AtomicU32 = AtomicU32::new(0);
static WINDOW_LOST_FRAMES: AtomicU32 = AtomicU32::new(0);
static WINDOW_SLOW_SPANS: AtomicU32 = AtomicU32::new(0);
static LAST_WARN_MS: AtomicU32 = AtomicU32::new(0);

fn now_ms() -> u32 {
    super::now_ms() as u32
}

/// Once per tick: close a window that is a second old, reporting it when
/// the steady handlers inside it cost [`JANK_LOST_FRAMES`] or more.
pub(super) fn on_tick() {
    let now = now_ms();
    if now.wrapping_sub(WINDOW_START_MS.load(Relaxed)) < JANK_WINDOW_MS {
        return;
    }
    WINDOW_START_MS.store(now, Relaxed);
    let lost = take(&WINDOW_LOST_FRAMES);
    let spans = take(&WINDOW_SLOW_SPANS);
    if lost >= JANK_LOST_FRAMES {
        crate::pd_warn!(
            "jank: {} frames lost to {} slow handlers in the last second (each >= {} ms)",
            lost,
            spans,
            SLOW_MS.load(Relaxed)
        );
    }
}

/// One warning a second at most — a chain of slow steps shows as one line,
/// not ten. A counters build is a measurement build and reports every one.
fn may_warn(now: u32) -> bool {
    if cfg!(feature = "parity-metrics") || now.wrapping_sub(LAST_WARN_MS.load(Relaxed)) >= 1000 {
        LAST_WARN_MS.store(now, Relaxed);
        true
    } else {
        false
    }
}

// ── Spans ────────────────────────────────────────────────────────────────────

/// Where a timed main-loop span began: its clock reading, the widget count,
/// and in a `parity-metrics` build the JVM's work counters, so a warning can
/// say how many bytecodes and cold resolutions the span ran — the numbers
/// that turn "took 60 ms" into "interpreted 18,000 bytecodes".
#[derive(Clone, Copy)]
pub(super) struct SpanStart {
    ms: u32,
    widgets: u32,
    #[cfg(feature = "parity-metrics")]
    counters: Counters,
}

#[cfg(feature = "parity-metrics")]
#[derive(Clone, Copy)]
struct Counters {
    insns: usize,
    resolves: usize,
    find_class: usize,
    declines: usize,
    native_us: usize,
    native_calls: usize,
    resolve_us: usize,
    clinit_us: usize,
    invoke_us: usize,
    invokes: usize,
    frame_us: usize,
    fields_us: usize,
    new_us: usize,
    other_us: usize,
    field_ops: usize,
    parses: usize,
    parse_us: usize,
    /// Cost of two back-to-back clock reads, so a reader can subtract the
    /// measurement itself from the per-op columns.
    clock_ns: u64,
    #[cfg(not(feature = "sim"))]
    cpu_us: u32,
}

#[cfg(feature = "parity-metrics")]
impl Counters {
    fn read() -> Self {
        use pico_jvm::parity as p;
        let clock_ns = {
            let a = crate::hal::system_clock::elapsed_realtime_nanos();
            let _ = crate::hal::system_clock::elapsed_realtime_nanos();
            let c = crate::hal::system_clock::elapsed_realtime_nanos();
            (c - a).max(0) as u64 / 2
        };
        Self {
            insns: p::insns(),
            resolves: p::resolves(),
            find_class: p::find_class_calls(),
            declines: p::cache_declines(),
            native_us: p::native_us(),
            native_calls: p::native_calls(),
            resolve_us: p::resolve_us(),
            clinit_us: p::clinit_us(),
            invoke_us: p::invoke_us(),
            invokes: p::invokes(),
            frame_us: p::frame_us(),
            fields_us: p::fields_us(),
            new_us: p::new_us(),
            other_us: p::other_us(),
            field_ops: p::field_ops(),
            parses: p::parses(),
            parse_us: p::parse_us(),
            clock_ns,
            #[cfg(not(feature = "sim"))]
            cpu_us: crate::rtos::freertos::current_task_runtime_counter(),
        }
    }

    /// The counters' movement since `self`, as one log line.
    fn report(&self) {
        use pico_jvm::parity as p;
        let now = Self::read();
        let (slowest_us, slowest_class, slowest_method) = p::slowest_native();
        #[cfg(not(feature = "sim"))]
        let cpu_us = now.cpu_us.wrapping_sub(self.cpu_us);
        #[cfg(feature = "sim")]
        let cpu_us = 0u32;
        crate::pd_warn!(
            "  counters: cpu={} us native={} us/{} calls resolve={} us clinit={} us invoke={} us/{} frame={} us fields={} us/{} new={} us other={} us insns={} resolves={} find_class={} declines={} parsed={}/{} us slowest={}.{} {} us fastest={} us clock={} ns",
            cpu_us,
            now.native_us.wrapping_sub(self.native_us),
            now.native_calls.wrapping_sub(self.native_calls),
            now.resolve_us.wrapping_sub(self.resolve_us),
            now.clinit_us.wrapping_sub(self.clinit_us),
            now.invoke_us.wrapping_sub(self.invoke_us),
            now.invokes.wrapping_sub(self.invokes),
            now.frame_us.wrapping_sub(self.frame_us),
            now.fields_us.wrapping_sub(self.fields_us),
            now.field_ops.wrapping_sub(self.field_ops),
            now.new_us.wrapping_sub(self.new_us),
            now.other_us.wrapping_sub(self.other_us),
            now.insns.wrapping_sub(self.insns),
            now.resolves.wrapping_sub(self.resolves),
            now.find_class.wrapping_sub(self.find_class),
            now.declines.wrapping_sub(self.declines),
            now.parses.wrapping_sub(self.parses),
            now.parse_us.wrapping_sub(self.parse_us),
            slowest_class,
            slowest_method,
            slowest_us,
            p::fastest_native_us(),
            self.clock_ns
        );
        log_packtime();
    }
}

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

#[inline(never)]
pub(super) fn span_start() -> SpanStart {
    #[cfg(feature = "parity-metrics")]
    pico_jvm::parity::reset_slowest_native();
    SpanStart {
        ms: now_ms(),
        widgets: handle_table::registered_count(),
        #[cfg(feature = "parity-metrics")]
        counters: Counters::read(),
    }
}

/// Whether the simulator prints every span, not just the slow ones:
/// `PICODROID_TRACE_SPANS=1`. Read once.
#[cfg(feature = "sim")]
fn trace_spans() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PICODROID_TRACE_SPANS").is_ok_and(|v| v.trim() == "1"))
}

/// Close the span `start` opened: account it, and warn when the budget's
/// rule says so. `class` names the Activity or Application for a lifecycle
/// span and is empty for the rest; `kind` says what the span was for, and a
/// steady span that registered [`BUILD_VIEWS_MIN`] widgets is judged as a
/// build.
#[inline(never)]
pub(super) fn end_span(span: &'static str, class: &'static str, kind: SpanKind, start: SpanStart) {
    let now = now_ms();
    let elapsed = now.wrapping_sub(start.ms);
    let views = handle_table::registered_count().wrapping_sub(start.widgets);
    let kind = if kind == SpanKind::Build || views >= BUILD_VIEWS_MIN {
        SpanKind::Build
    } else {
        SpanKind::Steady
    };
    let frames = elapsed / TICK_PERIOD_MS;
    #[cfg(feature = "sim")]
    if trace_spans() {
        eprintln!(
            "[sim] span: {span} [{class}] {elapsed} ms views={views} kind={}",
            match kind {
                SpanKind::Build => "build",
                SpanKind::Steady => "steady",
            }
        );
        #[cfg(feature = "parity-metrics")]
        start.counters.report();
    }
    let slow_ms = SLOW_MS.load(Relaxed);
    let build_ms = BUILD_MS.load(Relaxed);
    match kind {
        SpanKind::Build => {
            let over = build_ms != 0 && elapsed >= build_ms;
            STATS.build.note(elapsed, over, frames);
            if over && may_warn(now) {
                crate::pd_warn!(
                    "screen build: {} [{}] took {} ms, {} views ({} frames skipped; budget {} ms)",
                    span,
                    class,
                    elapsed,
                    views,
                    frames,
                    build_ms
                );
                #[cfg(feature = "parity-metrics")]
                start.counters.report();
            }
        }
        SpanKind::Steady => {
            let slow = slow_ms != 0 && elapsed >= slow_ms;
            STATS.steady.note(elapsed, slow, frames);
            if !slow {
                return;
            }
            add(&WINDOW_LOST_FRAMES, frames);
            add(&WINDOW_SLOW_SPANS, 1);
            let stuck = build_ms != 0 && elapsed >= build_ms;
            // A counters build is a measurement build: every slow step shows.
            let strict = STRICT.load(Relaxed) || cfg!(feature = "parity-metrics");
            if (stuck || strict) && may_warn(now) {
                crate::pd_warn!(
                    "slow handler: {} took {} ms ({} frames skipped) — stalls the UI tick",
                    span,
                    elapsed,
                    frames
                );
                #[cfg(feature = "parity-metrics")]
                start.counters.report();
            }
        }
    }
}
