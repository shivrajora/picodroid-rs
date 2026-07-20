// SPDX-License-Identifier: GPL-3.0-only
//! Platform glue for the `mem-diag` memory monitor (docs/memory-diagnostics.md).
//!
//! Drives one sample per monitor window off the 16 ms `MainTask::LvglTick`
//! cadence in [`crate::lifecycle::run_activity`], reads the native-heap and
//! JVM counters, prints the one-line `[memmon]` report, and feeds the
//! [`pico_jvm::mem_diag::GrowthSentinel`]s that watch for steady-state
//! growth. Only compiled under the `mem-diag` feature — absent the feature,
//! none of this exists in the binary.
//!
//! Threading: all state lives in main-task-only `static mut`s accessed via
//! `addr_of_mut!` (the `RECYCLED_MOTION_EVENT` discipline in
//! `lifecycle.rs`) — every caller is the UI main task. The one exception is
//! the sim control channel's `memstats` request, which crosses from the
//! reader thread as a host `AtomicBool`. Counters are plain fields because
//! thumbv6m (RP2040) has no atomic RMW instructions.
//!
//! Output rules: the monitor never allocates. Device output is `defmt` with
//! scalar args; sim output is `println!` under [`crate::sim_allocator::bypass`]
//! so the report cannot perturb the arena numbers it is reporting.

use pico_jvm::mem_diag::GrowthSentinel;
use pico_jvm::SharedJvmHeap;

use crate::system::native_handler::PicodroidNativeHandler;

/// Default monitor window when `PICODROID_MEMDIAG_WINDOW_MS` is unset (and
/// the device value, which has no env). Keep >= 500 ms: the sampler shares
/// the UI tick with the 50 ms slow-handler budget.
const DEFAULT_WINDOW_MS: u32 = 1000;
/// LVGL tick period the window cadence is derived from
/// (`crate::system::executors::tick_source::TICK_PERIOD_MS`).
const TICK_MS: u32 = 16;
/// GCs per window above which the distinct `GC-PRESSURE` alert fires —
/// churn symptom even when the live floor stays flat (10 GCs/s at the
/// default window = ~2560 allocs/s through the 256-alloc pacing threshold).
const GC_PRESSURE_PER_WINDOW: u32 = 10;

/// Simulated device heap size — same generated constant the sim allocator
/// models; on device it equals FreeRTOS `configTOTAL_HEAP_SIZE` (both are
/// generated from the mcu toml's `heap_kb`; see build.rs::emit_heap_config).
#[cfg(not(feature = "sim"))]
include!(concat!(env!("OUT_DIR"), "/heap_config.rs"));

// ── FreeRTOS heap FFI (device only) ─────────────────────────────────────────

/// Mirror of FreeRTOS `HeapStats_t` (heap_4.c). Field layout is fixed by
/// FreeRTOS; `size_t` = u32 on ARM32. The sim's `sim_heap4::HeapStats` is
/// the host-side mirror of the same numbers.
#[cfg(not(feature = "sim"))]
#[repr(C)]
struct FreeRtosHeapStats {
    available_heap_space_in_bytes: u32,
    size_of_largest_free_block_in_bytes: u32,
    size_of_smallest_free_block_in_bytes: u32,
    number_of_free_blocks: u32,
    minimum_ever_free_bytes_remaining: u32,
    number_of_successful_allocations: u32,
    number_of_successful_frees: u32,
}

#[cfg(not(feature = "sim"))]
extern "C" {
    fn xPortGetFreeHeapSize() -> u32;
    fn xPortGetMinimumEverFreeHeapSize() -> u32;
    fn vPortGetHeapStats(stats: *mut FreeRtosHeapStats);
}

// ── Monitor state (main task only) ──────────────────────────────────────────

struct MonitorState {
    banner_shown: bool,
    config_resolved: bool,
    ticks_per_window: u32,
    sentinel_on: bool,
    strict: bool,
    tick_count: u32,
    window_index: u32,
    live_sentinel: GrowthSentinel,
    native_sentinel: GrowthSentinel,
    prev_alloc_total: u32,
    prev_native_alloc: u32,
    prev_dyn_intern: u32,
    prev_gc_count: u32,
    prev_gc_freed: u32,
}

