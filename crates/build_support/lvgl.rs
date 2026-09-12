// SPDX-License-Identifier: GPL-3.0-only
//! LVGL C sources compilation.

use crate::config::collect_files;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// A decimal or `0x` integer from a toml map, if the key is present.
fn parse_int(props: &HashMap<String, String>, key: &str) -> Option<u64> {
    let raw = props.get(key)?.trim();
    let parsed = match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => raw.parse::<u64>(),
    };
    Some(parsed.unwrap_or_else(|e| panic!("`{key} = {raw}` is not an integer: {e}")))
}

/// Compile LVGL C sources into a static library.
///
/// `repo_root` must be the absolute path to the repository root so that
/// `third_party/lvgl` can be located regardless of which
/// `platforms/<family>/` directory the build.rs runs from. `conf_dir` is the
/// directory holding `lv_conf.h` — the calling crate's own `lvgl/`, beside
/// the other C configs it owns (`freertos-host/`, `net-freertos-tcp/`).
pub fn build(
    _out: &Path,
    board_cfg: &Option<HashMap<String, String>>,
    mcu: Option<&HashMap<String, String>>,
    repo_root: &Path,
    conf_dir: &Path,
) {
    let lvgl_src = repo_root.join("third_party/lvgl/src");
    if !lvgl_src.exists() {
        return;
    }

    let c_files = collect_files(&lvgl_src, "c");
    if c_files.is_empty() {
        return;
    }

    // Filter out stdlib backends we don't use (clib, micropython, rtthread),
    // GPU backends we disabled, and driver files (we use our own HAL).
    let c_files: Vec<_> = c_files
        .into_iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            !s.contains("stdlib/clib")
                && !s.contains("stdlib/micropython")
                && !s.contains("stdlib/rtthread")
                && !s.contains("draw/vg_lite")
                && !s.contains("draw/nxp")
                && !s.contains("draw/sdl")
                && !s.contains("draw/renesas")
                && !s.contains("draw/opengles")
                && !s.contains("libs/thorvg")
                && !s.contains("others/vg_lite_tvg")
                && !s.contains("/drivers/")
        })
        .collect();

    let lvgl_dir = repo_root.join("third_party/lvgl");
    let mut build = cc::Build::new();
    build
        // `lv_conf.h` is found through the include path: LV_CONF_INCLUDE_SIMPLE
        // below makes LVGL `#include "lv_conf.h"` unqualified.
        .include(conf_dir)
        .include(&lvgl_dir)
        .include(&lvgl_src)
        .define("LV_CONF_INCLUDE_SIMPLE", None)
        .define("LV_LVGL_H_INCLUDE_SIMPLE", None)
        .warnings(false)
        .extra_warnings(false);

    // Board-specific LVGL overrides (take precedence over lv_conf.h via #ifndef guards).
    if let Some(cfg) = board_cfg {
        if let Some(dpi) = cfg.get("lv_dpi") {
            build.define("LV_DPI_DEF", dpi.as_str());
        }
        if let Some(mem_kb) = cfg.get("lv_mem_kb") {
            let mem_val = format!("({mem_kb} * 1024U)");
            build.define("LV_MEM_SIZE", mem_val.as_str());
        }
        // Where the pool lives. A board that puts it in the module's PSRAM
        // (`lv_mem_in_psram`) hands LVGL the window's origin instead of the
        // .bss array: lv_mem_core_builtin.c creates the TLSF pool at
        // LV_MEM_ADR when that is nonzero. Device builds only — the
        // simulator has no such window and keeps its .bss pool whatever the
        // board says — and the draw-buffer hook beside lv_conf.h goes with
        // it, so render targets stay in SRAM
        // (docs/designs/psram-lvgl-fluid-scroll-2026-09.md §4).
        if cfg.get("lv_mem_in_psram").map(String::as_str) == Some("true")
            && crate::config::is_embedded()
        {
            let mcu = mcu.unwrap_or_else(|| {
                panic!("board.toml: lv_mem_in_psram = true needs the MCU descriptor")
            });
            let psram_kb = parse_int(mcu, "psram_kb").unwrap_or_else(|| {
                panic!("board.toml: lv_mem_in_psram = true but the MCU toml declares no psram_kb")
            });
            let origin = parse_int(mcu, "psram_origin")
                .unwrap_or_else(|| panic!("MCU toml declares psram_kb but no psram_origin"));
            let mem_kb = cfg
                .get("lv_mem_kb")
                .map(|v| v.trim().parse::<u64>().expect("lv_mem_kb"))
                .unwrap_or(64);
            assert!(
                mem_kb <= psram_kb,
                "board.toml: lv_mem_kb ({mem_kb} KB) does not fit the {psram_kb} KB of PSRAM"
            );
            build.define("LV_MEM_ADR", format!("{origin:#x}U").as_str());
            build.define("PICODROID_LV_MEM_IN_PSRAM", "1");
            let hook = conf_dir.join("lv_draw_buf_sram.c");
            build.file(&hook);
            println!("cargo:rerun-if-changed={}", hook.display());
        }
    }

    // ARM gcc defaults to -fshort-enums, making C enums 1 byte when values
    // fit.  Our Rust FFI (lvgl_ffi.rs) mirrors this with u8 typedefs.  On
    // x86_64 (sim builds) enums are 4 bytes by default, which breaks struct
    // layout (e.g. lv_indev_data_t.state lands at the wrong offset).  Force
    // -fshort-enums on non-ARM targets so the C and Rust layouts match.
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "arm" {
        build.flag("-fshort-enums");
    }

    // The MCU may pin the C optimisation level for its own target
    // (`c_opt_level`; the rp2040 compiles its C at -Os). Otherwise cc-rs
    // mirrors cargo's OPT_LEVEL.
    if let Some(mcu) = mcu {
        crate::config::apply_c_opt_level(&mut build, mcu);
    }

    for f in &c_files {
        build.file(f);
    }

    build.compile("lvgl");

    println!(
        "cargo:rerun-if-changed={}",
        conf_dir.join("lv_conf.h").display()
    );
    println!("cargo:rerun-if-changed={}", lvgl_src.display());
}
