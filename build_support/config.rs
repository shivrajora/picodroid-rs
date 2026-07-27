// SPDX-License-Identifier: GPL-3.0-only
//! Build-time configuration resolution: board + MCU discovery, TOML parsing,
//! filesystem walking. Pure helpers with no side-effects on cargo.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// A sensor peripheral declared in board.toml via `[[sensor]]`.
#[derive(Debug, Clone)]
pub struct SensorDecl {
    pub kind: String,
    pub bus: String,
    pub addr: u8,
}

/// A hardware button declared in board.toml via `[[button]]`.
#[derive(Debug, Clone)]
pub struct ButtonDecl {
    pub pin: u8,
    pub lv_key: String,
    pub keycode: i32,
}

/// Board configuration: flat key-value properties plus peripheral declarations.
#[derive(Debug)]
pub struct BoardConfig {
    pub props: HashMap<String, String>,
    pub sensors: Vec<SensorDecl>,
    pub buttons: Vec<ButtonDecl>,
    pub display: Option<HashMap<String, String>>,
    pub touch: Option<HashMap<String, String>>,
    pub background_pool: Option<HashMap<String, String>>,
    pub jvm: Option<HashMap<String, String>>,
}

const KNOWN_SENSOR_KINDS: &[&str] = &["bme688", "ltr559"];
const KNOWN_LV_KEYS: &[&str] = &["PREV", "NEXT", "ENTER", "ESC"];

/// Split a semicolon- or comma-separated TOML scalar into trimmed entries.
/// Empty entries are skipped. Used by MCU TOML keys that pack multiple
/// values (e.g. `freertos_c_defines = "A=1;B=0"`) without requiring a real
/// TOML array parser.
pub fn parse_str_list(val: &str) -> Vec<String> {
    val.split([';', ','])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn strip_quotes(val: &str) -> String {
    if (val.starts_with('"') && val.ends_with('"'))
        || (val.starts_with('\'') && val.ends_with('\''))
    {
        val[1..val.len() - 1].to_string()
    } else {
        val.to_string()
    }
}

fn parse_int_value(val: &str) -> u8 {
    let val = val.trim();
    if let Some(hex) = val.strip_prefix("0x").or_else(|| val.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("Invalid hex value: {val}"))
    } else {
        val.parse()
            .unwrap_or_else(|_| panic!("Invalid int value: {val}"))
    }
}

fn parse_i32_value(val: &str) -> i32 {
    let val = val.trim();
    if let Some(hex) = val.strip_prefix("0x").or_else(|| val.strip_prefix("0X")) {
        i32::from_str_radix(hex, 16).unwrap_or_else(|_| panic!("Invalid hex value: {val}"))
    } else {
        val.parse()
            .unwrap_or_else(|_| panic!("Invalid int value: {val}"))
    }
}

/// Current section being parsed in board.toml.
#[derive(PartialEq)]
enum Section {
    Top,
    Display,
    Touch,
    BackgroundPool,
    Jvm,
    Sensor,
    Button,
    Unknown,
}

