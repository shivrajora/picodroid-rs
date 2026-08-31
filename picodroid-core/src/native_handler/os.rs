// SPDX-License-Identifier: GPL-3.0-only
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        ("picodroid/os/SystemClock", "sleep") => Some(crate::os::system_clock::sleep(ctx.args)),
        ("picodroid/os/SystemClock", "elapsedRealtimeNanos") => {
            Some(crate::os::system_clock::elapsed_realtime_nanos())
        }
        ("picodroid/os/SystemClock", "setCurrentTimeMillis") => {
            Some(crate::os::system_clock::set_current_time_millis(ctx.args))
        }
        // Elapsed-since-boot until SystemClock.setCurrentTimeMillis anchors
        // the epoch (offset stays 0 before that, preserving the historical
        // behaviour for apps that never sync).
        ("java/lang/System", "currentTimeMillis") => {
            let nanos = crate::hal::system_clock::elapsed_realtime_nanos();
            let millis = nanos / 1_000_000 + crate::os::system_clock::wall_offset_ms();
            Some(Ok(Some(Value::Long(millis))))
        }
        ("picodroid/content/pm/PackageManager", "hasSystemFeature") => {
            // args[0] = this, args[1] = feature name String
            let supported = match ctx.args.get(1) {
                Some(Value::Reference(idx)) => match ctx.strings.resolve(*idx) {
                    // FEATURE_WIFI: board has a wireless driver compiled in.
                    Some("picodroid.hardware.wifi") => cfg!(has_network),
                    _ => false,
                },
                _ => false,
            };
            Some(Ok(Some(Value::Int(supported as i32))))
        }
        _ => None,
    }
}