impl MonitorState {
    const fn new() -> Self {
        Self {
            banner_shown: false,
            config_resolved: false,
            ticks_per_window: DEFAULT_WINDOW_MS / TICK_MS,
            sentinel_on: true,
            strict: false,
            tick_count: 0,
            window_index: 0,
            live_sentinel: GrowthSentinel::new(),
            native_sentinel: GrowthSentinel::new(),
            prev_alloc_total: 0,
            prev_native_alloc: 0,
            prev_dyn_intern: 0,
            prev_gc_count: 0,
            prev_gc_freed: 0,
        }
    }
}

static mut STATE: MonitorState = MonitorState::new();

/// Main-task-only accessor; see the module note on threading.
fn state() -> &'static mut MonitorState {
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}

/// JVM allocations performed by native glue outside the interpreter's
/// `bump_alloc_count` funnel (direct `heap.objects.alloc` etc. in
/// lifecycle/sensor code). Main-task-only; kept separate from the bytecode
/// counter so `[memmon]` distinguishes app churn from native-glue churn.
static mut NATIVE_ALLOC_TOTAL: u32 = 0;

/// Record `n` native-side JVM allocations (see [`NATIVE_ALLOC_TOTAL`]).
pub fn note_native_alloc(n: u32) {
    unsafe {
        let t = &mut *core::ptr::addr_of_mut!(NATIVE_ALLOC_TOTAL);
        *t = t.wrapping_add(n);
    }
}

fn native_alloc_total() -> u32 {
    unsafe { *core::ptr::addr_of!(NATIVE_ALLOC_TOTAL) }
}

/// Sim control channel → main task: an on-demand `memstats` snapshot was
/// requested. Host atomics are fine here (sim only).
#[cfg(feature = "sim")]
static MEMSTATS_REQUESTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Called from the control-channel reader thread (`hal/sim/display.rs`).
#[cfg(feature = "sim")]
pub fn request_memstats() {
    MEMSTATS_REQUESTED.store(true, core::sync::atomic::Ordering::Release);
}

// ── Env config (sim; device uses the defaults) ─────────────────────────────

#[cfg(feature = "sim")]
fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.as_str(), "1" | "on" | "true" | "yes"),
        Err(_) => default,
    }
}

