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
// Routes picodroid.util.Log.i to esp-println (USB-Serial-JTAG on the ESP32-S3,
// readable via `espflash --monitor`).  Returns None for every other native call
// so the JVM falls through to BuiltinHandler (java/lang/String, StringBuilder,
// Math, etc.).  Platform-specific handlers (GPIO, Display, Thread) are added
// in subsequent milestones.

struct EspNativeHandler;

impl NativeMethodHandler for EspNativeHandler {
    fn dispatch(
        &mut self,
        class_name: &str,
        method_name: &str,
        ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        if class_name == "picodroid/util/Log" && method_name == "i" {
            return Some(log_i(ctx));
        }
        None
    }
}

fn log_i(ctx: &mut NativeContext<'_>) -> Result<Option<Value>, JvmError> {
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

    if APK_DATA.is_empty() {
        return;
    }

    let apk = match Papk::parse(APK_DATA) {
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
