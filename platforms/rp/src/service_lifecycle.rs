// SPDX-License-Identifier: GPL-3.0-only
//! Service lifecycle and registry.
//!
//! Owns the runtime state for `picodroid.app.Service` instances:
//!   * a small fixed registry (one entry per started/bound class),
//!   * a separate table of [`ConnectionEntry`]s keyed by Java
//!     `ServiceConnection` reference, scoped to the Activity that bound
//!     them so they auto-unbind when the owner pops,
//!   * the Android-canonical onCreate / onStartCommand / onBind /
//!     onUnbind / onDestroy callback sequencing.
//!
//! Services run on the main thread between frames; long-running work belongs
//! on a [`picodroid.concurrent.Thread`]. There is no system-killing on an
//! MCU — `START_STICKY` is therefore vacuously true and `onStartCommand`'s
//! return value is ignored.

#![cfg(not(test))]

use pico_jvm::types::{JvmError, Value};
use pico_jvm::{Jvm, SharedJvmHeap};

use crate::dispatch_sites::{self, DISPATCH_SITES};
use crate::system::native_handler::{PendingServiceOp, PicodroidNativeHandler};

const MAX_SERVICES: usize = 8;
const MAX_CONNECTIONS: usize = 16;

/// One running Service. Cleared from [`SERVICE_REGISTRY`] when neither
/// `started` nor `bind_count > 0`.
#[derive(Copy, Clone)]
struct ServiceEntry {
    obj_ref: u16,
    class_name: &'static str,
    /// Cached IBinder returned by the most recent `onBind`. Reused for
    /// subsequent binds — Android's contract is "one onBind per service
    /// instance" until the last unbind.
    binder_ref: Option<u16>,
    started: bool,
    bind_count: u16,
    foreground_id: Option<i32>,
    next_start_id: i32,
}

/// One live `ServiceConnection`. Owner is the Activity that called
/// `bindService`; 0 means application-scoped (no Activity on the stack).
#[derive(Copy, Clone)]
struct ConnectionEntry {
    conn_ref: u16,
    service_class: &'static str,
    owner_activity_ref: u16,
}

/// Registry — fixed-size to avoid heap growth on the hot path. Walking 8
/// entries on lookup is cheaper than a hash map for the expected service
/// counts on an MCU.
static mut SERVICE_REGISTRY: [Option<ServiceEntry>; MAX_SERVICES] = [None; MAX_SERVICES];

/// Connection table — fixed-size for the same reason.
static mut CONNECTIONS: [Option<ConnectionEntry>; MAX_CONNECTIONS] = [None; MAX_CONNECTIONS];

// SAFETY: every public function here takes `&mut SharedJvmHeap` (or runs
// during Jvm execution at lifecycle boundaries), so the JVM's main-thread
// invariant guarantees exclusive access. The static tables are only touched
// from these entry points.

fn registry() -> &'static mut [Option<ServiceEntry>] {
    #[allow(static_mut_refs)]
    unsafe {
        &mut SERVICE_REGISTRY
    }
}

fn connections() -> &'static mut [Option<ConnectionEntry>] {
    #[allow(static_mut_refs)]
    unsafe {
        &mut CONNECTIONS
    }
}

fn registry_find(class_name: &str) -> Option<usize> {
    registry()
        .iter()
        .position(|e| matches!(e, Some(s) if s.class_name == class_name))
}

fn connection_find(conn_ref: u16) -> Option<usize> {
    connections()
        .iter()
        .position(|c| matches!(c, Some(e) if e.conn_ref == conn_ref))
}

fn dispatch_class(idx: usize) -> &'static str {
    crate::shrink_names::shrink_class(DISPATCH_SITES[idx].0)
}

fn dispatch_method(idx: usize) -> &'static str {
    DISPATCH_SITES[idx].1
}

/// Process one [`PendingServiceOp`]. Caller is `lifecycle::process_pending_op`,
/// which is called between frames so the JVM is quiescent.
pub(crate) fn process_pending_service_op(
    jvm: &mut Jvm,
    op: PendingServiceOp,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    match op {
        PendingServiceOp::Start {
            class_name,
            intent_ref,
        } => process_start(jvm, class_name, intent_ref, heap, handler),
        PendingServiceOp::Stop { class_name } => process_stop(jvm, class_name, heap, handler),
        PendingServiceOp::Bind {
            class_name,
            intent_ref,
            conn_ref,
            owner_activity_ref,
        } => process_bind(
            jvm,
            class_name,
            intent_ref,
            conn_ref,
            owner_activity_ref,
            heap,
            handler,
        ),
        PendingServiceOp::Unbind { conn_ref } => process_unbind(jvm, conn_ref, heap, handler),
    }
}

