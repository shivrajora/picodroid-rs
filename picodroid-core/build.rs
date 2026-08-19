// SPDX-License-Identifier: GPL-3.0-only
// Suppress dead-code / unused warnings from the imported build_support modules.
// picodroid-core only calls a subset of their functions.
#![allow(dead_code, unused_imports, unused_variables)]
//! picodroid-core build script.
//!
//! Emits `framework_classes.rs` / `framework_unshrink.rs`, plus every
//! board-derived *neutral* artifact (JVM sizing, sensor table, button table,
//! sleep, heap, background pool, handle table, display dimensions) and the
//! board-capability cfgs.
//!
//! The board is discovered by searching `../platforms/*/boards/<name>` for
//! the `board-*` feature the binary crate forwarded, so `cargo build -p
//! picodroid` keeps working with no env vars while this crate stays
//! independently buildable (boardless builds fall back to safe defaults).
//! Generators live in `build_support/board_cfg.rs`, shared with each
//! platform's build.rs — one implementation, two OUT_DIRs.
//! See `docs/designs/shared-core-extraction.md` §3.D.

#[path = "../build_support/config.rs"]
mod config;

#[path = "../build_support/board_cfg.rs"]
mod board_cfg;

#[path = "../build_support/jvm_defaults.rs"]
mod jvm_defaults;

#[path = "../build_support/freertos_host.rs"]
mod freertos_host;

#[path = "../build_support/lvgl.rs"]
mod lvgl;

#[path = "../build_support/papk.rs"]
mod papk;

fn main() {
    let out = &std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    // picodroid-core lives one directory below the repo root — the root is
    // where gradlew, sdk/, and Cargo.toml are.
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .expect("picodroid-core must be a direct subdirectory of the repo root");

    // Resolve the active board across platform families (None for boardless
    // builds such as `cargo build -p picodroid-core`).
    let board = board_cfg::resolve(&manifest_dir);

    papk::emit_framework_map_version(out, root);
    papk::embed_framework_classes(out, root, &board_cfg::framework_class_excludes(&board));

    // A board that declares a capability must have the matching feature
    // forwarded, or the driver would silently compile out.
    board_cfg::assert_forwarded_features_match(&board);

    // board.toml is the source of truth for capability cfgs and neutral
    // generated config. `Pins::Elsewhere`: the family HAL owns the
    // pin-bearing display/touch artifacts; we only need dimensions + cfgs.
    board_cfg::emit_neutral(out, &board, board_cfg::Pins::Elsewhere);

    // The simulator HAL lives in this crate (stage 8), and it emulates the
    // real panel and touch controller rather than faking their outputs — so
    // it needs the same pin-bearing wiring the family HAL does, not just the
    // neutral dimensions. Emitted from the shared generators in
    // build_support, into this crate's OUT_DIR.
    //
    // `has_display` / `has_touch` get emitted twice as a result (once by
    // emit_neutral above); duplicate `cargo:rustc-cfg` lines are harmless,
    // and separating cfg emission from file emission purely to avoid that
    // would complicate a generator four call sites share.
    let board_display = board.as_ref().and_then(|b| b.cfg.display.clone());
    let board_touch = board.as_ref().and_then(|b| b.cfg.touch.clone());
    config::emit_display_config(out, &board_display);
    config::emit_touch_config(out, &board_touch);

    if board.is_none() {
        // Boardless build: no board.toml to read, so fall back to the
        // forwarded Cargo features for capability cfgs. (With a board
        // resolved, emit_neutral already emitted these from board.toml and
        // assert_forwarded_features_match proved the two agree.)
        emit_capability_cfgs_from_features();
    }

    // LVGL's C sources are compiled here rather than by the platform crate,
    // because this crate now contains code that calls `lv_*`. Without it,
    // `cargo test -p picodroid-core` would fail to link — and independent
    // testability is most of the reason for extracting the framework.
    //
    // The platform crate must NOT also compile LVGL: `cc` static libs
    // propagate to the final binary through the dependency, so two builders
    // would mean duplicate symbols. Ownership moved in one commit for
    // exactly that reason.
    //
    // Board overrides (`lv_dpi`, `lv_mem_kb`) come from the same board.toml
    // resolution above; boardless builds get lv_conf.h's defaults.
    let lvgl_board_props = board.as_ref().map(|b| b.cfg.props.clone());
    lvgl::build(out, &lvgl_board_props, root);

    // The simulator's kernel: the real FreeRTOS + POSIX port, compiled for the
    // host. Owned here for the same reason LVGL is — this crate holds the code
    // that calls into it (hal/sim/rtos_freertos.rs) and the allocator shims the
    // kernel links against.
    if std::env::var("CARGO_FEATURE_SIM").is_ok() {
        freertos_host::build(root, &manifest_dir.join("freertos-host"));
    }
}

/// Boardless fallback: derive capability cfgs from forwarded Cargo features.
///
/// All `rustc-check-cfg` declarations are emitted unconditionally by
/// `board_cfg::emit_neutral`, so this only adds `rustc-cfg` values.
fn emit_capability_cfgs_from_features() {
    if std::env::var("CARGO_FEATURE_SENSOR_BME688").is_ok() {
        println!("cargo:rustc-cfg=sensor_bme688");
        println!("cargo:rustc-cfg=any_sensor");
    }
    if std::env::var("CARGO_FEATURE_SENSOR_LTR559").is_ok() {
        println!("cargo:rustc-cfg=sensor_ltr559");
        println!("cargo:rustc-cfg=any_sensor");
    }
    if std::env::var("CARGO_FEATURE_NETWORK_CYW43").is_ok() {
        println!("cargo:rustc-cfg=network_cyw43");
        println!("cargo:rustc-cfg=has_network");
    }
}
