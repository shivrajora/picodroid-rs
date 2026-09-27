// SPDX-License-Identifier: GPL-3.0-only
//! Compiles the vendored LVGL C sources (`third_party/lvgl`) against
//! `lvgl/lv_conf.h`, with the active board's overrides.
//!
//! The board is discovered the way picodroid-core's build script discovers
//! it: by searching `platforms/*/boards/<name>` for the `board-*` feature
//! forwarded to this crate. Both scripts call the same `build_support`
//! functions on the same board.toml, so the C build and the Rust cfgs cannot
//! disagree about `hw_vscroll` or where LVGL's pool lives.

use build_support::{board_cfg, config, hot_ram, lvgl};

fn main() {
    let out = &std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = &config::repo_root(&manifest_dir);
    let board = board_cfg::resolve(&manifest_dir);

    // The two cfgs the bindings gate declarations on. `lv_mem_in_psram` is
    // device-only (the simulator keeps its .bss pool); `hw_vscroll` is both.
    board_cfg::emit_lvgl_cfgs(&board);
    println!("cargo:rustc-check-cfg=cfg(hw_vscroll)");
    let hw_vscroll = board_cfg::hw_vscroll(&board);
    if hw_vscroll {
        println!("cargo:rustc-cfg=hw_vscroll");
    }

    // Board overrides (`lv_dpi`, `lv_mem_kb`) come from board.toml; boardless
    // builds get lv_conf.h's defaults. The MCU toml may pin the C
    // optimisation level for its target (`c_opt_level`; the rp2040 compiles
    // its C at -Os) and carries the default face ladder (`text_sizes`) a
    // board may override.
    let board_props = board.as_ref().map(|b| b.cfg.props.clone());
    let mcu = board.as_ref().map(|b| b.mcu().1);
    let text_sizes = board_cfg::text_sizes(&board);
    // A device board with `hot_ram_kb` set has its hot LVGL functions moved
    // to SRAM from the family's list (`build_support::hot_ram`); the
    // simulator executes from host memory and keeps the archive as built.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let hot_ram_list = board.as_ref().and_then(|b| {
        let (mcu_toml_path, _) = b.mcu();
        (target_os == "none" && hot_ram::board_hot_ram_kb(&b.cfg.props, &b.name) > 0)
            .then(|| hot_ram::list_path(&mcu_toml_path, "lvgl"))
    });
    lvgl::build(
        out,
        &board_props,
        mcu.as_ref(),
        root,
        &manifest_dir.join("lvgl"),
        hw_vscroll,
        &text_sizes,
        hot_ram_list.as_deref(),
    );
}
