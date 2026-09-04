// SPDX-License-Identifier: GPL-3.0-only
//! Native implementations for picodroid.net.NetworkInfo.

use pico_jvm::types::{JvmError, Value};

pub fn is_connected_native() -> Result<Option<Value>, JvmError> {
    let up = crate::hal::net::is_network_up();
    Ok(Some(Value::Int(if up { 1 } else { 0 })))
}

pub fn get_ip_address_native() -> Result<Option<Value>, JvmError> {
    let ip = crate::hal::net::get_ip_address();
    Ok(Some(Value::Int(ip as i32)))
}

/// The board's link kind as a `ConnectivityManager.TYPE_*` value. A build
/// fact: `board_cfg.rs` emits `network_link_<kind>` from `network_type`.
pub fn get_type_native() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(link_type())))
}

/// Android's `TYPE_WIFI` / `TYPE_ETHERNET`, or `TYPE_NONE` (-1).
pub fn link_type() -> i32 {
    if cfg!(network_link_wifi) {
        1
    } else if cfg!(network_link_ethernet) {
        9
    } else {
        -1
    }
}
