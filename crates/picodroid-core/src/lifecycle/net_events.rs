// SPDX-License-Identifier: GPL-3.0-only
//! Telling the app the link came up or dropped.
//!
//! The Android shape is `ConnectivityManager.NetworkCallback`; picodroid's
//! is the same Java class, and Java keeps the registrations — this side only
//! reports the link's state when it changed. The IP stack's event hook (or
//! the simulator's `net up|down`) bumps [`LINK_CHANGES`]; every tick the
//! loop compares that generation with the one it last acted on, one atomic
//! load, and only a change costs a Java call: `fireLinkChange(boolean up)`
//! with the state *now*, so a flap inside one frame is folded into what the
//! frame sees rather than replayed. Java fans out `onAvailable` / `onLost`
//! from there, on this thread, between frames.

use super::*;
use crate::hal::net_edge::LINK_CHANGES;

/// The link-change generation now, for the loop's starting point.
pub(super) fn link_generation() -> u32 {
    LINK_CHANGES.generation()
}

/// Report the link's state to `ConnectivityManager` if it changed since
/// `seen`; a repeat of the same state is filtered on the Java side.
pub(super) fn dispatch_connectivity(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    seen: &mut u32,
) {
    let generation = LINK_CHANGES.generation();
    if generation == *seen {
        return;
    }
    *seen = generation;
    // The time task takes the same edge from here, on the JVM task: on the
    // simulator the link flips on host threads that cannot notify a kernel
    // task, and this loop is the first task context to see them. On the
    // device the IP hook already woke it; a second wake costs one pass.
    crate::time_service::task::link_changed();
    let up = crate::hal::net::is_network_up();
    match jvm.invoke_static_with_args(
        dispatch_class(dispatch_sites::CONNECTIVITY_CHANGE),
        dispatch_method(dispatch_sites::CONNECTIVITY_CHANGE),
        &[Value::Int(if up { 1 } else { 0 })],
        heap,
        handler,
    ) {
        Ok(()) => crate::pd_info!(
            "[net] link {} -> ConnectivityManager",
            if up { "up" } else { "down" }
        ),
        Err(JvmError::Interrupted) => {}
        Err(e) => {
            // A NetworkCallback threw, or the callback allocated into a full
            // heap. Android crashes the app for the first; picodroid says
            // what was thrown and carries on, as it does for a Runnable.
            crate::monitor_store::release_all_held_by_current();
            log_error!("[net] ConnectivityManager callback error: {}", e);
        }
    }
}

/// The WiFi event generation now, for the loop's starting point.
#[cfg(network_link_wifi)]
pub(super) fn wifi_generation() -> u32 {
    crate::hal::wifi::WIFI_EVENTS.generation()
}

/// A scan finished or the station's state changed since `seen`: tell
/// `WifiManager`, which fans scan completion out to its callbacks and
/// leaves the state for apps to read (docs/designs/wifi-provisioning-2026-09.md).
#[cfg(network_link_wifi)]
pub(super) fn dispatch_wifi_events(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    seen: &mut u32,
) {
    let generation = crate::hal::wifi::WIFI_EVENTS.generation();
    if generation == *seen {
        return;
    }
    *seen = generation;
    match jvm.invoke_static_with_args(
        dispatch_class(dispatch_sites::WIFI_EVENT),
        dispatch_method(dispatch_sites::WIFI_EVENT),
        &[],
        heap,
        handler,
    ) {
        Ok(()) => {}
        Err(JvmError::Interrupted) => {}
        Err(e) => {
            crate::monitor_store::release_all_held_by_current();
            log_error!("[net] WifiManager event error: {}", e);
        }
    }
}
