// SPDX-License-Identifier: GPL-3.0-only
// Suppress dead-code / unused warnings from the imported build_support modules.
// picodroid-core only calls a subset of their functions.
#![allow(dead_code, unused_imports, unused_variables)]
//! picodroid-core build script.
//! Generates framework_classes.rs and framework_unshrink.rs into OUT_DIR.
//! Both picodroid (RP) and picodroid-esp reference picodroid-core as a path
//! dependency, so this runs once per workspace and its OUT_DIR is shared.

#[path = "../build_support/config.rs"]
mod config;

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

    papk::emit_framework_map_version(out, root);
    papk::embed_framework_classes(out, root);

    // Declare all board-capability cfgs as known so rustc doesn't warn when
    // picodroid-core code gates on them.  Actual cfg *values* are emitted
    // below from Cargo features forwarded by the binary crate.
    for cfg in &[
        "any_sensor",
        "sensor_bme688",
        "sensor_ltr559",
        "has_network",
        "network_cyw43",
        "has_display",
        "has_touch",
        "has_buttons",
    ] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
    }

    // Emit board-capability cfgs from Cargo features forwarded by the binary.
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
