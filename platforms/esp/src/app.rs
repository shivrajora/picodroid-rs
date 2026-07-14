// SPDX-License-Identifier: GPL-3.0-only
//! ESP JVM bootstrap — Milestone 1.
//!
//! Stripped-down version of the RP app.rs: no FreeRTOS, no system:: widget
//! state management (requires a running LVGL instance, Milestone 4+).
//! Loads framework classes from picodroid-core, optionally loads the APK,
//! and runs the JVM.

use alloc::boxed::Box;
use pico_jvm::apk::Papk;
use pico_jvm::types::{JvmError, Value};
use pico_jvm::{Jvm, NativeContext, NativeMethodHandler, SharedJvmHeap};

use picodroid_core::framework_classes::FRAMEWORK_CLASSES;

// Embedded APK bytes — empty slice when PICODROID_APK_PATH is not set.
include!(concat!(env!("OUT_DIR"), "/apk_data.rs"));

// Active shrink-map version for compat check.
include!(concat!(env!("OUT_DIR"), "/framework_mapping_version.rs"));

// ── Shared heap ──────────────────────────────────────────────────────────────
// Single-core ESP32-S3 in M1: only one task, no cross-core hazard.

struct SharedHeapCell(core::cell::UnsafeCell<SharedJvmHeap>);
// SAFETY: single-core, single-task in M1; no concurrent access.
unsafe impl Sync for SharedHeapCell {}

static SHARED_HEAP: SharedHeapCell =
    SharedHeapCell(core::cell::UnsafeCell::new(SharedJvmHeap::new()));

pub fn shared_heap() -> &'static mut SharedJvmHeap {
    unsafe { &mut *SHARED_HEAP.0.get() }
}

// ── Native handler ────────────────────────────────────────────────────────────
// Routes picodroid.util.Log.{v,d,i,w,e} to esp-println (USB-Serial-JTAG on the
// ESP32-S3, readable via `espflash --monitor`); every level shares one line
// format since esp-println has no severity channels.  Returns None for every
// other native call so the JVM falls through to BuiltinHandler
// (java/lang/String, StringBuilder, Math, etc.).  Platform-specific handlers
// (GPIO, Display, Thread) are added in subsequent milestones.

struct EspNativeHandler;

impl NativeMethodHandler for EspNativeHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        if class_name == "picodroid/util/Log"
            && matches!(method_name, "v" | "d" | "i" | "w" | "e")
        {
            return Some(log_line(ctx));
        }
        None
    }
}

fn log_line(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
    let tag = resolve_string(ctx.args.first().copied().unwrap_or(Value::Null), ctx)?;
    let msg = resolve_string(ctx.args.get(1).copied().unwrap_or(Value::Null), ctx)?;
    #[cfg(target_arch = "xtensa")]
    esp_println::println!("[{}] {}", tag, msg);
    #[cfg(not(target_arch = "xtensa"))]
    {
        let _ = (tag, msg);
    }
    Ok(None)
}

fn resolve_string<'a>(v: Value, ctx: &'a NativeContext<'_>) -> Result<&'a str, JvmError> {
    match v {
        Value::Reference(idx) => ctx.strings.resolve(idx).ok_or(JvmError::InvalidReference),
        _ => Err(JvmError::InvalidReference),
    }
}

// ── JVM entry point ───────────────────────────────────────────────────────────

