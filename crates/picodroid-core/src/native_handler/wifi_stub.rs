// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.net.wifi.WifiManager` on a board without a WiFi link: every
//! query answers "none" — no networks, nothing saved, station down — and
//! every request answers false, so a feature-unaware app degrades instead
//! of throwing. `PackageManager.hasSystemFeature(FEATURE_WIFI)` is the
//! check an app should make first. Shared by the no-network stub and by a
//! network board whose link is not WiFi.
use crate::shrink_names::mh;

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    method: u32,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    Some(match method {
        mh::nativeAvailable
        | mh::nativeStartScan
        | mh::nativeSave
        | mh::nativeForget
        | mh::nativeReconnect
        | mh::nativeDisconnect
        | mh::nativeScanGeneration
        | mh::nativeScanCount
        | mh::nativeScanRssi
        | mh::nativeScanChannel
        | mh::nativeScanSecurity
        | mh::nativeStatus
        | mh::nativeSavedSource => Ok(Some(Value::Int(0))),
        mh::nativeScanSsid | mh::nativeScanBssid | mh::nativeCurrentSsid | mh::nativeSavedSsid => {
            match ctx.strings.intern_dyn(b"") {
                Some(idx) => Ok(Some(Value::Reference(idx))),
                None => Err(JvmError::StackOverflow),
            }
        }
        _ => return None,
    })
}