/// Synchronously promote a Service to foreground — invoked from the native
/// dispatcher when the Service calls `startForeground`. Title/text are
/// already resolved by the caller (no JVM access needed here). Returns
/// `false` if the service isn't in the registry, in which case the
/// notification is dropped.
pub(crate) fn set_foreground(class_name: &str, notif_id: i32, title: &str, text: &str) -> bool {
    let slot = match registry_find(class_name) {
        Some(i) => i,
        None => return false,
    };
    registry()[slot].as_mut().unwrap().foreground_id = Some(notif_id);
    crate::system::notification::notify(notif_id, title, text);
    true
}

/// Synchronously demote — invoked from the native dispatcher.
pub(crate) fn clear_foreground(class_name: &str, remove: bool) {
    if let Some(slot) = registry_find(class_name) {
        let id = registry()[slot].as_ref().unwrap().foreground_id;
        registry()[slot].as_mut().unwrap().foreground_id = None;
        if remove {
            if let Some(id) = id {
                crate::system::notification::cancel(id);
            }
        }
    }
}

/// Last-bind cleanup when an Activity is popped. Walks [`CONNECTIONS`],
/// fires `onServiceDisconnected` on each conn owned by `activity_ref`,
/// decrements bind counts, and runs `onDestroy` for any service that falls
/// to (started=false, bind_count=0).
pub(crate) fn unbind_owned_by(
    activity_ref: u16,
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) {
    // Snapshot indices first — process_unbind mutates CONNECTIONS in place.
    let mut victims: [u16; MAX_CONNECTIONS] = [0; MAX_CONNECTIONS];
    let mut n = 0;
    for e in connections().iter().flatten() {
        if e.owner_activity_ref == activity_ref {
            victims[n] = e.conn_ref;
            n += 1;
        }
    }
    for &conn_ref in &victims[..n] {
        let _ = process_unbind(jvm, conn_ref, heap, handler);
    }
}