/// Load framework classes, load the APK (if present), and run the JVM.
/// Called from `hal::esp::boot::start_tasks`.
pub fn run_jvm() {
    let heap = shared_heap();
    heap.reset();

    let mut jvm = Box::new(Jvm::new());
    let mut handler = EspNativeHandler;

    for class_data in FRAMEWORK_CLASSES {
        jvm.load_class(class_data)
            .expect("failed to load framework class");
    }

    if apk_data().is_empty() {
        return;
    }

    let apk = match Papk::parse(apk_data()) {
        Ok(a) => a,
        Err(_) => return,
    };

    // Reject forward-incompatible PAPKs.
    apk.verify_compat(FRAMEWORK_MAP_VERSION).ok();

    if let Ok(classes) = apk.classes() {
        for entry in classes {
            jvm.load_class(entry.data).ok();
        }
    }

    // M1 entry-point dispatch: prefer Application.onCreate (mirrors RP's
    // lifecycle::run_application minus the activity event loop), fall back
    // to a static main when the manifest declares one.
    if let Some(application_class) = apk.application() {
        let static_name: &'static str =
            unsafe { core::mem::transmute::<&str, &'static str>(application_class) };
        let obj_ref = match heap.objects.alloc(static_name) {
            Some(r) => r,
            None => return,
        };
        match jvm.invoke_instance(static_name, "onCreate", obj_ref, heap, &mut handler) {
            Ok(_) | Err(JvmError::Interrupted) => {}
            Err(e) => {
                #[cfg(target_arch = "xtensa")]
                esp_println::println!("[jvm] Application.onCreate error: {:?}", e);
                #[cfg(not(target_arch = "xtensa"))]
                let _ = e;
            }
        }
    } else if let Some(main_class) = apk.main_class() {
        match jvm.invoke_static(main_class, "main", heap, &mut handler) {
            Ok(_) | Err(JvmError::Interrupted) => {}
            Err(e) => {
                #[cfg(target_arch = "xtensa")]
                esp_println::println!("[jvm] main error: {:?}", e);
                #[cfg(not(target_arch = "xtensa"))]
                let _ = e;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::array_heap::ArrayHeap;
    use pico_jvm::heap::StringTable;
    use pico_jvm::object_heap::ObjectHeap;

    fn make_ctx<'a>(
        strings: &'a mut StringTable,
        objects: &'a mut ObjectHeap,
        arrays: &'a mut ArrayHeap,
        args: &'a [Value],
    ) -> NativeContext<'a> {
        NativeContext {
            descriptor: "",
            args,
            strings,
            objects,
            arrays,
            classes: &[],
        }
    }

    /// `dispatch` must return `None` for any class/method outside the small
    /// allow-list — that's the signal to the JVM that BuiltinHandler should
    /// take over (java.lang.String, Math, collections, etc.). Returning
    /// `Some(Err(...))` here would silently break standard library calls.
    #[test]
    fn dispatch_returns_none_for_non_log_calls() {
        let mut h = EspNativeHandler;
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let args: [Value; 0] = [];
        let mut ctx = make_ctx(&mut strings, &mut objects, &mut arrays, &args);
        assert!(h.dispatch("java/lang/String", "length", &mut ctx).is_none());
        assert!(h.dispatch("picodroid/util/Log", "wtf", &mut ctx).is_none());
        assert!(h.dispatch("", "", &mut ctx).is_none());
    }

    /// Log.{v,d,i,w,e} are the methods ESP handles directly. With both args
    /// resolved to interned strings each returns `Ok(None)` (no return value).
    /// On host it's a no-op (xtensa-only `esp_println::println!`), but the
    /// handshake with the JVM still has to succeed.
    #[test]
    fn log_i_with_valid_string_args_returns_ok_none() {
        let mut h = EspNativeHandler;
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let tag = strings.intern(b"TestTag").expect("intern tag");
        let msg = strings.intern(b"hello world").expect("intern msg");
        let args = [Value::Reference(tag), Value::Reference(msg)];
        let mut ctx = make_ctx(&mut strings, &mut objects, &mut arrays, &args);
        for level in ["v", "d", "i", "w", "e"] {
            let result = h.dispatch("picodroid/util/Log", level, &mut ctx);
            assert!(matches!(result, Some(Ok(None))));
        }
    }

    /// Log.i with a non-Reference arg (e.g. Null where a String was expected)
    /// must surface InvalidReference — a panic here would crash the device
    /// instead of letting the JVM throw a NullPointerException to Java.
    #[test]
    fn log_i_with_null_tag_returns_invalid_reference() {
        let mut h = EspNativeHandler;
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let msg = strings.intern(b"msg").unwrap();
        let args = [Value::Null, Value::Reference(msg)];
        let mut ctx = make_ctx(&mut strings, &mut objects, &mut arrays, &args);
        let result = h.dispatch("picodroid/util/Log", "i", &mut ctx);
        assert!(matches!(result, Some(Err(JvmError::InvalidReference))));
    }

    #[test]
    fn log_i_with_int_arg_returns_invalid_reference() {
        let mut h = EspNativeHandler;
        let mut strings = StringTable::new();
        let mut objects = ObjectHeap::new();
        let mut arrays = ArrayHeap::new();
        let tag = strings.intern(b"tag").unwrap();
        let args = [Value::Reference(tag), Value::Int(42)];
        let mut ctx = make_ctx(&mut strings, &mut objects, &mut arrays, &args);
        let result = h.dispatch("picodroid/util/Log", "i", &mut ctx);
        assert!(matches!(result, Some(Err(JvmError::InvalidReference))));
    }

    /// `shared_heap` is a single-instance accessor backed by a SyncUnsafeCell.
    /// Calling it twice must yield the same heap (the JVM bootstrap and any
    /// later native call have to see the same object/string state).
    #[test]
    fn shared_heap_returns_same_instance_each_call() {
        let a = shared_heap() as *const _;
        let b = shared_heap() as *const _;
        assert_eq!(a, b, "shared_heap must alias to the static singleton");
    }
}
