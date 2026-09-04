// SPDX-License-Identifier: GPL-3.0-only
use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (class_name, method_name) {
        (c::picodroid_os_SystemClock, m::sleep) => Some(crate::os::system_clock::sleep(ctx.args)),
        (c::picodroid_os_SystemClock, m::elapsedRealtimeNanos) => {
            Some(crate::os::system_clock::elapsed_realtime_nanos())
        }
        (c::picodroid_os_SystemClock, m::setCurrentTimeMillis) => {
            Some(crate::os::system_clock::set_current_time_millis(ctx.args))
        }
        // Elapsed-since-boot until SystemClock.setCurrentTimeMillis anchors
        // the epoch (offset stays 0 before that, preserving the historical
        // behaviour for apps that never sync).
        (c::java_lang_System, m::currentTimeMillis) => {
            let nanos = crate::hal::system_clock::elapsed_realtime_nanos();
            let millis = nanos / 1_000_000 + crate::os::system_clock::wall_offset_ms();
            Some(Ok(Some(Value::Long(millis))))
        }
        (c::picodroid_content_pm_PackageManager, m::hasSystemFeature) => {
            // args[0] = this, args[1] = feature name String
            let supported = match ctx.args.get(1) {
                Some(Value::Reference(idx)) => match ctx.strings.resolve(*idx) {
                    // The link kind, a build fact (board_cfg.rs emits
                    // network_link_<kind> from board.toml's network_type).
                    Some("picodroid.hardware.wifi") => cfg!(network_link_wifi),
                    Some("picodroid.hardware.ethernet") => cfg!(network_link_ethernet),
                    _ => false,
                },
                _ => false,
            };
            Some(Ok(Some(Value::Int(supported as i32))))
        }
        _ => None,
    }
}
