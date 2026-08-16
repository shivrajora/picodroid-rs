// SPDX-License-Identifier: GPL-3.0-only
//! Booting an app: the shared JVM heap, the class loaders, and [`run_app`].
//!
//! Split out of the RP crate's `app.rs`. What stayed behind is only what is
//! genuinely per-family: the APK blob accessor its `build.rs` generates, and
//! the post-run idle loop. Everything here is the same on any MCU.
//!
//! The ordering inside [`run_app`] is a contract, not a style choice — root
//! registration before the first class load, heap reset before pre-reserve,
//! per-subsystem state reset before either. It is moved verbatim for that
//! reason; see the comments at each step.

use alloc::boxed::Box;
use papk_format::Papk;
use pico_jvm::types::JvmError;
use pico_jvm::{Jvm, SharedJvmHeap};

use crate::framework_classes::FRAMEWORK_CLASSES;
use crate::host;

/// Re-exported at the path it has always had. The constant itself now lives
/// in [`crate::framework_map`], which is compiled unconditionally — this
/// module is not, and the install path needs the version in test builds.
pub use crate::framework_map::FRAMEWORK_MAP_VERSION;

/// Boot-time heap pre-reservation sizes from board.toml `[jvm]` (PEM-3).
mod prereserve_config {
    include!(concat!(env!("OUT_DIR"), "/jvm_prereserve_config.rs"));
}

// ── Shared heap ──────────────────────────────────────────────────────────────
//
// All JVM threads share one heap (objects, arrays, strings), matching the
// standard Java memory model. Only one JVM task runs at a time — the core is
// single-core (the simulator models the same), JVM work is pinned to it, and
// `configUSE_TIME_SLICING = 0` guarantees a running JVM task keeps the CPU
// until it blocks (sleep / socket / queue), so switches between JVM tasks
// land only at yield points, never mid-heap-mutation — so allocation needs
// no global lock. Parked-mid-execute tasks' frames stay visible to GC via
// the frame registry in `GcState` (see pico_jvm::gc).

// SAFETY: upheld by the single-JVM-task invariant above, which is part of
// the RTOS contract every platform implements.
struct SharedHeapCell(core::cell::UnsafeCell<SharedJvmHeap>);
unsafe impl Sync for SharedHeapCell {}

static SHARED_HEAP: SharedHeapCell =
    SharedHeapCell(core::cell::UnsafeCell::new(SharedJvmHeap::new()));

/// The process-wide shared JVM heap.
///
/// # Safety
/// Call only from JVM task context, never an ISR. Allocation inside
/// ObjectHeap/ArrayHeap is short enough that no task switch can land
/// mid-alloc at a cooperative yield point (sleep / UART read).
pub fn shared_heap() -> &'static mut SharedJvmHeap {
    unsafe { &mut *SHARED_HEAP.0.get() }
}

// ── Class loader registration ────────────────────────────────────────────────
//
// `Thread.start()` spawns a task that builds a fresh Jvm and must load every
// application class before invoking Runnable.run(). A single registered
// loader reads straight from the active APK, so every app works without
// special-casing.

type ClassLoaderFn = fn(&mut Jvm) -> Result<(), JvmError>;

// SAFETY: single JVM task (see above); registered once from the bootstrap
// task before any Thread.start() dispatch can occur.
struct ClassLoaderCell(core::cell::UnsafeCell<Option<ClassLoaderFn>>);
unsafe impl Sync for ClassLoaderCell {}

static CLASS_LOADER: ClassLoaderCell = ClassLoaderCell(core::cell::UnsafeCell::new(None));

pub fn register_class_loader(f: ClassLoaderFn) {
    unsafe { *CLASS_LOADER.0.get() = Some(f) }
}

pub fn load_classes(jvm: &mut Jvm) -> Result<(), JvmError> {
    unsafe {
        match *CLASS_LOADER.0.get() {
            Some(f) => f(jvm),
            None => Ok(()),
        }
    }
}

// ── Framework class loading ──────────────────────────────────────────────────

/// Load every picodroid framework class into `jvm`.
///
/// Framework classes (`picodroid.*`) are compiled by `build.rs` and embedded
/// in firmware Flash — platform code, not app code, mirroring Android's boot
/// classpath model.
fn load_framework_classes(jvm: &mut Jvm) -> Result<(), JvmError> {
    for class_data in FRAMEWORK_CLASSES {
        jvm.load_class(class_data)?;
    }
    Ok(())
}

