// SPDX-License-Identifier: GPL-3.0-only
// Suppress dead-code / unused warnings from the imported build_support modules.
#![allow(dead_code, unused_imports, unused_variables)]

#[path = "../../build_support/boards.rs"]
mod boards;

#[path = "../../build_support/config.rs"]
mod config;

#[path = "../../build_support/papk.rs"]
mod papk;

use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // platforms/esp lives two directories below the repo root.
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("platforms/esp must be two levels below the repo root");

    let is_embedded = matches!(
        env::var("CARGO_CFG_TARGET_ARCH")
            .unwrap_or_default()
            .as_str(),
        "arm" | "xtensa"
    );

    // ESP memory map / linker scripts come from esp-hal's build.rs (it
    // copies linkall.x, memory.x, esp32s3.x, alias.x, hal-defaults.x into
    // its OUT_DIR and adds that to the link search path).  We rely on
    // -Tlinkall.x in .cargo/config.toml; no per-MCU .x file of our own.

    // Board capability cfgs — emit check-cfg so rustc accepts the cfg names.
    for cfg in &[
        "has_touch",
        "has_network",
        "has_buttons",
        "any_sensor",
        "sensor_bme688",
        "sensor_ltr559",
        "network_cyw43",
    ] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
    }

    // Resolve board and emit board_imports.rs + display_config.rs.
    let active_board = config::resolve_active_board();
    boards::emit_board_imports(out, active_board.as_deref());

    let board_display = active_board.and_then(|name| {
        let path = format!("boards/{name}/board.toml");
        println!("cargo:rerun-if-changed={path}");
        config::parse_board_toml(&path).display
    });
    // config::emit_display_config also emits the has_display check-cfg.
    config::emit_display_config(out, &board_display);

    // APK embedding. ESP M1 has no PAPK flash region yet, so always embed
    // inline (pass false for is_arm_embedded). The arm_embedded path in
    // embed_apk assumes the APK lives in PAPK_FLASH (placed by
    // embed_papk_flash_init), which is RP-only today.
    papk::embed_apk(out, false);
    // PAPK flash init places a linker section in the PAPK_FLASH memory region,
    // which is only declared in RP linker scripts.  esp-hal's memory.x has no
    // such region, so skip this step for ESP builds (we already inlined the
    // APK above via embed_apk(false)).
    let is_esp = matches!(
        env::var("CARGO_CFG_TARGET_ARCH")
            .unwrap_or_default()
            .as_str(),
        "xtensa"
    );
    papk::embed_papk_flash_init(out, is_embedded && !is_esp);
    // Framework mapping version for compat check in app.rs.
    papk::emit_framework_map_version(out, root);

    // LVGL: vendor/lvgl/src is relative to picodroid-esp/ and does not exist
    // in M1 (the submodule lives in the root workspace).  lvgl::build returns
    // early when the path is absent — no LVGL link step for M1 ESP builds.
    // No LVGL functions are called in M1 ESP, so unresolved extern symbols do
    // not appear.  A vendor/ symlink or submodule copy enables LVGL for M2+.
}
