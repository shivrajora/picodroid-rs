// SPDX-License-Identifier: GPL-3.0-only
//! Delivering due alarms to the running app.

use super::*;

// ── Alarms ──────────────────────────────────────────────────────────────────

/// Ask [`crate::alarms`] what is due and act on the one thing it names.
///
/// Two outcomes. The alarm belongs to the running app, and its Activity is
/// pushed on top of whatever is showing — the same `startActivity` an app
/// makes, reached through `AlarmManager.fireAlarm`. Or it belongs to some
/// other app, and this one is torn down for it exactly as the HOME key does
/// it; the alarm is delivered by a later tick, once its owner is up.
#[cfg(all(not(test), has_multi_app))]
pub(super) fn dispatch_alarms(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    directory_seen: &mut u32,
) {
    use crate::alarms::{Action, Now};
    use crate::native_handler::{PendingActivityOp, PendingOp};

    // Most ticks stop here: the table is scanned, and the wall clock's
    // seqlock read, only once something can be due — or the package
    // directory changed, since a poll is also what forgets an uninstalled
    // owner's alarms.
    let elapsed_ms = crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000;
    let directory = crate::packages::directory_generation();
    if *directory_seen == directory
        && !crate::alarms::due(elapsed_ms, crate::os::system_clock::wall_offset_ms)
    {
        return;
    }
    *directory_seen = directory;
    let now = Now {
        elapsed_ms,
        wall_ms: elapsed_ms + crate::os::system_clock::wall_offset_ms(),
    };
    let action = crate::alarms::poll(
        now,
        crate::packages::running(),
        crate::packages::run_generation(),
        &|package| crate::packages::find(package).is_some(),
        handler.has_pending_launch(),
    );

    match action {
        None => {}
        Some(Action::Wake(wake)) => {
            let package = wake.package();
            match crate::packages::request_launch(package) {
                Ok(()) => {
                    crate::pd_info!("[alarm] wake {}", package);
                    handler.enqueue_op(PendingOp::Activity(PendingActivityOp::Launch));
                }
                // `poll` only names installed owners, so this is a package
                // uninstalled between the two calls; the next poll drops it.
                Err(_) => crate::pd_warn!("[alarm] wake {} failed: not installed", package),
            }
        }
        Some(Action::Fire(fire)) => fire_now(jvm, heap, handler, &fire),
    }
}

/// Hand one fired alarm to Java: an Activity alarm to `AlarmManager.fireAlarm`,
/// a broadcast one to `BroadcastReceiver.fireAlarm`, which takes the receiver
/// class and the action as two arguments, one of them null.
#[cfg(all(not(test), has_multi_app))]
fn fire_now(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
    fire: &crate::alarms::Fire,
) {
    use crate::alarms::Target;

    let Some(args) = fire_args(fire, heap, handler) else {
        crate::pd_error!("[alarm] fire dropped: no room to intern its strings");
        return;
    };
    let [name, k0, v0, k1, v1] = args;
    let (site, args): (usize, &[Value]) = match fire.target {
        Target::Activity => (dispatch_sites::ALARM_FIRE, &[name, k0, v0, k1, v1]),
        Target::Receiver => (
            dispatch_sites::ALARM_BROADCAST,
            &[name, Value::Null, k0, v0, k1, v1],
        ),
        Target::Action => (
            dispatch_sites::ALARM_BROADCAST,
            &[Value::Null, name, k0, v0, k1, v1],
        ),
    };
    match jvm.invoke_static_with_args(
        dispatch_class(site),
        dispatch_method(site),
        args,
        heap,
        handler,
    ) {
        Ok(()) => crate::pd_info!(
            "[alarm] fire {} {}#{} late {} ms{}",
            fire.package(),
            fire.class(),
            fire.request_code,
            fire.late_ms,
            fire.target.log_suffix()
        ),
        Err(JvmError::Interrupted) => {}
        Err(e) => {
            crate::monitor_store::release_all_held_by_current();
            log_error!("[alarm] fire failed: {}", e);
        }
    }
}

/// Deliver every alarm due to the running app before it leaves without an
/// Activity (`run_application`'s headless path): there is no tick to come
/// back for them, and a cold wake for a receiver-only app would otherwise
/// run it twice for nothing and drop the alarm. Other packages' alarms are
/// left for the next poll; nobody is woken from here.
#[cfg(all(not(test), has_multi_app))]
pub(super) fn deliver_due_alarms_headless(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) {
    use crate::alarms::Now;

    let Some(running) = crate::packages::running() else {
        return;
    };
    for _ in 0..crate::alarms::CAPACITY {
        let elapsed_ms = crate::hal::system_clock::elapsed_realtime_nanos() / 1_000_000;
        let now = Now {
            elapsed_ms,
            wall_ms: elapsed_ms + crate::os::system_clock::wall_offset_ms(),
        };
        let Some(fire) = crate::alarms::take_due_for(now, running) else {
            break;
        };
        fire_now(jvm, heap, handler, &fire);
    }
}

/// `fireAlarm`'s arguments: the class name and up to two `(key, value)`
/// extras, with a null key standing for an absent one.
///
/// Interning can fail when the string table is full; one emergency GC and a
/// retry, as [`ensure_recycled_key_event`] does, since no interpreter
/// safepoint can relieve the pressure from out here.
#[cfg(all(not(test), has_multi_app))]
pub(super) fn fire_args(
    fire: &crate::alarms::Fire,
    heap: &mut SharedJvmHeap,
    handler: &mut crate::native_handler::PicodroidNativeHandler,
) -> Option<[Value; 5]> {
    fn intern(
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

    // The class first: with it interned, a later failure costs only a
    // short-lived string rather than a half-built argument list.
    let class = Value::Reference(intern(fire.class(), heap, handler)?);
    let mut args = [
        class,
        Value::Null,
        Value::Int(0),
        Value::Null,
        Value::Int(0),
    ];
    for i in 0..crate::alarms::MAX_EXTRAS {
        let Some((key, value)) = fire.extra(i) else {
            break;
        };
        args[1 + i * 2] = Value::Reference(intern(key, heap, handler)?);
        args[2 + i * 2] = Value::Int(value);
    }
    Some(args)
}
