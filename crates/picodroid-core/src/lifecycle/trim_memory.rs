// SPDX-License-Identifier: GPL-3.0-only
//! `onTrimMemory`: delivering the framework's memory-pressure signal.
//!
//! Android's `ComponentCallbacks2.onTrimMemory(level)` reaches the app in two
//! ways here. The activity stack delivers it before it destroys a covered
//! Activity to make room (`activity_stack::reclaim_covered_activities`), so a
//! view cache dropped then — its views go back to the LVGL pool — can save
//! that Activity. And the event loop delivers it once per *episode* when
//! either arena runs short while the app runs: the native heap read every
//! iteration through the O(1) counter (`host::native_heap_free_bytes`, a
//! word on the device), the LVGL pool every Nth frame (`lv_mem_monitor`
//! walks the pool). The arms, their thresholds and their re-arm bands are
//! [`crate::memory_pressure`] (pure, unit-tested); this file reads the
//! arenas and makes the upcall. The native arm seldom re-arms: the JVM keeps
//! the native memory it has grown into even after a GC, so a dropped Java
//! cache stops growth rather than returning bytes; the push path says it
//! again at each launch.
//!
//! One static upcall, `Context.fireTrimMemory(int)`, fans out in Java in
//! Android's order (the Application, then every live Activity, then every
//! Service); the level `LOW_MEMORY` (negative) means `onLowMemory`. Only
//! `TRIM_MEMORY_RUNNING_CRITICAL` is ever sent: one app runs at a time, so
//! nothing is "hidden" or "background" (docs/decisions.md 2026-10-20).

use super::*;
pub(super) use crate::memory_pressure::{EPISODE, LOW_MEMORY, RUNNING_CRITICAL};

/// One event-loop iteration's look at both arenas; `tick` says whether this
/// iteration was a frame tick (the pool is walked on every Nth). At most one
/// delivery per call.
pub(super) fn poll(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    tick: bool,
) -> LifecycleControl {
    // `memtrim <level>` from the simulator's control channel, pressure or not.
    #[cfg(feature = "sim")]
    if let Some(level) = crate::memory_pressure::take_synthetic() {
        return fire(jvm, heap, handler, level);
    }
    let ep = &EPISODE;
    let total = ep.native_total(|| {
        let s = crate::host::native_heap_stats();
        s.used_bytes.saturating_add(s.free_bytes)
    });
    let mut due = ep.observe_native(crate::host::native_heap_free_bytes(), total);
    if ep.pool_due(tick) && crate::graphics::lvgl::is_initialized() {
        let (free, total) = crate::graphics::lvgl::pool_free_bytes();
        due |= ep.observe_pool(free, total);
    }
    if due {
        return fire(jvm, heap, handler, RUNNING_CRITICAL);
    }
    LifecycleControl::Continue
}

/// Deliver `level` through `Context.fireTrimMemory`, on the main task. A
/// callback that throws past Java's own catch, or a fan-out that allocates
/// into a full heap, is logged and the loop carries on, as for a Runnable.
pub(super) fn fire(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    level: i32,
) -> LifecycleControl {
    crate::pd_info!("trim: level {}", level);
    match jvm.invoke_static_with_args(
        dispatch_class(dispatch_sites::TRIM_MEMORY),
        dispatch_method(dispatch_sites::TRIM_MEMORY),
        &[Value::Int(level)],
        heap,
        handler,
    ) {
        Ok(()) => LifecycleControl::Continue,
        Err(JvmError::Interrupted) => LifecycleControl::Break,
        Err(e) => {
            crate::monitor_store::release_all_held_by_current();
            log_error!("trim: fireTrimMemory error: {}", e);
            LifecycleControl::Continue
        }
    }
}
