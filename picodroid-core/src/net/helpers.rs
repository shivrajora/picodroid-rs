// SPDX-License-Identifier: GPL-3.0-only
//! Shared helper functions for picodroid.net native methods.

use alloc::format;
use core::ffi::c_void;
use pico_jvm::heap::StringTable;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use crate::hal::types::{NetError, NetErrorKind};

use super::socket_table;
use crate::shrink_names::c;

/// Allocate an exception of `class` carrying `msg` and wrap it as a thrown
/// Java exception. Alloc-by-name with builtin-hierarchy catch matching — the
/// class needs no .class file (the `java/net/*` chains live in the JVM's
/// `builtin_super`), and `Throwable.getMessage()` surfaces the message via
/// the ObjectHeap side table.
pub fn throw_named_exception(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    class: &'static str,
    msg: &str,
) -> JvmError {
    match objects.alloc(class) {
        Some(idx) => {
            if let Some(midx) = strings.intern_dyn(msg.as_bytes()) {
                objects.register_exception_message(idx, midx);
            }
            JvmError::Exception(idx)
        }
        None => JvmError::StackOverflow,
    }
}

/// Allocate a `java/io/IOException` carrying `msg`.
///
/// Use this for genuine I/O failures that fit no more specific `java.net`
/// type; keep `InvalidReference` for malformed native arguments, which are
/// bugs, not I/O conditions.
pub fn throw_io_exception(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    msg: &str,
) -> JvmError {
    throw_named_exception(objects, strings, c::java_io_IOException, msg)
}

/// Which socket operation a [`NetError`] came from — picks the Java
/// exception class and Android-style message in [`throw_net_exception`].
#[derive(Clone, Copy)]
pub enum NetOpCtx<'a> {
    Connect,
    Send,
    Recv,
    Accept,
    Bind,
    /// Hostname being resolved, for the `UnknownHostException` message.
    Dns(&'a str),
}

impl NetOpCtx<'_> {
    fn op_name(&self) -> &'static str {
        match self {
            NetOpCtx::Connect => "connect",
            NetOpCtx::Send => "send",
            NetOpCtx::Recv => "recv",
            NetOpCtx::Accept => "accept",
            NetOpCtx::Bind => "bind",
            NetOpCtx::Dns(_) => "resolve",
        }
    }
}

/// Map a HAL [`NetError`] to the typed `java.net` exception Android apps
/// expect, with Android's message wording. Combinations outside the table
/// fall back to `java/io/IOException` with the op and family code — `raw`
/// is a positive host errno in sim and a negated FreeRTOS+TCP errno on
/// device, diagnostic only.
pub fn throw_net_exception(
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    e: NetError,
    op: NetOpCtx<'_>,
) -> JvmError {
    use NetErrorKind as K;
    let dynamic: alloc::string::String;
    let (class, msg): (&'static str, &str) = match (e.kind, op) {
        (K::Refused, NetOpCtx::Connect) => (c::java_net_ConnectException, "Connection refused"),
        (K::TimedOut, NetOpCtx::Connect) => {
            (c::java_net_SocketTimeoutException, "connect timed out")
        }
        (K::TimedOut, NetOpCtx::Recv) => (c::java_net_SocketTimeoutException, "Read timed out"),
        (K::TimedOut, NetOpCtx::Accept) => (c::java_net_SocketTimeoutException, "Accept timed out"),
        (K::Unreachable, NetOpCtx::Connect) => {
            (c::java_net_NoRouteToHostException, "Host unreachable")
        }
        (K::AddrInUse, NetOpCtx::Bind) => (c::java_net_BindException, "Address already in use"),
        (K::HostLookup, NetOpCtx::Dns(host)) => {
            dynamic = format!("Unable to resolve host \"{host}\"");
            (c::java_net_UnknownHostException, dynamic.as_str())
        }
        (K::Closed, _) => (c::java_net_SocketException, "Connection reset"),
        // Any other bind failure stays a SocketException — `DatagramSocket(int)`
        // declares `throws SocketException` (as java.net does), so the thrown
        // type must never be a plain IOException the declaration can't cover.
        (_, NetOpCtx::Bind) => {
            dynamic = format!("bind failed (err {})", e.raw);
            (c::java_net_SocketException, dynamic.as_str())
        }
        _ => {
            dynamic = format!("{} failed (err {})", op.op_name(), e.raw);
            (c::java_io_IOException, dynamic.as_str())
        }
    };
    throw_named_exception(objects, strings, class, msg)
}

/// The `SocketException` for operations on a socket whose handle no longer
/// resolves — closed (or never opened). Matches `java.net`'s wording.
pub fn throw_socket_closed(objects: &mut ObjectHeap, strings: &mut StringTable) -> JvmError {
    throw_named_exception(
        objects,
        strings,
        c::java_net_SocketException,
        "Socket is closed",
    )
}

/// Extract `this` object index from args[0].
pub fn extract_obj_idx(args: &[Value]) -> Result<u16, JvmError> {
    match args.first() {
        Some(Value::ObjectRef(idx)) => Ok(*idx),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Read the socket handle field from `this` and look up the raw pointer.
///
/// A malformed argument shape stays `InvalidReference` (a JVM-level bug);
/// a handle that no longer resolves is an app-visible state — the socket
/// was closed — and throws a catchable `SocketException`.
pub fn extract_socket_ptr(
    args: &[Value],
    objects: &mut ObjectHeap,
    strings: &mut StringTable,
    handle_field: usize,
) -> Result<*mut c_void, JvmError> {
    let idx = extract_obj_idx(args)?;
    let handle = match objects.get_field(idx, handle_field) {
        Some(Value::Int(h)) => h,
        _ => return Err(JvmError::InvalidReference),
    };
    let ptr = socket_table::lookup(handle);
    if ptr.is_null() {
        return Err(throw_socket_closed(objects, strings));
    }
    Ok(ptr)
}

/// Read the socket handle (i32) from `this` object's field.
pub fn extract_handle(args: &[Value], objects: &ObjectHeap, field: usize) -> Result<i32, JvmError> {
    let idx = extract_obj_idx(args)?;
    match objects.get_field(idx, field) {
        Some(Value::Int(h)) => Ok(h),
        _ => Err(JvmError::InvalidReference),
    }
}
