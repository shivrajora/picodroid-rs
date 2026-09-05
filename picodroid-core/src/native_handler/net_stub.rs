// SPDX-License-Identifier: GPL-3.0-only
//! Stub dispatcher for `picodroid/net/*` native methods on boards that
//! lack networking (`has_network` cfg is off). Status queries return
//! safe defaults (disconnected, 0.0.0.0). Any call that would actually
//! touch the network throws `UnsupportedOperationException`, which apps
//! can avoid by feature-checking via
//! `PackageManager.hasSystemFeature(FEATURE_WIFI)` first.

use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

/// Every `picodroid/net/*` class, spelled as loaded — a prefix test cannot
/// see through `--shrink`.
const NET_CLASSES: &[&str] = &[
    c::picodroid_net_DatagramPacket,
    c::picodroid_net_DatagramSocket,
    c::picodroid_net_HttpInputStream,
    c::picodroid_net_HttpOutputStream,
    c::picodroid_net_HttpURLConnection,
    c::picodroid_net_InetAddress,
    c::picodroid_net_NetworkInfo,
    c::picodroid_net_ServerSocket,
    c::picodroid_net_Socket,
    c::picodroid_net_URL,
];

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if !NET_CLASSES.contains(&class_name) {
        return None;
    }

    match (class_name, method_name) {
        // Status queries must remain callable so feature-unaware apps can
        // probe and fall back gracefully.
        (c::picodroid_net_NetworkInfo, m::isConnected) => Some(Ok(Some(Value::Int(0)))),
        (c::picodroid_net_NetworkInfo, m::getIpAddress) => Some(Ok(Some(Value::Int(0)))),
        // ConnectivityManager.TYPE_NONE
        (c::picodroid_net_NetworkInfo, m::getType) => Some(Ok(Some(Value::Int(-1)))),

        // Everything else would need a live stack — surface a clean exception.
        _ => Some(Err(unsupported(ctx))),
    }
}

fn unsupported(ctx: &mut NativeContext<'_>) -> JvmError {
    match ctx
        .objects
        .alloc(c::java_lang_UnsupportedOperationException)
    {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}