/// Parse a board TOML file with flat properties, `[display]`, `[touch]`,
/// `[[sensor]]`, and `[[button]]` sections.
pub fn parse_board_toml(path: &str) -> BoardConfig {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
    let mut props = HashMap::new();
    let mut sensors: Vec<SensorDecl> = Vec::new();
    let mut buttons: Vec<ButtonDecl> = Vec::new();
    let mut display: Option<HashMap<String, String>> = None;
    let mut touch: Option<HashMap<String, String>> = None;
    let mut background_pool: Option<HashMap<String, String>> = None;
    let mut jvm: Option<HashMap<String, String>> = None;
    let mut section = Section::Top;
    let mut cur_sensor: HashMap<String, String> = HashMap::new();
    let mut cur_button: HashMap<String, String> = HashMap::new();

    // Flush any in-progress [[array]] entry before transitioning sections.
    macro_rules! flush_array {
        ($section:expr, $sensors:ident, $buttons:ident, $cur_sensor:ident, $cur_button:ident, $path:ident) => {
            match $section {
                Section::Sensor => {
                    $sensors.push(finish_sensor(&$cur_sensor, $path));
                    $cur_sensor.clear();
                }
                Section::Button => {
                    $buttons.push(finish_button(&$cur_button, $path));
                    $cur_button.clear();
                }
                _ => {}
            }
        };
    }

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section headers
        if trimmed == "[[sensor]]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            section = Section::Sensor;
            continue;
        }
        if trimmed == "[[button]]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            section = Section::Button;
            continue;
        }
        if trimmed == "[display]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            display = Some(HashMap::new());
            section = Section::Display;
            continue;
        }
        if trimmed == "[touch]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            touch = Some(HashMap::new());
            section = Section::Touch;
            continue;
        }
        if trimmed == "[background_pool]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            background_pool = Some(HashMap::new());
            section = Section::BackgroundPool;
            continue;
        }
        if trimmed == "[jvm]" {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            jvm = Some(HashMap::new());
            section = Section::Jvm;
            continue;
        }
        if trimmed.starts_with('[') {
            flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
            section = Section::Unknown;
            continue;
        }

        // Key-value pairs routed by current section
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let val = strip_quotes(val.trim());
            match section {
                Section::Top => {
                    props.insert(key, val);
                }
                Section::Sensor => {
                    cur_sensor.insert(key, val);
                }
                Section::Button => {
                    cur_button.insert(key, val);
                }
                Section::Display => {
                    display.as_mut().unwrap().insert(key, val);
                }
                Section::Touch => {
                    touch.as_mut().unwrap().insert(key, val);
                }
                Section::BackgroundPool => {
                    background_pool.as_mut().unwrap().insert(key, val);
                }
                Section::Jvm => {
                    jvm.as_mut().unwrap().insert(key, val);
                }
                Section::Unknown => {}
            }
        }
    }
    flush_array!(section, sensors, buttons, cur_sensor, cur_button, path);
    BoardConfig {
        props,
        sensors,
        buttons,
        display,
        touch,
        background_pool,
        jvm,
    }
}

fn finish_sensor(fields: &HashMap<String, String>, path: &str) -> SensorDecl {
    let kind = fields
        .get("kind")
        .unwrap_or_else(|| panic!("{path}: [[sensor]] missing 'kind'"))
        .clone();
    if !KNOWN_SENSOR_KINDS.contains(&kind.as_str()) {
        panic!("{path}: unknown sensor kind '{kind}' (known: {KNOWN_SENSOR_KINDS:?})");
    }
    let bus = fields
        .get("bus")
        .unwrap_or_else(|| panic!("{path}: [[sensor]] kind={kind} missing 'bus'"))
        .clone();
    let addr_str = fields
        .get("addr")
        .unwrap_or_else(|| panic!("{path}: [[sensor]] kind={kind} missing 'addr'"));
    let addr = parse_int_value(addr_str);
    SensorDecl { kind, bus, addr }
}

fn finish_button(fields: &HashMap<String, String>, path: &str) -> ButtonDecl {
    let pin_str = fields
        .get("pin")
        .unwrap_or_else(|| panic!("{path}: [[button]] missing 'pin'"));
    let pin = parse_int_value(pin_str);
    let lv_key = fields
        .get("lv_key")
        .unwrap_or_else(|| panic!("{path}: [[button]] pin={pin} missing 'lv_key'"))
        .clone();
    if !KNOWN_LV_KEYS.contains(&lv_key.as_str()) {
        panic!("{path}: unknown lv_key '{lv_key}' (known: {KNOWN_LV_KEYS:?})");
    }
    let keycode_str = fields
        .get("keycode")
        .unwrap_or_else(|| panic!("{path}: [[button]] pin={pin} missing 'keycode'"));
    let keycode = parse_i32_value(keycode_str);
    ButtonDecl {
        pin,
        lv_key,
        keycode,
    }
}

/// Parse a simple TOML file (flat key = value pairs, no tables/arrays).
/// Supports string values (quoted or unquoted), integer values, and booleans.
/// Lines starting with '#' are comments.
pub fn parse_toml(path: &str) -> HashMap<String, String> {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
    let mut map = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let val = strip_quotes(val.trim());
            map.insert(key, val);
        }
    }
    map
}

