// SPDX-License-Identifier: GPL-3.0-only
//! The system broadcasts: one `BroadcastReceiver.fireSystem(mask)` per frame
//! (docs/designs/broadcasts-2026-10.md §3.3).
//!
//! Every source is an edge the loop folds between two ticks — the screen
//! bits `service_display_power` raised, a clock or zone generation that
//! moved, the wall-clock minute that changed — ANDed with the interest mask
//! the app's registered filters declared (`crate::broadcasts`). The edges
//! are read every tick, listener or not, so a change nobody was registered
//! for is not replayed to a receiver registered later: Android's registered
//! receivers hear only what happens after them. Nothing crosses to Java but
//! the mask; Java builds the Intents and fans out.

use super::*;
use crate::broadcasts;

/// What the loop last saw of each source.
pub(super) struct Seen {
    time_set: u32,
    zone: u32,
    /// The wall-clock minute at the last tick while `TIME_TICK` was wanted;
    /// `None` otherwise, so a receiver registered mid-minute hears the next
    /// edge and not its own registration.
    minute: Option<i64>,
    /// The link-change generation: `NETWORK_STATE_CHANGED` and
    /// `CONNECTIVITY_ACTION`, the two names Android has for the same edge.
    #[cfg(has_network)]
    link: u32,
    /// The Wi-Fi event generation, and what the station's status and the
    /// scan generation were when it last moved: the one counter covers
    /// both, and each action fires only for its own change.
    #[cfg(network_link_wifi)]
    wifi: u32,
    #[cfg(network_link_wifi)]
    status: crate::hal::wifi::Status,
    #[cfg(network_link_wifi)]
    scan: u32,
}

impl Seen {
    /// The sources as they stand when the loop starts.
    pub(super) fn now() -> Seen {
        Seen {
            time_set: broadcasts::time_set_generation(),
            zone: broadcasts::zone_generation(),
            minute: None,
            #[cfg(has_network)]
            link: crate::hal::net_edge::LINK_CHANGES.generation(),
            #[cfg(network_link_wifi)]
            wifi: crate::hal::wifi::WIFI_EVENTS.generation(),
            #[cfg(network_link_wifi)]
            status: crate::hal::wifi::status(),
            #[cfg(network_link_wifi)]
            scan: crate::hal::wifi::scan_generation(),
        }
    }

    /// The network edges since the last tick, as bits; 0 on a board with
    /// no network, where nothing below is compiled.
    #[allow(unused_mut)]
    fn network_edges(&mut self) -> u32 {
        let mut fired = 0;
        #[cfg(has_network)]
        {
            let link = crate::hal::net_edge::LINK_CHANGES.generation();
            if link != self.link {
                self.link = link;
                fired |= broadcasts::NETWORK_STATE | broadcasts::CONNECTIVITY;
            }
        }
        #[cfg(network_link_wifi)]
        {
            let wifi = crate::hal::wifi::WIFI_EVENTS.generation();
            if wifi != self.wifi {
                self.wifi = wifi;
                let status = crate::hal::wifi::status();
                if status != self.status {
                    self.status = status;
                    fired |= broadcasts::SUPPLICANT_STATE;
                }
                let scan = crate::hal::wifi::scan_generation();
                if scan != self.scan {
                    self.scan = scan;
                    fired |= broadcasts::SCAN_RESULTS;
                }
            }
        }
        fired
    }
}

