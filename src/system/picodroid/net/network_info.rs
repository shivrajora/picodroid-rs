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