/// Scan `CARGO_FEATURE_BOARD_*` env vars for the active board name.
///
/// Cargo uppercases feature names and replaces `-` with `_`, so the env-var
/// suffix already matches our snake_case board directory names after
/// lowercasing. `has_board` decides whether a candidate name corresponds to a
/// real board directory. Panics if more than one board feature is active.
fn resolve_active_board_with(has_board: impl Fn(&str) -> bool) -> Option<String> {
    let mut found: Option<String> = None;
    for (key, _) in env::vars() {
        let Some(suffix) = key.strip_prefix("CARGO_FEATURE_BOARD_") else {
            continue;
        };
        let candidate = suffix.to_lowercase();
        if !has_board(&candidate) {
            continue;
        }
        if let Some(prev) = &found {
            panic!(
                "Multiple board features active: {} and {}. Pick exactly one with \
                 `--features board-<name>` (see docs/cargo-aliases.md).",
                prev, candidate
            );
        }
        found = Some(candidate);
    }
    found
}

/// Locate a board directory by name, searching the calling crate first and
/// then every `platforms/<family>/boards/` in the repo.
///
/// `manifest_dir` is the calling crate's `CARGO_MANIFEST_DIR`. Platform
/// crates (`platforms/rp`) match on the first arm; crates that sit one level
/// below the repo root (`picodroid-core`) match on the second. This is what
/// lets a shared crate's build script read the same `board.toml` the active
/// platform crate is building against, without env vars or duplicated data.
pub fn find_board_dir(manifest_dir: &Path, name: &str) -> Option<PathBuf> {
    let local = manifest_dir.join("boards").join(name);
    if local.is_dir() {
        return Some(local);
    }
    // <manifest>/../platforms/*/boards/<name> — e.g. picodroid-core reaching
    // platforms/rp/boards/testbench_rp2350.
    let platforms = manifest_dir.parent()?.join("platforms");
    let mut entries: Vec<PathBuf> = fs::read_dir(platforms)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort(); // deterministic across filesystems
    entries
        .into_iter()
        .map(|family| family.join("boards").join(name))
        .find(|candidate| candidate.is_dir())
}

/// Resolve the active board to its directory, searching across platform
/// families. Returns `(name, dir)`; `None` when no board feature is active
/// (boardless host builds such as a bare `cargo build -p picodroid-core`).
pub fn resolve_active_board_dir(manifest_dir: &Path) -> Option<(String, PathBuf)> {
    let name = resolve_active_board_with(|n| find_board_dir(manifest_dir, n).is_some())?;
    let dir = find_board_dir(manifest_dir, &name)
        .unwrap_or_else(|| panic!("board directory vanished between probe and read: {name}"));
    Some((name, dir))
}

/// Resolve the active MCU name from the board.toml's `mcu` field. Every
/// `boards/*/board.toml` is required to declare this; chip-level Cargo
/// features (`chip-rp2040`, `chip-rp2350`, …) exist only to gate dep crates.
pub fn resolve_active_mcu(board_cfg: &HashMap<String, String>) -> String {
    board_cfg
        .get("mcu")
        .cloned()
        .expect("board.toml must declare 'mcu = \"<name>\"' (e.g. mcu = \"rp2350\")")
}