/// App-exit teardown: run `onDestroy` on every still-live Service. Called
/// from the activity-stack teardown path in `lifecycle.rs`.
pub(crate) fn destroy_all(
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) {
    // Snapshot to avoid iterator-vs-mutation issues (each onDestroy may
    // observe other entries via static state).
    let mut victims: [Option<(u16, &'static str)>; MAX_SERVICES] = [None; MAX_SERVICES];
    for (i, e) in registry().iter().enumerate() {
        if let Some(s) = e {
            victims[i] = Some((s.obj_ref, s.class_name));
        }
    }
    for v in victims.iter().flatten() {
        let _ = invoke_lifecycle(
            jvm,
            v.1,
            dispatch_sites::SERVICE_ON_DESTROY,
            v.0,
            heap,
            handler,
        );
    }
    // Wipe the registry — apps that re-enter run_application get a fresh slate.
    for e in registry().iter_mut() {
        *e = None;
    }
    for c in connections().iter_mut() {
        *c = None;
    }
}

// ── Op processors ────────────────────────────────────────────────────────

fn ensure_registered(
    class_name: &'static str,
    jvm: &mut Jvm,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> Option<usize> {
    if let Some(i) = registry_find(class_name) {
        return Some(i);
    }
    let obj_ref = crate::lifecycle::instantiate_component(jvm, class_name, heap, handler)?;
    let slot = registry().iter().position(|e| e.is_none())?;
    registry()[slot] = Some(ServiceEntry {
        obj_ref,
        class_name,
        binder_ref: None,
        started: false,
        bind_count: 0,
        foreground_id: None,
        next_start_id: 0,
    });
    if invoke_lifecycle(
        jvm,
        class_name,
        dispatch_sites::SERVICE_ON_CREATE,
        obj_ref,
        heap,
        handler,
    )
    .is_break()
    {
        return None;
    }
    Some(slot)
}

fn process_start(
    jvm: &mut Jvm,
    class_name: &'static str,
    intent_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let slot = match ensure_registered(class_name, jvm, heap, handler) {
        Some(s) => s,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    let (obj_ref, start_id) = {
        let entry = registry()[slot].as_mut().unwrap();
        entry.started = true;
        entry.next_start_id += 1;
        (entry.obj_ref, entry.next_start_id)
    };
    invoke_service_on_start_command(
        jvm, class_name, obj_ref, intent_ref, start_id, heap, handler,
    )
}

fn process_stop(
    jvm: &mut Jvm,
    class_name: &'static str,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let slot = match registry_find(class_name) {
        Some(i) => i,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    {
        let entry = registry()[slot].as_mut().unwrap();
        entry.started = false;
    }
    maybe_destroy(jvm, slot, heap, handler)
}

fn process_bind(
    jvm: &mut Jvm,
    class_name: &'static str,
    intent_ref: u16,
    conn_ref: u16,
    owner_activity_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let slot = match ensure_registered(class_name, jvm, heap, handler) {
        Some(s) => s,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    // First bind triggers onBind; subsequent ones reuse the cached IBinder.
    let (obj_ref, first_bind) = {
        let entry = registry()[slot].as_mut().unwrap();
        let first = entry.bind_count == 0;
        entry.bind_count = entry.bind_count.saturating_add(1);
        (entry.obj_ref, first)
    };
    if first_bind {
        match invoke_service_returning_object(
            jvm,
            class_name,
            obj_ref,
            intent_ref,
            dispatch_sites::SERVICE_ON_BIND,
            heap,
            handler,
        ) {
            Ok(binder) => {
                registry()[slot].as_mut().unwrap().binder_ref = binder;
            }
            Err(crate::lifecycle::LifecycleControl::Break) => {
                return crate::lifecycle::LifecycleControl::Break
            }
            Err(_) => {}
        }
    }
    // Record the connection.
    if let Some(slot) = connections().iter().position(|c| c.is_none()) {
        connections()[slot] = Some(ConnectionEntry {
            conn_ref,
            service_class: class_name,
            owner_activity_ref,
        });
    }
    // Deliver onServiceConnected with the cached IBinder.
    let binder_ref = registry()[slot].as_ref().unwrap().binder_ref;
    match binder_ref {
        Some(br) => invoke_connection_connected(jvm, conn_ref, br, heap, handler),
        None => crate::lifecycle::LifecycleControl::Continue,
    }
}

fn process_unbind(
    jvm: &mut Jvm,
    conn_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let conn_slot = match connection_find(conn_ref) {
        Some(i) => i,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    let class_name = connections()[conn_slot].as_ref().unwrap().service_class;
    connections()[conn_slot] = None;

    if invoke_connection_disconnected(jvm, conn_ref, heap, handler).is_break() {
        return crate::lifecycle::LifecycleControl::Break;
    }

    let svc_slot = match registry_find(class_name) {
        Some(i) => i,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    let last_unbind = {
        let entry = registry()[svc_slot].as_mut().unwrap();
        entry.bind_count = entry.bind_count.saturating_sub(1);
        entry.bind_count == 0
    };
    if last_unbind {
        let (obj_ref, class_name) = {
            let e = registry()[svc_slot].as_ref().unwrap();
            (e.obj_ref, e.class_name)
        };
        // We don't carry an Intent for unbind — pass null. Service.onUnbind
        // returning true (rebind requested) is recorded but rebind is v2.
        let _ = invoke_service_returning_bool(
            jvm,
            class_name,
            obj_ref,
            0,
            dispatch_sites::SERVICE_ON_UNBIND,
            heap,
            handler,
        );
    }
    maybe_destroy(jvm, svc_slot, heap, handler)
}

fn maybe_destroy(
    jvm: &mut Jvm,
    slot: usize,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let (obj_ref, class_name, do_destroy) = {
        let e = registry()[slot].as_ref().unwrap();
        (e.obj_ref, e.class_name, !e.started && e.bind_count == 0)
    };
    if !do_destroy {
        return crate::lifecycle::LifecycleControl::Continue;
    }
    // Cancel any outstanding foreground notification before tearing down.
    let foreground_id = registry()[slot].as_ref().unwrap().foreground_id;
    if let Some(id) = foreground_id {
        crate::system::notification::cancel(id);
    }
    registry()[slot] = None;
    invoke_lifecycle(
        jvm,
        class_name,
        dispatch_sites::SERVICE_ON_DESTROY,
        obj_ref,
        heap,
        handler,
    )
}

// ── Bytecode invocation helpers ─────────────────────────────────────────

fn invoke_lifecycle(
    jvm: &mut Jvm,
    runtime_class: &'static str,
    site_idx: usize,
    obj_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let method = dispatch_method(site_idx);
    // Try the runtime subclass first; fall back to the framework default
    // (no-op) if the subclass doesn't override.
    match jvm.invoke_instance(runtime_class, method, obj_ref, heap, handler) {
        Ok(()) => crate::lifecycle::LifecycleControl::Continue,
        Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
        Err(JvmError::MethodNotFound) => {
            let fallback = dispatch_class(site_idx);
            match jvm.invoke_instance(fallback, method, obj_ref, heap, handler) {
                Ok(()) => crate::lifecycle::LifecycleControl::Continue,
                Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
                Err(_) => crate::lifecycle::LifecycleControl::Continue,
            }
        }
        Err(_) => crate::lifecycle::LifecycleControl::Continue,
    }
}

fn invoke_service_on_start_command(
    jvm: &mut Jvm,
    runtime_class: &'static str,
    obj_ref: u16,
    intent_ref: u16,
    start_id: i32,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let method = dispatch_method(dispatch_sites::SERVICE_ON_START_COMMAND);
    let extra = [intent_ref_value(intent_ref), Value::Int(start_id)];
    match jvm.invoke_instance_with_args_returning(
        runtime_class,
        method,
        obj_ref,
        &extra,
        heap,
        handler,
    ) {
        Ok(_) => crate::lifecycle::LifecycleControl::Continue,
        Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
        Err(JvmError::MethodNotFound) => {
            let fallback = dispatch_class(dispatch_sites::SERVICE_ON_START_COMMAND);
            match jvm.invoke_instance_with_args_returning(
                fallback, method, obj_ref, &extra, heap, handler,
            ) {
                Ok(_) => crate::lifecycle::LifecycleControl::Continue,
                Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
                Err(_) => crate::lifecycle::LifecycleControl::Continue,
            }
        }
        Err(_) => crate::lifecycle::LifecycleControl::Continue,
    }
}

fn invoke_service_returning_object(
    jvm: &mut Jvm,
    runtime_class: &'static str,
    obj_ref: u16,
    intent_ref: u16,
    site_idx: usize,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> Result<Option<u16>, crate::lifecycle::LifecycleControl> {
    let method = dispatch_method(site_idx);
    let extra = [intent_ref_value(intent_ref)];
    let result = jvm.invoke_instance_with_args_returning(
        runtime_class,
        method,
        obj_ref,
        &extra,
        heap,
        handler,
    );
    match result {
        Ok(Some(Value::ObjectRef(r))) => Ok(Some(r)),
        Ok(_) => Ok(None),
        Err(JvmError::Interrupted) => Err(crate::lifecycle::LifecycleControl::Break),
        Err(JvmError::MethodNotFound) => {
            let fallback = dispatch_class(site_idx);
            match jvm.invoke_instance_with_args_returning(
                fallback, method, obj_ref, &extra, heap, handler,
            ) {
                Ok(Some(Value::ObjectRef(r))) => Ok(Some(r)),
                Ok(_) => Ok(None),
                Err(JvmError::Interrupted) => Err(crate::lifecycle::LifecycleControl::Break),
                Err(_) => Ok(None),
            }
        }
        Err(_) => Ok(None),
    }
}

fn invoke_service_returning_bool(
    jvm: &mut Jvm,
    runtime_class: &'static str,
    obj_ref: u16,
    intent_ref: u16,
    site_idx: usize,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> bool {
    let method = dispatch_method(site_idx);
    let extra = [intent_ref_value(intent_ref)];
    let r = jvm.invoke_instance_with_args_returning(
        runtime_class,
        method,
        obj_ref,
        &extra,
        heap,
        handler,
    );
    match r {
        Ok(Some(Value::Int(n))) => n != 0,
        Err(JvmError::MethodNotFound) => {
            let fallback = dispatch_class(site_idx);
            matches!(
                jvm.invoke_instance_with_args_returning(
                    fallback, method, obj_ref, &extra, heap, handler
                ),
                Ok(Some(Value::Int(n))) if n != 0
            )
        }
        _ => false,
    }
}

fn invoke_connection_connected(
    jvm: &mut Jvm,
    conn_ref: u16,
    binder_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let conn_class = match heap.objects.class_name(conn_ref) {
        Some(s) => s,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    let extra = [Value::ObjectRef(binder_ref)];
    match jvm.invoke_instance_with_args(
        conn_class,
        "onServiceConnected",
        conn_ref,
        &extra,
        heap,
        handler,
    ) {
        Ok(()) => crate::lifecycle::LifecycleControl::Continue,
        Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
        Err(_) => crate::lifecycle::LifecycleControl::Continue,
    }
}

fn invoke_connection_disconnected(
    jvm: &mut Jvm,
    conn_ref: u16,
    heap: &mut SharedJvmHeap,
    handler: &mut PicodroidNativeHandler,
) -> crate::lifecycle::LifecycleControl {
    let conn_class = match heap.objects.class_name(conn_ref) {
        Some(s) => s,
        None => return crate::lifecycle::LifecycleControl::Continue,
    };
    match jvm.invoke_instance(conn_class, "onServiceDisconnected", conn_ref, heap, handler) {
        Ok(()) => crate::lifecycle::LifecycleControl::Continue,
        Err(JvmError::Interrupted) => crate::lifecycle::LifecycleControl::Break,
        Err(_) => crate::lifecycle::LifecycleControl::Continue,
    }
}

fn intent_ref_value(intent_ref: u16) -> Value {
    if intent_ref == 0 {
        Value::Null
    } else {
        Value::ObjectRef(intent_ref)
    }
}
