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

/// Board configuration: flat key-value properties plus peripheral declarations.
#[derive(Debug)]
pub struct BoardConfig {
    pub props: HashMap<String, String>,
    pub sensors: Vec<SensorDecl>,
    pub display: Option<HashMap<String, String>>,
    pub touch: Option<HashMap<String, String>>,
}

const KNOWN_SENSOR_KINDS: &[&str] = &["bme688"];

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

/// Current section being parsed in board.toml.
#[derive(PartialEq)]
enum Section {
    Top,
    Display,
    Touch,
    Sensor,
    Unknown,
}

/// Parse a board TOML file with flat properties, `[display]`, `[touch]`,
/// and `[[sensor]]` sections.
pub fn parse_board_toml(path: &str) -> BoardConfig {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));
    let mut props = HashMap::new();
    let mut sensors: Vec<SensorDecl> = Vec::new();
    let mut display: Option<HashMap<String, String>> = None;
    let mut touch: Option<HashMap<String, String>> = None;
    let mut section = Section::Top;
    let mut cur_sensor: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section headers
        if trimmed == "[[sensor]]" {
            if section == Section::Sensor {
                sensors.push(finish_sensor(&cur_sensor, path));
                cur_sensor.clear();
            }
            section = Section::Sensor;
            continue;
        }
        if trimmed == "[display]" {
            if section == Section::Sensor {
                sensors.push(finish_sensor(&cur_sensor, path));
                cur_sensor.clear();
            }
            display = Some(HashMap::new());
            section = Section::Display;
            continue;
        }
        if trimmed == "[touch]" {
            if section == Section::Sensor {
                sensors.push(finish_sensor(&cur_sensor, path));
                cur_sensor.clear();
            }
            touch = Some(HashMap::new());
            section = Section::Touch;
            continue;
        }
        if trimmed.starts_with('[') {
            if section == Section::Sensor {
                sensors.push(finish_sensor(&cur_sensor, path));
                cur_sensor.clear();
            }
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
                Section::Display => {
                    display.as_mut().unwrap().insert(key, val);
                }
                Section::Touch => {
                    touch.as_mut().unwrap().insert(key, val);
                }
                Section::Unknown => {}
            }
        }
    }
    if section == Section::Sensor {
        sensors.push(finish_sensor(&cur_sensor, path));
    }
    BoardConfig {
        props,
        sensors,
        display,
        touch,
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

/// Resolve the active board name from Cargo feature flags.
/// Scans CARGO_FEATURE_BOARD_* env vars set by Cargo.
pub fn resolve_active_board() -> Option<String> {
    const BOARDS: &[&str] = &[
        "testbench_rp2040",
        "testbench_rp2350",
        "testbench_rp2350w",
        "pico_enviro_mon",
    ];
    for board in BOARDS {
        let feature = format!("CARGO_FEATURE_BOARD_{}", board.to_uppercase());
        if env::var(&feature).is_ok() {
            return Some(board.to_string());
        }
    }
    None
}

/// Resolve the active MCU name from Cargo feature flags.
/// Falls back to the board.toml `mcu` field if no chip feature is active.
pub fn resolve_active_mcu(board_cfg: &HashMap<String, String>) -> String {
    if env::var("CARGO_FEATURE_CHIP_RP2040").is_ok() {
        return "rp2040".to_string();
    }
    if env::var("CARGO_FEATURE_CHIP_RP2350").is_ok()
        || env::var("CARGO_FEATURE_CHIP_RP2350_HAL").is_ok()
    {
        return "rp2350".to_string();
    }
    board_cfg
        .get("mcu")
        .cloned()
        .expect("Cannot determine MCU: no chip feature active and board.toml has no 'mcu' key")
}

/// Find the MCU .toml file by searching mcus/<family>/<name>.toml.
pub fn find_mcu_toml(mcu_name: &str) -> String {
    let mcus_dir = Path::new("mcus");
    if let Ok(entries) = fs::read_dir(mcus_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let candidate = entry.path().join(format!("{mcu_name}.toml"));
                if candidate.exists() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
    }
    panic!("MCU definition not found: mcus/*/{mcu_name}.toml");
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