// ── Active APK pointer ───────────────────────────────────────────────────────
//
// `run_app` publishes the current APK here so `load_classes_from_apk` — a
// bare fn, not a closure — serves both the built-in APK and a dynamically
// installed PAPK for Thread.start() tasks, without closure captures.
//
// SAFETY: single JVM task; one writer (the bootstrap task) and readers that
// only run after it has published.
struct ActiveApkCell(core::cell::UnsafeCell<(*const u8, usize)>);
unsafe impl Sync for ActiveApkCell {}

static ACTIVE_APK: ActiveApkCell =
    ActiveApkCell(core::cell::UnsafeCell::new((core::ptr::null(), 0)));

// ── APK-driven class loading ─────────────────────────────────────────────────

/// Load every class from the active APK into `jvm`.
///
/// Used at startup and by `Thread.start()` when it spawns a fresh `Jvm`,
/// reading the pointer published by [`run_app`] so dynamically installed
/// PAPKs load correctly in child threads.
fn load_classes_from_apk(jvm: &mut Jvm) -> Result<(), JvmError> {
    let apk_data: &[u8] = unsafe {
        let (ptr, len) = *ACTIVE_APK.0.get();
        assert!(!ptr.is_null(), "ACTIVE_APK not set");
        core::slice::from_raw_parts(ptr, len)
    };
    let apk = Papk::parse(apk_data).map_err(|_| JvmError::InvalidBytecode)?;
    for entry in apk.classes().map_err(|_| JvmError::InvalidBytecode)? {
        jvm.load_class(entry.data)?;
    }
    Ok(())
}

/// Framework classes then app classes — for `Thread.start()` tasks that need
/// a fully populated fresh `Jvm`.
fn load_all_classes(jvm: &mut Jvm) -> Result<(), JvmError> {
    load_framework_classes(jvm)?;
    load_classes_from_apk(jvm)
}

// ── run_app ──────────────────────────────────────────────────────────────────

