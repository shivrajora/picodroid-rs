// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.net.wifi.WifiManager` on a board without a WiFi link: every
//! query answers "none" — no networks, nothing saved, station down — and
//! every request answers false, so a feature-unaware app degrades instead
//! of throwing. `PackageManager.hasSystemFeature(FEATURE_WIFI)` is the
//! check an app should make first. Shared by the no-network stub and by a
//! network board whose link is not WiFi.

use crate::shrink_names::m;
use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    Some(match method_name {
        m::nativeAvailable
        | m::nativeStartScan
        | m::nativeSave
        | m::nativeForget
        | m::nativeReconnect
        | m::nativeDisconnect
        | m::nativeScanGeneration
        | m::nativeScanCount
        | m::nativeScanRssi
        | m::nativeScanChannel
        | m::nativeScanSecurity
        | m::nativeStatus
        | m::nativeSavedSource => Ok(Some(Value::Int(0))),
        m::nativeScanSsid | m::nativeScanBssid | m::nativeCurrentSsid | m::nativeSavedSsid => {
            match ctx.strings.intern_dyn(b"") {
                Some(idx) => Ok(Some(Value::Reference(idx))),
                None => Err(JvmError::StackOverflow),
            }
        }
        _ => return None,
    })
}
