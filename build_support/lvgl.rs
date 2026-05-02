// SPDX-License-Identifier: GPL-3.0-only
//! LVGL C sources compilation.

use crate::config::collect_files;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// Compile LVGL C sources into a static library.
pub fn build(_out: &Path, board_cfg: &Option<HashMap<String, String>>) {
    let lvgl_src = Path::new("vendor/lvgl/src");
    if !lvgl_src.exists() {
        return;
    }

    let c_files = collect_files(lvgl_src, "c");
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

    let mut build = cc::Build::new();
    build
        .include(".")
        .include("vendor/lvgl")
        .include("vendor/lvgl/src")
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

    for f in &c_files {
        build.file(f);
    }

    build.compile("lvgl");

    println!("cargo:rerun-if-changed=lv_conf.h");
    println!("cargo:rerun-if-changed=vendor/lvgl/src");
}