fn resolve_config(st: &mut MonitorState) {
    if st.config_resolved {
        return;
    }
    st.config_resolved = true;
    #[cfg(feature = "sim")]
    {
        let window_ms = std::env::var("PICODROID_MEMDIAG_WINDOW_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|ms| *ms >= TICK_MS)
            .unwrap_or(DEFAULT_WINDOW_MS);
        st.ticks_per_window = (window_ms / TICK_MS).max(1);
        st.sentinel_on = env_flag("PICODROID_MEMDIAG_SENTINEL", true);
        st.strict = env_flag("PICODROID_MEMDIAG_STRICT", false);
    }
    // Device: compiled-in defaults (1 s window, sentinel on, never strict —
    // a diagnostic must not halt a device).
}

/// True when the per-class alloc histogram is requested
/// (`PICODROID_MEMDIAG_HISTO=1`, sim only).
#[cfg(feature = "sim")]
fn histo_enabled() -> bool {
    use core::sync::atomic::{AtomicU8, Ordering};
    // 0 = unread, 1 = off, 2 = on (PICODROID_HANDLE_SANITIZER pattern).
    static CACHED: AtomicU8 = AtomicU8::new(0);
    match CACHED.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = env_flag("PICODROID_MEMDIAG_HISTO", false);
            CACHED.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Apply the runtime heap-diagnostic flags to a fresh heap. Called once at
/// heap creation (before class loading) so every alloc is counted and the
/// offensive checks cover the whole run.
#[cfg(feature = "sim")]
pub fn apply_heap_flags(heap: &mut SharedJvmHeap) {
    if histo_enabled() {
        heap.objects.set_histo_enabled(true);
    }
    if env_flag("PICODROID_MEMDIAG_OFFENSIVE", false) {
        pico_jvm::mem_diag::set_offensive(true);
        let _b = crate::sim_allocator::bypass();
        println!(
            "[memmon] offensive checks ON (poison-on-free, GC poison check, \
             post-GC integrity sweep, allocator canaries)"
        );
    }
}

/// Print the top allocation classes (histogram must be enabled). Sim-only:
/// sorting/formatting uses host std under `bypass()`.
#[cfg(feature = "sim")]
fn print_histo_top(heap: &SharedJvmHeap) {
    if !histo_enabled() {
        return;
    }
    let histo = heap.objects.alloc_histo();
    let _b = crate::sim_allocator::bypass();
    let mut entries: std::vec::Vec<(u32, u16)> = histo
        .iter()
        .enumerate()
        .filter(|(_, &c)| c > 0)
        .map(|(i, &c)| (c, i as u16))
        .collect();
    entries.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    print!("[memmon] histo top:");
    for (count, idx) in entries.iter().take(8) {
        let name = heap.objects.class_name_by_idx(*idx).unwrap_or("?");
        print!(" {name}={count}");
    }
    println!();
}

// ── Native heap sampling ────────────────────────────────────────────────────

struct NativeHeapSample {
    /// Bytes currently used in the arena (total - free).
    used: u32,
    /// Free bytes right now.
    free: u32,
    /// Lowest free-byte level ever observed (high-water complement).
    min_free: u32,
    /// Largest single free block (0 when unavailable).
    largest_free_block: u32,
}

#[cfg(feature = "sim")]
fn sample_native_heap() -> NativeHeapSample {
    let (cur, _peak, limit) = crate::sim_allocator::heap_stats();
    let (free, min_free, largest) = match crate::sim_allocator::heap4_stats() {
        Some(h) => (
            h.free_bytes as u32,
            h.min_ever_free_bytes as u32,
            h.largest_free_block as u32,
        ),
        // Uncapped mode (-l 0): no arena, only the byte meter exists.
        None => (
            limit.saturating_sub(cur) as u32,
            limit.saturating_sub(cur) as u32,
            0,
        ),
    };
    NativeHeapSample {
        used: cur as u32,
        free,
        min_free,
        largest_free_block: largest,
    }
}

#[cfg(not(feature = "sim"))]
fn sample_native_heap() -> NativeHeapSample {
    // SAFETY: plain FreeRTOS accessors; vPortGetHeapStats fills the struct
    // it is handed (layout mirrored from HeapStats_t above).
    unsafe {
        let free = xPortGetFreeHeapSize();
        let min_free = xPortGetMinimumEverFreeHeapSize();
        let mut stats = core::mem::MaybeUninit::<FreeRtosHeapStats>::uninit();
        vPortGetHeapStats(stats.as_mut_ptr());
        let stats = stats.assume_init();
        NativeHeapSample {
            used: (DEVICE_HEAP_BYTES as u32).saturating_sub(free),
            free,
            min_free,
            largest_free_block: stats.size_of_largest_free_block_in_bytes,
        }
    }
}

// ── Report output ───────────────────────────────────────────────────────────

/// Permille of free space NOT reachable as one contiguous block — 0 means
/// "all free space is one block", 900 means heavy fragmentation.
fn frag_permille(native: &NativeHeapSample) -> u32 {
    if native.free == 0 || native.largest_free_block == 0 {
        return 0;
    }
    1000u32.saturating_sub((native.largest_free_block.saturating_mul(1000)) / native.free)
}

#[allow(clippy::too_many_arguments)]
fn print_report(
    window: u32,
    live: u32,
    obj: u32,
    arr: u32,
    str_bytes: u32,
    floor: u32,
    native: &NativeHeapSample,
    gc_delta: u32,
    freed_delta: u32,
    alloc_delta: u32,
    native_alloc_delta: u32,
    intern_delta: u32,
) {
    let frag = frag_permille(native);
    #[cfg(feature = "sim")]
    {
        let _b = crate::sim_allocator::bypass();
        println!(
            "[memmon] w={} live={} obj={} arr={} str={} floor={} nused={} nfree={} nmin={} lblk={} gc=+{} freed=+{} alloc=+{} nalloc=+{} stri=+{} frag={}pm",
            window,
            live,
            obj,
            arr,
            str_bytes,
            floor,
            native.used,
            native.free,
            native.min_free,
            native.largest_free_block,
            gc_delta,
            freed_delta,
            alloc_delta,
            native_alloc_delta,
            intern_delta,
            frag,
        );
    }
    #[cfg(not(feature = "sim"))]
    defmt::info!(
        "memmon: w={=u32} live={=u32} obj={=u32} arr={=u32} str={=u32} floor={=u32} nused={=u32} nfree={=u32} nmin={=u32} lblk={=u32} gc=+{=u32} freed=+{=u32} alloc=+{=u32} nalloc=+{=u32} stri=+{=u32} frag={=u32}pm",
        window,
        live,
        obj,
        arr,
        str_bytes,
        floor,
        native.used,
        native.free,
        native.min_free,
        native.largest_free_block,
        gc_delta,
        freed_delta,
        alloc_delta,
        native_alloc_delta,
        intern_delta,
        frag,
    );
}

fn print_leak(kind: &'static str, r: &pico_jvm::mem_diag::LeakReport) {
    #[cfg(feature = "sim")]
    {
        let _b = crate::sim_allocator::bypass();
        println!(
            "[memmon] LEAK? {} floor rose +{} B over {} windows (baseline {} B, now {} B)",
            kind, r.delta, r.windows, r.baseline, r.now
        );
    }
    #[cfg(not(feature = "sim"))]
    defmt::warn!(
        "memmon: LEAK? {=str} floor rose +{=u32} B over {=u32} windows (baseline {=u32} B, now {=u32} B)",
        kind,
        r.delta,
        r.windows,
        r.baseline,
        r.now
    );
}

// ── Public hooks ────────────────────────────────────────────────────────────

/// Arm the growth sentinels. Called once the Activity's `onCreate` has
/// completed (legitimate construction growth is over); idempotent.
pub fn arm() {
    let st = state();
    resolve_config(st);
    st.live_sentinel.arm();
    st.native_sentinel.arm();
}

/// Per-tick hook — called from the `MainTask::LvglTick` arm. Cheap between
/// window boundaries (one increment + compare; plus a request-flag load on
/// sim).
pub fn on_tick(heap: &mut SharedJvmHeap, handler: &PicodroidNativeHandler) {
    let st = state();
    resolve_config(st);
    if !st.banner_shown {
        st.banner_shown = true;
        #[cfg(feature = "sim")]
        {
            let _b = crate::sim_allocator::bypass();
            println!(
                "[memmon] memdiag: ACTIVE (window={}ms sentinel={} strict={})",
                st.ticks_per_window * TICK_MS,
                if st.sentinel_on { "on" } else { "off" },
                if st.strict { "on" } else { "off" },
            );
        }
        #[cfg(not(feature = "sim"))]
        defmt::info!(
            "memdiag: ACTIVE (window={=u32}ms sentinel=on strict=off)",
            st.ticks_per_window * TICK_MS
        );
    }

    #[cfg(feature = "sim")]
    if MEMSTATS_REQUESTED.swap(false, core::sync::atomic::Ordering::Acquire) {
        snapshot(heap, handler);
    }

    st.tick_count += 1;
    if st.tick_count < st.ticks_per_window {
        return;
    }
    st.tick_count = 0;
    st.window_index += 1;
    sample_window(heap, handler);
}

/// Sample, report, and run the sentinels for the window that just ended.
fn sample_window(heap: &mut SharedJvmHeap, handler: &PicodroidNativeHandler) {
    let st = state();

    let obj = heap.objects.live_bytes() as u32;
    let arr = heap.arrays.live_bytes() as u32;
    let strb = heap.strings.live_bytes() as u32;
    let live = obj + arr + strb;
    // Post-GC floor: exact leak signal when a GC ran; before the first GC
    // the raw live sum is exact (nothing has ever been freed).
    let floor = heap.gc_state.take_window_post_gc_floor().unwrap_or(live);

    let native = sample_native_heap();

    let (_, gc_count, gc_freed) = handler.gc_stats();
    let gc_delta = gc_count.wrapping_sub(st.prev_gc_count);
    let freed_delta = gc_freed.wrapping_sub(st.prev_gc_freed);
    st.prev_gc_count = gc_count;
    st.prev_gc_freed = gc_freed;

    let alloc_total = heap.gc_state.alloc_total;
    let alloc_delta = alloc_total.wrapping_sub(st.prev_alloc_total);
    st.prev_alloc_total = alloc_total;

    let nat_total = native_alloc_total();
    let native_alloc_delta = nat_total.wrapping_sub(st.prev_native_alloc);
    st.prev_native_alloc = nat_total;

    let intern_total = heap.strings.dyn_intern_total();
    let intern_delta = intern_total.wrapping_sub(st.prev_dyn_intern);
    st.prev_dyn_intern = intern_total;

    print_report(
        st.window_index,
        live,
        obj,
        arr,
        strb,
        floor,
        &native,
        gc_delta,
        freed_delta,
        alloc_delta,
        native_alloc_delta,
        intern_delta,
    );

    if gc_delta >= GC_PRESSURE_PER_WINDOW {
        #[cfg(feature = "sim")]
        {
            let _b = crate::sim_allocator::bypass();
            println!(
                "[memmon] GC-PRESSURE {} GCs this window (alloc=+{} nalloc=+{}) — churn even if live is flat",
                gc_delta, alloc_delta, native_alloc_delta
            );
        }
        #[cfg(not(feature = "sim"))]
        defmt::warn!(
            "memmon: GC-PRESSURE {=u32} GCs this window (alloc=+{=u32} nalloc=+{=u32})",
            gc_delta,
            alloc_delta,
            native_alloc_delta
        );
    }

    if st.sentinel_on {
        let live_trip = st.live_sentinel.push_window(floor);
        let native_trip = st.native_sentinel.push_window(native.used);
        let tripped = live_trip.is_some() || native_trip.is_some();
        if let Some(r) = live_trip {
            print_leak("live", &r);
        }
        if let Some(r) = native_trip {
            print_leak("native", &r);
        }
        #[cfg(feature = "sim")]
        if tripped && st.strict {
            let _b = crate::sim_allocator::bypass();
            println!("[memmon] STRICT mode: aborting on sentinel trip");
            std::process::abort();
        }
        #[cfg(not(feature = "sim"))]
        let _ = tripped; // device never aborts on a diagnostic
    }
}

/// On-demand snapshot (sim `memstats` control command and the exit summary).
/// Prints one report line for the partial window without disturbing the
/// periodic cadence or the sentinels.
#[cfg(feature = "sim")]
pub fn snapshot(heap: &mut SharedJvmHeap, handler: &PicodroidNativeHandler) {
    let st = state();
    resolve_config(st);
    let obj = heap.objects.live_bytes() as u32;
    let arr = heap.arrays.live_bytes() as u32;
    let strb = heap.strings.live_bytes() as u32;
    let live = obj + arr + strb;
    // Peek the floor without draining the window (periodic cadence owns it).
    let floor = if heap.gc_state.min_post_gc_live != u32::MAX {
        heap.gc_state.min_post_gc_live
    } else if heap.gc_state.last_post_gc_live != 0 {
        heap.gc_state.last_post_gc_live
    } else {
        live
    };
    let native = sample_native_heap();
    let (_, gc_count, gc_freed) = handler.gc_stats();
    {
        let _b = crate::sim_allocator::bypass();
        println!(
            "[memmon] snapshot live={} obj={} arr={} str={} floor={} nused={} nfree={} nmin={} lblk={} gc={} freed={} alloc={} nalloc={} stri={} frag={}pm",
            live,
            obj,
            arr,
            strb,
            floor,
            native.used,
            native.free,
            native.min_free,
            native.largest_free_block,
            gc_count,
            gc_freed,
            heap.gc_state.alloc_total,
            native_alloc_total(),
            heap.strings.dyn_intern_total(),
            frag_permille(&native),
        );
    }
    print_histo_top(heap);
}
