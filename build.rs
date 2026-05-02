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
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let is_arm_embedded = target_arch == "arm";

    let mut sensors = Vec::new();
    let mut buttons = Vec::new();
    let mut board_display = None;
    let mut board_touch = None;
    let mut board_background_pool = None;

    // Parse board config for the active board (both ARM and sim).
    let board_cfg_full = config::resolve_active_board().map(|name| {
        let path = format!("boards/{name}/board.toml");
        println!("cargo:rerun-if-changed={path}");
        config::parse_board_toml(&path)
    });

    if let Some(ref bc) = board_cfg_full {
        sensors = bc.sensors.clone();
        buttons = bc.buttons.clone();
        board_display = bc.display.clone();
        board_touch = bc.touch.clone();
        board_background_pool = bc.background_pool.clone();
    }

    // Emit OUT_DIR/board_imports.rs so src/boards/mod.rs can include the
    // active board's mod.rs (or no-op when no board is active).
    let active_board_name = config::resolve_active_board();
    boards::emit_board_imports(out, active_board_name.as_deref());

    if is_arm_embedded {
        let board = board_cfg_full
            .as_ref()
            .expect("No board feature active — enable board-testbench or similar");

        let mcu_name = config::resolve_active_mcu(&board.props);
        let mcu_toml_path = config::find_mcu_toml(&mcu_name);
        let mcu = config::parse_toml(&mcu_toml_path);
        println!("cargo:rerun-if-changed={mcu_toml_path}");

        let mcu_family = mcu
            .get("family")
            .cloned()
            .unwrap_or_else(|| panic!("MCU toml missing 'family': {mcu_toml_path}"));

        boards::place_memory_x(out, &board.props, &mcu_family, &mcu_name);

        let freertos_config_dir = format!("mcus/{mcu_family}");
        freertos::build(out, &mcu, &mcu_toml_path, &mcu_family, &freertos_config_dir);

        if board.props.get("network_type").map(String::as_str) == Some("cyw43") {
            network::build_cyw43_driver(&mcu_family, &freertos_config_dir);
            network::build_freertos_tcp(&mcu_family, &freertos_config_dir);
        }
    }

    emit_network_config(&board_cfg_full);
    emit_sensor_config(out, &sensors);
    emit_display_config(out, &board_display);
    emit_touch_config(out, &board_touch);
    emit_button_config(out, &buttons);
    emit_background_pool_config(out, &board_background_pool);

    // LVGL applies to both ARM and sim builds.
    let board_cfg = board_cfg_full.as_ref().map(|bc| bc.props.clone());
    lvgl::build(out, &board_cfg);

    papk::emit_framework_map_version(out);
    papk::embed_framework_classes(out);
    papk::embed_apk(out, is_arm_embedded);
    papk::embed_papk_flash_init(out, is_arm_embedded);
}

/// Emit `has_network` / `network_<type>` rustc cfgs from board.toml. Mirrors the
/// `sensor_<kind>` pattern — board.toml is the single source of truth, and Rust
/// code gates modules on these cfgs rather than on Cargo features.
fn emit_network_config(board: &Option<config::BoardConfig>) {
    // Known network drivers — extend this list when adding a new wireless chip.
    const KNOWN_NETWORK_TYPES: &[&str] = &["cyw43"];

    println!("cargo:rustc-check-cfg=cfg(has_network)");
    for t in KNOWN_NETWORK_TYPES {
        println!("cargo:rustc-check-cfg=cfg(network_{t})");
    }

    let Some(b) = board else { return };
    if b.props.get("has_network").map(String::as_str) != Some("true") {
        return;
    }
    println!("cargo:rustc-cfg=has_network");

    if let Some(t) = b.props.get("network_type") {
        if !KNOWN_NETWORK_TYPES.contains(&t.as_str()) {
            panic!("board.toml: unknown network_type '{t}' (known: {KNOWN_NETWORK_TYPES:?})");
        }
        println!("cargo:rustc-cfg=network_{t}");
    }
}