/// Deliver the manifest broadcasts this run of `package` was started for,
/// if any: one `BroadcastReceiver.fireManifest(class, bits)` per declared
/// receiver class that names one of them. `true` when this is such a run,
/// which `run_application` then keeps headless.
#[cfg(has_multi_app)]
pub(super) fn deliver_manifest_broadcasts(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    package: &str,
) -> bool {
    let Some(bits) = broadcasts::manifest_current(package) else {
        return false;
    };
    let Some(entry) = crate::packages::find(package) else {
        return true;
    };
    // One call per receiver class, carrying the bits it declared.
    let mut done: [Option<&'static str>; 8] = [None; 8];
    let mut n = 0;
    for bit in [
        broadcasts::BOOT_COMPLETED,
        broadcasts::TIME_CHANGED,
        broadcasts::TIMEZONE_CHANGED,
    ] {
        if bits & bit == 0 {
            continue;
        }
        for class in entry.receivers_for(broadcasts::manifest_action(bit)) {
            if done[..n].contains(&Some(class)) {
                continue;
            }
            if n < done.len() {
                done[n] = Some(class);
                n += 1;
            }
            let mut mask = 0;
            for b in [
                broadcasts::BOOT_COMPLETED,
                broadcasts::TIME_CHANGED,
                broadcasts::TIMEZONE_CHANGED,
            ] {
                if bits & b != 0
                    && entry
                        .receivers_for(broadcasts::manifest_action(b))
                        .any(|c| c == class)
                {
                    mask |= b;
                }
            }
            let Some(name) = intern_or_collect(class, heap, handler) else {
                crate::pd_error!("[broadcast] manifest delivery dropped: no room for its string");
                continue;
            };
            match jvm.invoke_static_with_args(
                dispatch_class(dispatch_sites::MANIFEST_BROADCAST),
                dispatch_method(dispatch_sites::MANIFEST_BROADCAST),
                &[Value::Reference(name), Value::Int(mask as i32)],
                heap,
                handler,
            ) {
                Ok(()) => {
                    crate::pd_info!("[broadcast] manifest 0x{:x} -> {} {}", mask, package, class)
                }
                Err(JvmError::Interrupted) => return true,
                Err(e) => {
                    crate::monitor_store::release_all_held_by_current();
                    log_error!("[broadcast] fireManifest error: {}", e);
                }
            }
        }
    }
    true
}

/// Intern `text`, with one emergency collection when the string table is
/// full, as the alarm delivery does.
#[cfg(has_multi_app)]
fn intern_or_collect(
    text: &str,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<u16> {
    if let Some(idx) = heap.strings.intern_dyn(text.as_bytes()) {
        return Some(idx);
    }
    heap.collect_now(handler);
    heap.strings.intern_dyn(text.as_bytes())
}

/// Fold this frame's edges and, when a registered filter wants any of them,
/// hand them to `BroadcastReceiver.fireSystem` as one mask.
pub(super) fn dispatch_broadcasts(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    seen: &mut Seen,
) {
    let mut fired = broadcasts::take_pending();

    let time_set = broadcasts::time_set_generation();
    if time_set != seen.time_set {
        seen.time_set = time_set;
        fired |= broadcasts::TIME_CHANGED;
    }
    let zone = broadcasts::zone_generation();
    if zone != seen.zone {
        seen.zone = zone;
        fired |= broadcasts::TIMEZONE_CHANGED;
    }
    fired |= seen.network_edges();

    let interest = broadcasts::interest();
    // The wall clock's seqlock is read only while someone wants the minute.
    if interest & broadcasts::TIME_TICK != 0 {
        let elapsed_ms = crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000;
        let minute = broadcasts::minute_of(elapsed_ms + crate::os::system_clock::wall_offset_ms());
        if seen.minute.is_some_and(|last| last != minute) {
            fired |= broadcasts::TIME_TICK;
        }
        seen.minute = Some(minute);
    } else {
        seen.minute = None;
    }

    // A manifest delivery waiting for another package: the launcher is
    // the one app whose teardown costs nothing, so it makes way; the chain
    // runs between apps and the launcher comes back after it. Any other
    // app keeps the foreground; the delivery waits for its exit.
    #[cfg(has_multi_app)]
    {
        broadcasts::manifest_refresh();
        let running = crate::packages::running();
        let launcher = crate::packages::launcher().map(|l| l.package());
        if running.is_some()
            && running == launcher
            && !handler.has_pending_launch()
            && broadcasts::manifest_pending_for_other(running)
        {
            crate::pd_info!("[broadcast] manifest delivery: leaving the launcher");
            handler.enqueue_op(crate::native_handler::PendingOp::Activity(
                crate::native_handler::PendingActivityOp::Launch,
            ));
        }
    }

    let mask = fired & interest;
    if mask == 0 {
        return;
    }
    // A queued cross-package launch means this stack is about to be torn
    // down (the alarm poll's rule): an onReceive that started an Activity
    // would push into it. Single-app boards never launch across packages.
    #[cfg(has_multi_app)]
    if handler.has_pending_launch() {
        return;
    }
    match jvm.invoke_static_with_args(
        dispatch_class(dispatch_sites::BROADCAST),
        dispatch_method(dispatch_sites::BROADCAST),
        &[Value::Int(mask as i32)],
        heap,
        handler,
    ) {
        Ok(()) => crate::pd_info!("[broadcast] system 0x{:x}", mask),
        Err(JvmError::Interrupted) => {}
        Err(e) => {
            // A receiver threw past Java's own catch, or the fan-out
            // allocated into a full heap: say so and carry on, as for a
            // Runnable.
            crate::monitor_store::release_all_held_by_current();
            log_error!("[broadcast] fireSystem error: {}", e);
        }
    }
}
