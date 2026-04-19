//! Stub dispatcher for `picodroid/net/*` native methods on boards that
//! lack networking (`has_network` cfg is off). Status queries return
//! safe defaults (disconnected, 0.0.0.0). Any call that would actually
//! touch the network throws `UnsupportedOperationException`, which apps
//! can avoid by feature-checking via
//! `PackageManager.hasSystemFeature(FEATURE_WIFI)` first.

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    if !class_name.starts_with("picodroid/net/") {
        return None;
    }

    match (class_name, method_name) {
        // Status queries must remain callable so feature-unaware apps can
        // probe and fall back gracefully.
        ("picodroid/net/NetworkInfo", "isConnected") => Some(Ok(Some(Value::Int(0)))),
        ("picodroid/net/NetworkInfo", "getIpAddress") => Some(Ok(Some(Value::Int(0)))),

        // Everything else would need a live stack — surface a clean exception.
        _ => Some(Err(unsupported(ctx))),
    }
}

fn unsupported(ctx: &mut NativeContext<'_>) -> JvmError {
    match ctx.objects.alloc("java/lang/UnsupportedOperationException") {
        Some(idx) => JvmError::Exception(idx),
        None => JvmError::StackOverflow,
    }
}