fn emit_sensor_config(out: &std::path::Path, sensors: &[config::SensorDecl]) {
    println!("cargo:rustc-check-cfg=cfg(any_sensor)");
    println!("cargo:rustc-check-cfg=cfg(sensor_bme688)");
    println!("cargo:rustc-check-cfg=cfg(sensor_ltr559)");

    if !sensors.is_empty() {
        println!("cargo:rustc-cfg=any_sensor");
    }
    let mut kinds_seen = std::collections::HashSet::new();
    for s in sensors {
        if kinds_seen.insert(s.kind.clone()) {
            println!("cargo:rustc-cfg=sensor_{}", s.kind);
        }
    }

    let mut code = String::new();
    code.push_str("// Generated by build.rs — do not edit\n\n");
    code.push_str("#[derive(Debug, Clone, Copy)]\n");
    code.push_str("#[repr(u8)]\n");
    code.push_str("pub enum SensorKind {\n");
    code.push_str("    Bme688 = 0,\n");
    code.push_str("    Ltr559 = 1,\n");
    code.push_str("}\n\n");
    code.push_str("#[derive(Debug, Clone, Copy)]\n");
    code.push_str("pub struct SensorHwCfg {\n");
    code.push_str("    pub kind: SensorKind,\n");
    code.push_str("    pub bus_id: u8,\n");
    code.push_str("    pub addr: u8,\n");
    code.push_str("}\n\n");

    code.push_str("pub const SENSORS: &[SensorHwCfg] = &[\n");
    for s in sensors {
        let kind_variant = match s.kind.as_str() {
            "bme688" => "Bme688",
            "ltr559" => "Ltr559",
            other => panic!("No SensorKind variant for '{other}'"),
        };
        let bus_id = s
            .bus
            .strip_prefix("I2C")
            .unwrap_or_else(|| panic!("unsupported bus '{}', expected I2Cn", s.bus))
            .parse::<u8>()
            .unwrap_or_else(|_| panic!("invalid bus id in '{}'", s.bus));
        code.push_str(&format!(
            "    SensorHwCfg {{ kind: SensorKind::{kind_variant}, bus_id: {bus_id}, addr: 0x{:02X} }},\n",
            s.addr
        ));
    }
    code.push_str("];\n");

    let path = out.join("sensor_table.rs");
    let mut f = File::create(&path).expect("create sensor_table.rs");
    f.write_all(code.as_bytes()).expect("write sensor_table.rs");
}

fn emit_display_config(
    out: &std::path::Path,
    display: &Option<std::collections::HashMap<String, String>>,
) {
    println!("cargo:rustc-check-cfg=cfg(has_display)");

    let mut code = String::from("// Generated by build.rs — do not edit\n\n");

    if let Some(d) = display {
        println!("cargo:rustc-cfg=has_display");

        let get = |key: &str| -> &str {
            d.get(key)
                .unwrap_or_else(|| panic!("[display] missing '{key}'"))
                .as_str()
        };

        code.push_str(&format!(
            "pub const SCREEN_WIDTH: u16 = {};\n",
            get("width")
        ));
        code.push_str(&format!(
            "pub const SCREEN_HEIGHT: u16 = {};\n",
            get("height")
        ));
        code.push_str(&format!(
            "pub const BAND_HEIGHT: usize = {};\n",
            get("band_height")
        ));
        code.push_str(&format!(
            "pub const SCROLL_LIMIT: u8 = {};\n",
            get("scroll_limit")
        ));
        code.push_str(&format!("pub const SPI_ID: u8 = {};\n", get("spi_id")));
        code.push_str(&format!("pub const SPI_FREQ: u32 = {};\n", get("spi_freq")));
        code.push_str(&format!("pub const PIN_DC: u8 = {};\n", get("pin_dc")));
        code.push_str(&format!("pub const PIN_CS: u8 = {};\n", get("pin_cs")));
        code.push_str(&format!("pub const PIN_BL: u8 = {};\n", get("pin_bl")));

        // RST is optional (some boards tie RST high)
        if let Some(rst) = d.get("pin_rst") {
            code.push_str(&format!("pub const PIN_RST: Option<u8> = Some({rst});\n"));
        } else {
            code.push_str("pub const PIN_RST: Option<u8> = None;\n");
        }

        code.push_str(&format!("pub const MADCTL: u8 = {};\n", get("madctl")));
    } else {
        code.push_str("pub const SCREEN_WIDTH: u16 = 320;\n");
        code.push_str("pub const SCREEN_HEIGHT: u16 = 240;\n");
        code.push_str("pub const BAND_HEIGHT: usize = 20;\n");
        code.push_str("pub const SCROLL_LIMIT: u8 = 30;\n");
    }

    let path = out.join("display_config.rs");
    let mut f = File::create(&path).expect("create display_config.rs");
    f.write_all(code.as_bytes())
        .expect("write display_config.rs");
}

