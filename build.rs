//! Build-script orchestrator. Submodules under `build_support/` own each
//! concern (config discovery, FreeRTOS compile, LVGL compile, networking,
//! PAPK/APK embedding). This file wires them together.

#[path = "build_support/config.rs"]
mod config;

#[path = "build_support/boards.rs"]
mod boards;

#[path = "build_support/freertos.rs"]
mod freertos;

#[path = "build_support/lvgl.rs"]
mod lvgl;

#[path = "build_support/network.rs"]
mod network;

#[path = "build_support/papk.rs"]
mod papk;

use std::env;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_arm_embedded = target_arch == "arm";

    if is_arm_embedded {
        let board_name = config::resolve_active_board()
            .expect("No board feature active — enable board-testbench or similar");
        let board_toml_path = format!("boards/{board_name}/board.toml");
        let board = config::parse_toml(&board_toml_path);
        println!("cargo:rerun-if-changed={board_toml_path}");

        let mcu_name = config::resolve_active_mcu(&board);
        let mcu_toml_path = config::find_mcu_toml(&mcu_name);
        let mcu = config::parse_toml(&mcu_toml_path);
        println!("cargo:rerun-if-changed={mcu_toml_path}");

        let mcu_family = mcu
            .get("family")
            .cloned()
            .unwrap_or_else(|| panic!("MCU toml missing 'family': {mcu_toml_path}"));

        boards::place_memory_x(out, &board, &mcu_family, &mcu_name);

        let freertos_config_dir = format!("mcus/{mcu_family}");
        freertos::build(out, &mcu, &mcu_toml_path, &mcu_family, &freertos_config_dir);

        if env::var("CARGO_FEATURE_NET_CYW43").is_ok() {
            network::build_cyw43_driver(&freertos_config_dir);
            network::build_freertos_tcp(&freertos_config_dir);
        }
    }

    // LVGL applies to both ARM and sim builds.
    let board_cfg = config::resolve_active_board().map(|name| {
        let path = format!("boards/{name}/board.toml");
        config::parse_toml(&path)
    });
    lvgl::build(out, &board_cfg);

    papk::emit_framework_map_version(out);
    papk::embed_framework_classes(out);
    papk::embed_apk(out, is_arm_embedded);
    papk::embed_papk_flash_init(out, is_arm_embedded);
}
