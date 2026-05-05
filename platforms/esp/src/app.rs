// SPDX-License-Identifier: GPL-3.0-only
//! ESP JVM bootstrap — Milestone 1.
//!
//! Stripped-down version of the RP app.rs: no FreeRTOS, no system:: widget
//! state management (requires a running LVGL instance, Milestone 4+).
//! Loads framework classes from picodroid-core, optionally loads the APK,
//! and runs the JVM.

use alloc::boxed::Box;
use pico_jvm::apk::Papk;
use pico_jvm::types::JvmError;
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

// ── Stub native handler ───────────────────────────────────────────────────────
// Returns None for every call so the JVM falls through to BuiltinHandler
// (java/lang/String, StringBuilder, Math, etc.).  Platform-specific handlers
// (GPIO, Display, Thread) are added in subsequent milestones.

struct StubHandler;

impl NativeMethodHandler for StubHandler {
    fn dispatch(
        &mut self,
        _class_name: &str,
        _method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<pico_jvm::types::Value>, JvmError>> {
        None
    }
}

// ── JVM entry point ───────────────────────────────────────────────────────────

/// Load framework classes, load the APK (if present), and run the JVM.
/// Called from `hal::esp::boot::start_tasks`.
pub fn run_jvm() {
    let heap = shared_heap();
    heap.reset();

    let mut jvm = Box::new(Jvm::new());
    let mut handler = StubHandler;

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

    if let Some(main_class) = apk.main_class() {
        match jvm.invoke_static(main_class, "main", heap, &mut handler) {
            Ok(_) | Err(JvmError::Interrupted) => {}
            Err(_) => {}
        }
    }
}