fn emit_touch_config(
    out: &std::path::Path,
    touch: &Option<std::collections::HashMap<String, String>>,
) {
    println!("cargo:rustc-check-cfg=cfg(has_touch)");

    let mut code = String::from("// Generated by build.rs — do not edit\n\n");

    if let Some(t) = touch {
        println!("cargo:rustc-cfg=has_touch");

        let get = |key: &str| -> &str {
            t.get(key)
                .unwrap_or_else(|| panic!("[touch] missing '{key}'"))
                .as_str()
        };

        code.push_str(&format!(
            "pub const TOUCH_SPI_FREQ: u32 = {};\n",
            get("spi_freq")
        ));
        code.push_str(&format!(
            "pub const TOUCH_PIN_CS: u8 = {};\n",
            get("pin_cs")
        ));
        code.push_str(&format!(
            "pub const TOUCH_PIN_IRQ: u8 = {};\n",
            get("pin_irq")
        ));
        code.push_str(&format!(
            "pub const TOUCH_PIN_MISO: u8 = {};\n",
            get("pin_miso")
        ));
        code.push_str(&format!(
            "pub const TOUCH_CAL_X_MIN: u16 = {};\n",
            get("cal_x_min")
        ));
        code.push_str(&format!(
            "pub const TOUCH_CAL_X_MAX: u16 = {};\n",
            get("cal_x_max")
        ));
        code.push_str(&format!(
            "pub const TOUCH_CAL_Y_MIN: u16 = {};\n",
            get("cal_y_min")
        ));
        code.push_str(&format!(
            "pub const TOUCH_CAL_Y_MAX: u16 = {};\n",
            get("cal_y_max")
        ));

        let swap = t.get("swap_xy").map_or("false", |v| v.as_str());
        code.push_str(&format!("pub const TOUCH_SWAP_XY: bool = {swap};\n"));
    }

    let path = out.join("touch_config.rs");
    let mut f = File::create(&path).expect("create touch_config.rs");
    f.write_all(code.as_bytes()).expect("write touch_config.rs");
}

/// Emit background thread pool constants from board.toml's optional
/// `[background_pool]` section. Missing keys fall back to defaults
/// (4 workers, priority 5 in the BG tier, 4 KiB stack, 32-deep queue).
fn emit_background_pool_config(
    out: &std::path::Path,
    pool: &Option<std::collections::HashMap<String, String>>,
) {
    const DEFAULT_THREADS: u32 = 4;
    const DEFAULT_PRIORITY: u32 = 5; // PRIORITY_BG_5
    const DEFAULT_STACK_BYTES: u32 = 4096;
    const DEFAULT_QUEUE_DEPTH: u32 = 32;

    let threads = pool
        .as_ref()
        .and_then(|p| p.get("threads"))
        .map(|v| v.parse::<u32>().expect("[background_pool] threads: int"))
        .unwrap_or(DEFAULT_THREADS);
    let priority = pool
        .as_ref()
        .and_then(|p| p.get("priority"))
        .map(|v| v.parse::<u32>().expect("[background_pool] priority: int"))
        .unwrap_or(DEFAULT_PRIORITY);
    let stack_bytes = pool
        .as_ref()
        .and_then(|p| p.get("stack_bytes"))
        .map(|v| {
            v.parse::<u32>()
                .expect("[background_pool] stack_bytes: int")
        })
        .unwrap_or(DEFAULT_STACK_BYTES);
    let queue_depth = pool
        .as_ref()
        .and_then(|p| p.get("queue_depth"))
        .map(|v| {
            v.parse::<u32>()
                .expect("[background_pool] queue_depth: int")
        })
        .unwrap_or(DEFAULT_QUEUE_DEPTH);

    assert!(
        (1..=10).contains(&priority),
        "[background_pool] priority must be in 1..=10 (BG tier), got {priority}"
    );
    assert!(
        (1..=32).contains(&threads),
        "[background_pool] threads must be in 1..=32, got {threads}"
    );

    let mut code = String::from("// Generated by build.rs — do not edit\n\n");
    code.push_str(&format!("pub const POOL_THREADS: u32 = {threads};\n"));
    code.push_str(&format!("pub const POOL_PRIORITY: u8 = {priority};\n"));
    code.push_str(&format!(
        "pub const POOL_STACK_BYTES: u32 = {stack_bytes};\n"
    ));
    code.push_str(&format!(
        "pub const POOL_QUEUE_DEPTH: u32 = {queue_depth};\n"
    ));

    let path = out.join("background_pool_config.rs");
    let mut f = File::create(&path).expect("create background_pool_config.rs");
    f.write_all(code.as_bytes())
        .expect("write background_pool_config.rs");
}

fn emit_button_config(out: &std::path::Path, buttons: &[config::ButtonDecl]) {
    println!("cargo:rustc-check-cfg=cfg(has_buttons)");

    let mut code = String::from("// Generated by build.rs — do not edit\n\n");

    if !buttons.is_empty() {
        println!("cargo:rustc-cfg=has_buttons");
    }

    // Table entries: (pin, LV_KEY_*, android_keycode).
    // Referenced from engine.rs where `use crate::lvgl_ffi::*;` is in scope.
    code.push_str("pub const BUTTONS: &[(u8, u32, i32)] = &[\n");
    for b in buttons {
        code.push_str(&format!(
            "    ({}, LV_KEY_{}, {}),\n",
            b.pin, b.lv_key, b.keycode
        ));
    }
    code.push_str("];\n");

    let path = out.join("button_config.rs");
    let mut f = File::create(&path).expect("create button_config.rs");
    f.write_all(code.as_bytes())
        .expect("write button_config.rs");
}
