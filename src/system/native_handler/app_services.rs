// SPDX-License-Identifier: GPL-3.0-only
//! Native bridge for `Context.{start,stop,bind,unbind}Service`,
//! `Service.{stopSelf,startForeground,stopForeground}`, and
//! `NotificationManager.{notify,cancel}`.
//!
//! Each entry just packages the call site's arguments into a
//! [`PendingServiceOp`] and queues it on the handler. The actual lifecycle
//! callbacks (onCreate, onStartCommand, onBind, ...) run between frames in
//! [`crate::service_lifecycle`].

use pico_jvm::native::NativeContext;
use pico_jvm::types::{JvmError, Value};

use super::PendingServiceOp;
use super::{PendingOp, PicodroidNativeHandler};

/// Try to handle a `(class, method)` call. Returns `Some(...)` only when
/// this dispatcher recognises the method.
pub(super) fn dispatch(
    handler: &mut PicodroidNativeHandler,
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    // Context.startService / stopService / bindService / unbindService —
    // matched on method name regardless of receiver class because the JVM
    // dispatches with the runtime subclass (Activity/Application/...).
    match method_name {
        "startService" => return Some(handle_start_service(handler, ctx)),
        "stopService" => return Some(handle_stop_service(handler, ctx)),
        "bindService" => return Some(handle_bind_service(handler, ctx)),
        "unbindService" => return Some(handle_unbind_service(handler, ctx)),
        _ => {}
    }
    // Service-specific methods — these arrive with the runtime Service
    // subclass as `class_name`, so we don't gate on it.
    let _ = class_name;
    match method_name {
        "stopSelf" => Some(handle_stop_self(handler, ctx)),
        "startForeground" => Some(handle_start_foreground(handler, ctx)),
        "stopForeground" => Some(handle_stop_foreground(handler, ctx)),
        _ => None,
    }
}

fn intent_target_class(ctx: &NativeContext<'_>, intent_ref: u16) -> Option<&'static str> {
    let name_idx = match ctx.objects.get_field(intent_ref, 0) {
        Some(Value::Reference(idx)) => idx,
        _ => return None,
    };
    let s = ctx.strings.resolve(name_idx)?;
    // SAFETY: the StringTable entry is Flash-backed for the lifetime of
    // the JVM (see jvm/src/heap.rs).
    Some(unsafe { core::mem::transmute::<&str, &'static str>(s) })
}

fn obj_class_name(ctx: &NativeContext<'_>, obj_ref: u16) -> Option<&'static str> {
    let s = ctx.objects.class_name(obj_ref)?;
    Some(unsafe { core::mem::transmute::<&str, &'static str>(s) })
}

fn handle_start_service(
    handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    if let Some(Value::ObjectRef(intent_ref)) = ctx.args.get(1) {
        if let Some(class_name) = intent_target_class(ctx, *intent_ref) {
            handler.enqueue_op(PendingOp::Service(PendingServiceOp::Start {
                class_name,
                intent_ref: *intent_ref,
            }));
        }
    }
    Ok(None)
}

fn handle_stop_service(
    handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    if let Some(Value::ObjectRef(intent_ref)) = ctx.args.get(1) {
        if let Some(class_name) = intent_target_class(ctx, *intent_ref) {
            handler.enqueue_op(PendingOp::Service(PendingServiceOp::Stop { class_name }));
        }
    }
    Ok(None)
}

fn handle_bind_service(
    handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(intent_ref)) = ctx.args.get(1) else {
        return Ok(None);
    };
    let Some(Value::ObjectRef(conn_ref)) = ctx.args.get(2) else {
        return Ok(None);
    };
    let Some(class_name) = intent_target_class(ctx, *intent_ref) else {
        return Ok(None);
    };
    let owner = handler.current_activity().map(|(r, _)| r).unwrap_or(0);
    handler.enqueue_op(PendingOp::Service(PendingServiceOp::Bind {
        class_name,
        intent_ref: *intent_ref,
        conn_ref: *conn_ref,
        owner_activity_ref: owner,
    }));
    Ok(None)
}

fn handle_unbind_service(
    handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    if let Some(Value::ObjectRef(conn_ref)) = ctx.args.get(1) {
        handler.enqueue_op(PendingOp::Service(PendingServiceOp::Unbind {
            conn_ref: *conn_ref,
        }));
    }
    Ok(None)
}

fn handle_stop_self(
    handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    if let Some(Value::ObjectRef(this_ref)) = ctx.args.first() {
        if let Some(class_name) = obj_class_name(ctx, *this_ref) {
            handler.enqueue_op(PendingOp::Service(PendingServiceOp::Stop { class_name }));
        }
    }
    Ok(None)
}

/// `Service.startForeground(int id, Notification n)` is processed
/// synchronously: if it were queued like other ops, a `stopSelf` issued on
/// the same frame would destroy the Service before the foreground state
/// landed. The notification fields are read off the heap here — no JVM
/// re-entry needed.
fn handle_start_foreground(
    _handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(this_ref)) = ctx.args.first() else {
        return Ok(None);
    };
    let Some(Value::Int(notif_id)) = ctx.args.get(1) else {
        return Ok(None);
    };
    let Some(Value::ObjectRef(notif_ref)) = ctx.args.get(2) else {
        return Ok(None);
    };
    let Some(class_name) = obj_class_name(ctx, *this_ref) else {
        return Ok(None);
    };
    let title = read_string_field(ctx, *notif_ref, 0).unwrap_or("");
    let text = read_string_field(ctx, *notif_ref, 1).unwrap_or("");
    crate::service_lifecycle::set_foreground(class_name, *notif_id, title, text);
    Ok(None)
}

fn handle_stop_foreground(
    _handler: &mut PicodroidNativeHandler,
    ctx: &NativeContext<'_>,
) -> Result<Option<Value>, JvmError> {
    let Some(Value::ObjectRef(this_ref)) = ctx.args.first() else {
        return Ok(None);
    };
    let Some(Value::Int(remove)) = ctx.args.get(1) else {
        return Ok(None);
    };
    let Some(class_name) = obj_class_name(ctx, *this_ref) else {
        return Ok(None);
    };
    crate::service_lifecycle::clear_foreground(class_name, *remove != 0);
    Ok(None)
}

fn read_string_field<'a>(ctx: &'a NativeContext<'_>, obj_ref: u16, slot: usize) -> Option<&'a str> {
    match ctx.objects.get_field(obj_ref, slot) {
        Some(Value::Reference(idx)) => ctx.strings.resolve(idx),
        _ => None,
    }
}
