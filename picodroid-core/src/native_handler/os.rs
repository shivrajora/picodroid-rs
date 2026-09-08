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
        // `Context.getPackageName()`: invokevirtual dispatches with the
        // receiver's class (an Activity or Application subclass), so match
        // the method alone; no other served method shares the name.
        (_, m::getPackageName) => {
            let name = crate::packages::running().unwrap_or("");
            Some(match ctx.strings.intern_dyn(name.as_bytes()) {
                Some(idx) => Ok(Some(Value::Reference(idx))),
                None => Err(JvmError::StackOverflow),
            })
        }
        // ── PackageManager queries (multi-app boards) ─────────────────
        // One value per call over the package directory; an index is the
        // package's position, stable while an app runs (single writer).
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeCount) => Some(Ok(Some(Value::Int(
            crate::packages::entries().count() as i32,
        )))),
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeIndexOf) => {
            let index = match ctx.args.first() {
                Some(Value::Reference(idx)) => ctx
                    .strings
                    .resolve(*idx)
                    .and_then(|name| crate::packages::entries().position(|e| e.package() == name)),
                _ => None,
            };
            Some(Ok(Some(Value::Int(index.map_or(-1, |i| i as i32)))))
        }
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativePackageName) => {
            Some(package_string(ctx, |e| e.package()))
        }
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeVersionName) => {
            Some(package_string(ctx, |e| e.version()))
        }
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeLabel) => {
            Some(package_string(ctx, |e| e.label()))
        }
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeVersionCode) => Some(Ok(Some(
            Value::Int(package_at(ctx.args).map_or(0, |e| e.version_code() as i32)),
        ))),
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeIsSystem) => Some(Ok(Some(Value::Int(
            package_at(ctx.args).is_some_and(|e| e.kind == crate::packages::Kind::System) as i32,
        )))),
        #[cfg(has_multi_app)]
        (c::picodroid_content_pm_PackageManager, m::nativeIconHandle) => {
            Some(Ok(Some(Value::Int(
                package_at(ctx.args)
                    .and_then(crate::graphics::assets::register_icon)
                    .unwrap_or(-1),
            ))))
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

/// The directory entry a `PackageManager` native's `int index` names.
#[cfg(has_multi_app)]
fn package_at(args: &[Value]) -> Option<&'static crate::packages::Entry> {
    match args.first() {
        Some(Value::Int(i)) if *i >= 0 => crate::packages::entries().nth(*i as usize),
        _ => None,
    }
}

/// A string field of the indexed entry, interned for Java; empty when the
/// index is stale.
#[cfg(has_multi_app)]
fn package_string(
    ctx: &mut NativeContext<'_>,
    field: fn(&crate::packages::Entry) -> &'static str,
) -> Result<Option<Value>, JvmError> {
    let s = package_at(ctx.args).map_or("", field);
    match ctx.strings.intern_dyn(s.as_bytes()) {
        Some(idx) => Ok(Some(Value::Reference(idx))),
        None => Err(JvmError::StackOverflow),
    }
}