/// Run the JVM against `apk_data`.
///
/// Resets the shared heap (clearing the previous app's state), loads
/// framework and app classes, then launches an Activity if the manifest
/// declares one, else `<main_class>.main()`. Returns when execution finishes
/// or `pdb install` interrupts it; `JvmError::Interrupted` is a clean exit
/// signal, not an error.
pub fn run_app(apk_data: &[u8]) {
    // Publish the APK pointer so load_classes_from_apk (a bare fn) picks it
    // up even when called from Thread.start()-spawned child tasks.
    unsafe { *ACTIVE_APK.0.get() = (apk_data.as_ptr(), apk_data.len()) };

    // Register GC root providers before the first class load — hence before
    // any GC can run. Objects held only by native code (Views in listener
    // maps, bound Services, the Display singleton) are invisible to the
    // collector until this runs, and a GC before it would sweep them while
    // live. Idempotent, so a PDB app reload re-entering here is fine.
    crate::gc_root_registration::register_all();
    // Then any the platform owns. This crate registers its own first so a
    // family that gets its hook wrong loses only its own providers, never
    // the framework's.
    host::register_gc_roots();

    // Clear the previous app's heap state, then claim the board-tuned
    // steady-state storage while the heap is young and contiguous (PEM-3
    // pre-reservation; zeros are no-ops).
    shared_heap().reset();
    shared_heap().prereserve(
        prereserve_config::PRERESERVE_OBJ_CHUNKS,
        prereserve_config::PRERESERVE_FIELDS_VALUES,
        prereserve_config::PRERESERVE_ARR_CHUNKS,
        prereserve_config::PRERESERVE_ARENA_VALUES,
        prereserve_config::PRERESERVE_STR_CHUNKS,
    );
    crate::lifecycle::reset_dispatch_event_state();
    crate::graphics::widgets::reset_button_state();
    crate::graphics::widgets::reset_progress_bar_state();
    crate::graphics::widgets::reset_toggle_button_state();
    crate::graphics::widgets::reset_switch_state();
    crate::graphics::widgets::reset_seek_bar_state();
    crate::graphics::widgets::reset_check_box_state();
    crate::graphics::widgets::reset_radio_button_state();
    crate::graphics::widgets::reset_spinner_state();
    crate::graphics::widgets::reset_toast_state();
    crate::graphics::widgets::reset_snackbar_state();
    crate::graphics::widgets::reset_alert_dialog_state();
    crate::graphics::widgets::reset_date_picker_state();
    crate::graphics::widgets::reset_time_picker_state();
    crate::graphics::widgets::reset_swipe_refresh_state();
    crate::graphics::widgets::reset_animation_state();
    crate::graphics::widgets::reset_keyboard_state();
    crate::graphics::widgets::reset_edit_text_state();
    crate::graphics::widgets::reset_list_view_state();
    crate::graphics::widgets::reset_number_picker_state();
    crate::graphics::view::reset_key_listener_state();
    crate::graphics::view::reset_touch_listener_state();
    crate::graphics::view::reset_swipe_listener_state();
    crate::graphics::view::reset_focus_change_listener_state();
    crate::graphics::lvgl::events::reset_key_event_queue();
    crate::graphics::lvgl::events::reset_edit_mode();
    crate::graphics::lvgl::events::reset_activity_groups();
    crate::graphics::lvgl::handle_table::reset();
    crate::graphics::assets::clear();
    // Sensor registrations hold u16 heap refs from the previous app; they
    // must not survive into the reset heap (visit_gc_roots would walk them).
    // Also publishes all-disabled demand so the sampler parks between apps.
    crate::hardware::sensors::reset();
    #[cfg(not(feature = "sim"))]
    crate::monitor_store::clear();

    // Parse the APK once up front so the class table can be pre-sized to the
    // exact framework + app class count. Avoids 7 Vec doubling reallocations
    // (and their transient double-buffering) during framework registration.
    // These error arms stay hand-paired rather than using `pd_log`: the papk
    // error type implements `Debug`/`Display` but not `defmt::Format`, which
    // is why the device arm wraps it in `Debug2Format`. A `pd_error!` would
    // not compile on that arm.
    let apk_for_count = match Papk::parse(apk_data) {
        Ok(a) => a,
        Err(e) => {
            #[cfg(not(feature = "sim"))]
            defmt::error!(
                "PAPK parse failed during pre-sizing: {}",
                defmt::Debug2Format(&e)
            );
            #[cfg(feature = "sim")]
            eprintln!("[sim] PAPK parse failed during pre-sizing: {:?}", e);
            return;
        }
    };
    let apk_class_count = apk_for_count.classes().map(|it| it.count()).unwrap_or(0);
    let mut jvm = Box::new(Jvm::with_capacity(
        FRAMEWORK_CLASSES.len() + apk_class_count,
    ));
    let heap = shared_heap();
    let mut handler = crate::native_handler::PicodroidNativeHandler::new();
    // Runtime heap-diagnostic flags (PICODROID_MEMDIAG_HISTO / _OFFENSIVE) —
    // applied before any class loads so the whole run is covered.
    #[cfg(all(feature = "sim", feature = "mem-diag"))]
    crate::mem_diag::apply_heap_flags(heap);
    host::heap_checkpoint("post-jvm-new");

    // Register the combined loader so Thread.start() spawned tasks load both
    // framework and app classes into their fresh Jvm instances.
    register_class_loader(load_all_classes);

    // Platform (framework) classes first, then app classes from the APK.
    load_framework_classes(&mut jvm).unwrap();
    host::heap_checkpoint("post-framework-load");
    load_classes_from_apk(&mut jvm).unwrap();
    host::heap_checkpoint("post-app-load");

    // Determine the entry point from the APK manifest. The pre-sizing parse
    // above already returned on error, so a second failure here is a
    // fresh-bytes anomaly; bail cleanly rather than panic on-device.
    let apk = match Papk::parse(apk_data) {
        Ok(a) => a,
        Err(e) => {
            #[cfg(not(feature = "sim"))]
            defmt::error!(
                "PAPK parse failed during entry-point lookup: {}",
                defmt::Debug2Format(&e)
            );
            #[cfg(feature = "sim")]
            eprintln!("[sim] PAPK parse failed during entry-point lookup: {:?}", e);
            return;
        }
    };

    // Reject PAPKs built against a newer shrink-map release than this build
    // knows about. Maps are append-only per release, so older PAPKs are
    // always accepted; only a forward-incompatible one fails here. Log and
    // return rather than panic: a bad APK should not take down the boot path
    // before pdb can recover.
    if let Err(e) = apk.verify_compat(FRAMEWORK_MAP_VERSION) {
        #[cfg(not(feature = "sim"))]
        defmt::error!(
            "PAPK framework-map-version incompatible with firmware (firmware = {}): {}",
            FRAMEWORK_MAP_VERSION,
            defmt::Debug2Format(&e)
        );
        #[cfg(feature = "sim")]
        eprintln!(
            "[sim] PAPK framework-map-version incompatible with firmware (firmware = {}): {:?}",
            FRAMEWORK_MAP_VERSION, e
        );
        return;
    }

    // Build the bundled-asset registry from this papk's ASSETS section so
    // `ImageView.setImageSource("name.png")` resolves at runtime. Empty for
    // legacy v1.0 papks and any v1.1 papk built without `--assets-dir`.
    crate::graphics::assets::init_from_papk(&apk);

    #[cfg(feature = "sim")]
    let start = std::time::Instant::now();

    if let Some(application_class) = apk.application() {
        // The APK data is &'static [u8] (Flash-backed), so the parsed class
        // name string is also 'static. Transmute the lifetime so alloc() can
        // store it in the object heap.
        let static_name: &'static str =
            unsafe { core::mem::transmute::<&str, &'static str>(application_class) };
        crate::lifecycle::run_application(&mut jvm, static_name, heap, &mut handler);
    } else if let Some(activity_class) = apk.activity() {
        let static_name: &'static str =
            unsafe { core::mem::transmute::<&str, &'static str>(activity_class) };
        let obj_ref = heap
            .objects
            .alloc(static_name)
            .expect("OOM allocating Activity");
        crate::lifecycle::run_activity(&mut jvm, static_name, obj_ref, None, heap, &mut handler);
    } else {
        let main_class = apk
            .main_class()
            .expect("APK manifest is missing 'main-class'");

        // Interrupted is a clean cooperative stop — not a real error.
        match jvm.invoke_static(main_class, "main", heap, &mut handler) {
            Ok(_) | Err(JvmError::Interrupted) => {}
            #[cfg(not(feature = "sim"))]
            Err(e) => defmt::error!("JVM error: {}", defmt::Display2Format(&e)),
            #[cfg(feature = "sim")]
            Err(e) => eprintln!("[jvm] error: {}", e),
        }
    }

    // Parity checkpoint (docs/parity-audit.md P1): deterministic work
    // counters in an identical, greppable format on both sim (stdout) and
    // device (defmt/RTT). Cross-environment checks assert these EQUAL.
    #[cfg(feature = "parity-metrics")]
    {
        let (_, gcs, _) = handler.gc_stats();
        let insns = pico_jvm::parity::insns();
        let allocs = pico_jvm::parity::allocs();
        let (bands, fbytes) = crate::graphics::lvgl::lifecycle::flush_stats::snapshot();
        #[cfg(not(feature = "sim"))]
        defmt::info!(
            "parity: insns={=usize} allocs={=usize} gcs={=u32} bands={=usize} fbytes={=usize}",
            insns,
            allocs,
            gcs,
            bands,
            fbytes
        );
        #[cfg(feature = "sim")]
        println!(
            "parity: insns={} allocs={} gcs={} bands={} fbytes={}",
            insns, allocs, gcs, bands, fbytes
        );
    }

    #[cfg(feature = "sim")]
    {
        host::heap_checkpoint("post-onCreate");
        // Final memory snapshot: gives non-Activity apps (plain main-class
        // runs with no tick loop) one [memmon] line, and Activity soaks a
        // closing figure to grep.
        #[cfg(feature = "mem-diag")]
        crate::mem_diag::snapshot(heap, &handler);
        let (gc_ns, gc_count, gc_freed) = handler.gc_stats();
        let (parsed, total) = jvm.count_parsed();
        println!(
            "[sim] JVM wall-clock: {} ms, gc: {} collections, {} freed, {} us, \
             lazy-load: {}/{} classes parsed",
            start.elapsed().as_millis(),
            gc_count,
            gc_freed,
            gc_ns / 1000,
            parsed,
            total,
        );
    }
}
