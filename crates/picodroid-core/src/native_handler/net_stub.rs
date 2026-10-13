// SPDX-License-Identifier: GPL-3.0-only
//! Stub dispatcher for `picodroid/net/*` native methods on boards that
//! lack networking (`has_network` cfg is off). Status queries return
//! safe defaults (disconnected, 0.0.0.0). Any call that would actually
//! touch the network throws `UnsupportedOperationException`, which apps
//! can avoid by feature-checking via
//! `PackageManager.hasSystemFeature(FEATURE_WIFI)` first.
use crate::shrink_names::{c, ch, mh};
use pico_jvm::NativeKey;

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

/// Every `picodroid/net/*` class, spelled as loaded — a prefix test cannot
/// see through `--shrink`.
const NET_CLASSES: &[u32] = &[
    ch::picodroid_net_ConnectivityManager,
    ch::picodroid_net_ConnectivityManager_NetworkCallback,
    ch::picodroid_net_DatagramPacket,
    ch::picodroid_net_DatagramSocket,
    ch::picodroid_net_HttpInputStream,
    ch::picodroid_net_HttpOutputStream,
    ch::picodroid_net_HttpURLConnection,
    ch::picodroid_net_InetAddress,
    ch::picodroid_net_Network,
    ch::picodroid_net_NetworkCapabilities,
    ch::picodroid_net_NetworkInfo,
    ch::picodroid_net_NetworkRequest,
    ch::picodroid_net_NetworkRequest_Builder,
    ch::picodroid_net_ServerSocket,
    ch::picodroid_net_Socket,
    ch::picodroid_net_URL,
    ch::picodroid_net_wifi_WifiManager,
];

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if !NET_CLASSES.contains(&key.class) {
        return None;
    }

    match (key.class, key.method) {
        // Status queries must remain callable so feature-unaware apps can
        // probe and fall back gracefully.
        (ch::picodroid_net_NetworkInfo, mh::isConnected) => Some(Ok(Some(Value::Int(0)))),
        (ch::picodroid_net_NetworkInfo, mh::getIpAddress) => Some(Ok(Some(Value::Int(0)))),
        // ConnectivityManager.TYPE_NONE
        (ch::picodroid_net_NetworkInfo, mh::getType) => Some(Ok(Some(Value::Int(-1)))),
        // WifiManager: no networks, nothing saved, every request refused.
        (ch::picodroid_net_wifi_WifiManager, _) => super::wifi_stub::dispatch(key.method, ctx),

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