/// Find the MCU .toml file under `<platform_root>/mcus/<family>/<name>.toml`.
///
/// `platform_root` is the `platforms/<family>/` directory that owns the
/// board — derived from the resolved board directory, so shared crates can
/// read MCU definitions they do not own.
pub fn find_mcu_toml_in(platform_root: &Path, mcu_name: &str) -> String {
    let mcus_dir = platform_root.join("mcus");
    if let Ok(entries) = fs::read_dir(&mcus_dir) {
        let mut families: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        families.sort(); // deterministic across filesystems
        for family in families {
            if family.is_dir() {
                let candidate = family.join(format!("{mcu_name}.toml"));
                if candidate.exists() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
    }
    panic!(
        "MCU definition not found: {}/*/{mcu_name}.toml",
        mcus_dir.display()
    );
}

/// Recursively collect all files with the given extension under `dir`.
pub fn collect_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut result = Vec::new();
    collect_files_recursive(dir, ext, &mut result);
    result
}

fn collect_files_recursive(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
}

/// Emit `OUT_DIR/display_config.rs` from a board.toml `[display]` section.
///
/// Call from any platform's `build.rs` after parsing board.toml. The generated
/// file is `include!`-d by each platform's `hal/*/display.rs`.
///
/// When `display` is `None` (no `[display]` in board.toml), safe defaults are
/// emitted so the crate still compiles (useful for M1 stubs).
pub fn emit_display_config(out: &Path, display: &Option<HashMap<String, String>>) {
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

        // Hardware-only pin constants — only present when display is active.
        if let Some(spi_id) = d.get("spi_id") {
            code.push_str(&format!("pub const SPI_ID: u8 = {spi_id};\n"));
        }
        if let Some(spi_freq) = d.get("spi_freq") {
            code.push_str(&format!("pub const SPI_FREQ: u32 = {spi_freq};\n"));
        }
        // Optional SPI pad overrides — when None, the HAL falls back to the
        // chip-default pins (e.g. SPI0 SCK=GP2, MOSI=GP3, MISO=GP0 on RP2350).
        // The Pimoroni Pico Enviro+ Pack routes SPI0 SCK/MOSI to GP18/GP19
        // and is the canonical reason this knob exists.
        for key in ["spi_sck", "spi_mosi", "spi_miso"] {
            let const_name = key.to_uppercase();
            if let Some(v) = d.get(key) {
                code.push_str(&format!(
                    "pub const {const_name}: Option<u8> = Some({v});\n"
                ));
            } else {
                code.push_str(&format!("pub const {const_name}: Option<u8> = None;\n"));
            }
        }
        if let Some(pin_dc) = d.get("pin_dc") {
            code.push_str(&format!("pub const PIN_DC: u8 = {pin_dc};\n"));
        }
        if let Some(pin_cs) = d.get("pin_cs") {
            code.push_str(&format!("pub const PIN_CS: u8 = {pin_cs};\n"));
        }
        if let Some(pin_bl) = d.get("pin_bl") {
            code.push_str(&format!("pub const PIN_BL: u8 = {pin_bl};\n"));
        }
        if let Some(rst) = d.get("pin_rst") {
            code.push_str(&format!("pub const PIN_RST: Option<u8> = Some({rst});\n"));
        } else {
            code.push_str("pub const PIN_RST: Option<u8> = None;\n");
        }
        if let Some(madctl) = d.get("madctl") {
            code.push_str(&format!("pub const MADCTL: u8 = {madctl};\n"));
        }
    } else {
        // No [display] in board.toml — emit safe defaults so the crate compiles.
        code.push_str("pub const SCREEN_WIDTH: u16 = 320;\n");
        code.push_str("pub const SCREEN_HEIGHT: u16 = 240;\n");
        code.push_str("pub const BAND_HEIGHT: usize = 20;\n");
        code.push_str("pub const SCROLL_LIMIT: u8 = 30;\n");
    }

    let path = out.join("display_config.rs");
    fs::write(&path, code.as_bytes()).unwrap_or_else(|e| panic!("write display_config.rs: {e}"));
}

/// Emit `OUT_DIR/touch_config.rs` — the XPT2046 pin/calibration wiring for
/// `hal/rp/touch.rs` (family) and `hal/sim/touch.rs` (picodroid-core).
///
/// Emitted into both OUT_DIRs by the two build scripts rather than copied:
/// the simulator emulates a real XPT2046, so it needs the same CS line, SPI
/// frequency and calibration the hardware driver uses. One generator, two
/// consumers — the same rule as `board_cfg.rs`.
pub fn emit_touch_config(out: &Path, touch: &Option<HashMap<String, String>>) {
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
    fs::write(&path, code.as_bytes()).unwrap_or_else(|e| panic!("write touch_config.rs: {e}"));
}
