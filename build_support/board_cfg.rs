// SPDX-License-Identifier: GPL-3.0-only
//! Board-derived code generation shared by every crate that needs it.
//!
//! Both `platforms/<family>/build.rs` and `picodroid-core/build.rs` pull this
//! module in via `#[path]`, so each `board.toml` key is parsed and emitted by
//! exactly one implementation. That matters: the 2026-05 partial extraction
//! left two copies of `dispatch_sites`/`task_priority` behind and they
//! silently diverged for three months (cleaned up in `fc896b3`). Generated
//! config is the same hazard, so there is one generator and two callers.
//!
//! Split of responsibilities:
//!
//! * **Neutral** (this module, emitted into `picodroid-core`'s OUT_DIR and
//!   also into the platform's while its code still reads them): JVM sizing,
//!   sleep timings, sensor table, background pool, handle table, button
//!   table, display *dimensions*, and the board-capability cfgs.
//! * **Family-owned** (platform `build.rs` only): pin-bearing display/touch
//!   config, `memory.x`, the C builds, APK embedding.
//!
//! See `docs/designs/shared-core-extraction.md` §3.D.

use crate::config;
use crate::jvm_defaults;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The active board plus the paths needed to read everything it references.
pub struct ResolvedBoard {
    pub name: String,
    /// `…/platforms/<family>` — the crate that owns this board's MCU defs.
    pub platform_root: PathBuf,
    pub cfg: config::BoardConfig,
}

impl ResolvedBoard {
    /// Parse the MCU toml this board declares.
    pub fn mcu(&self) -> (String, HashMap<String, String>) {
        let mcu_name = config::resolve_active_mcu(&self.cfg.props);
        let path = config::find_mcu_toml_in(&self.platform_root, &mcu_name);
        println!("cargo:rerun-if-changed={path}");
        let parsed = config::parse_toml(&path);
        (path, parsed)
    }
}

/// Framework classes this board opts out of embedding, from the optional
/// top-level `framework_class_excludes` key (a `;`- or `,`-separated list of
/// JVM internal names, e.g. `picodroid/json/JSONObject`).
///
/// The SDK ships whole on every board and is loaded at boot, so each class
/// costs its `.class` size in flash everywhere — the RP2040's program region
/// has no room to spare. Excluding a class here keeps it off boards that
/// cannot afford it; apps built for such a board must not reference it.
/// Empty for boardless builds and for boards that set nothing.
pub fn framework_class_excludes(board: &Option<ResolvedBoard>) -> Vec<String> {
    board
        .as_ref()
        .and_then(|b| b.cfg.props.get("framework_class_excludes"))
        .map(|v| config::parse_str_list(v))
        .unwrap_or_default()
}

/// Resolve the active board for a crate whose manifest lives at
/// `manifest_dir`, searching across platform families. `None` for boardless
/// builds (bare `cargo build -p picodroid-core`, `cargo test` with no board
/// feature), where every generator below falls back to safe defaults.
pub fn resolve(manifest_dir: &Path) -> Option<ResolvedBoard> {
    let (name, dir) = config::resolve_active_board_dir(manifest_dir)?;
    let toml = dir.join("board.toml");
    println!("cargo:rerun-if-changed={}", toml.display());
    let cfg = config::parse_board_toml(&toml.to_string_lossy());
    let platform_root = dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| {
            panic!(
                "board dir must be <platform>/boards/<name>: {}",
                dir.display()
            )
        })
        .to_path_buf();
    Some(ResolvedBoard {
        name,
        platform_root,
        cfg,
    })
}

/// Emit every board-capability cfg plus the neutral generated files.
///
/// `pins` selects whether the caller also owns the pin-bearing display/touch
/// artifacts: platform crates emit those themselves (and their own
/// `has_display`/`has_touch`), so they pass `Pins::Owned` to avoid emitting
/// the cfg twice.
pub fn emit_neutral(out: &Path, board: &Option<ResolvedBoard>, pins: Pins) {
    emit_heap_config(out, board);
    emit_network_cfgs(board);
    emit_sensor_config(out, board);
    emit_button_config(out, board);
    emit_background_pool_config(out, board);
    emit_sleep_config(out, board);
    emit_jvm_state_config(out, board);
    emit_handle_table_config(out, board);
    if pins == Pins::Elsewhere {
        emit_display_dims(out, board);
        emit_touch_cfg(board);
    }
}

/// Whether the calling crate owns the pin-bearing display/touch artifacts.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Pins {
    /// Caller emits `display_config.rs` / `touch_config.rs` itself
    /// (platform crates wiring their own HAL).
    Owned,
    /// Caller only needs dimensions and capability cfgs (shared crates).
    Elsewhere,
}

fn props(board: &Option<ResolvedBoard>) -> Option<&HashMap<String, String>> {
    board.as_ref().map(|b| &b.cfg.props)
}

fn write_generated(out: &Path, name: &str, body: &str) {
    let path = out.join(name);
    let mut f = File::create(&path).unwrap_or_else(|e| panic!("create {name}: {e}"));
    f.write_all(body.as_bytes())
        .unwrap_or_else(|e| panic!("write {name}: {e}"));
}

/// Parse `heap_kb` from an MCU toml (the single source for the FreeRTOS
/// arena size — docs/parity-audit.md M2).
pub fn mcu_heap_kb(mcu: &HashMap<String, String>, mcu_toml_path: &str) -> u32 {
    mcu.get("heap_kb")
        .unwrap_or_else(|| panic!("MCU toml missing 'heap_kb': {mcu_toml_path}"))
        .parse()
        .unwrap_or_else(|e| panic!("MCU toml 'heap_kb' not a number ({mcu_toml_path}): {e}"))
}

/// Emit `OUT_DIR/heap_config.rs` with the device heap arena size for the
/// active board's MCU — consumed by the simulator's default heap cap
/// (`sim_allocator.rs`) and by `mem_diag`, keeping it single-sourced with the
/// FreeRTOS C build's `configTOTAL_HEAP_SIZE` (docs/parity-audit.md M2).
/// Falls back to the RP2350 value for boardless host builds, where the
/// constant is compiled but the cap never enforces.
pub fn emit_heap_config(out: &Path, board: &Option<ResolvedBoard>) {
    let heap_bytes = match board {
        Some(b) => {
            let (path, mcu) = b.mcu();
            mcu_heap_kb(&mcu, &path) as usize * 1024
        }
        None => 416 * 1024,
    };
    write_generated(
        out,
        "heap_config.rs",
        &format!(
            "// Generated by build.rs from mcus/<family>/<mcu>.toml `heap_kb` — do not edit\n\
             pub const DEVICE_HEAP_BYTES: usize = {heap_bytes};\n"
        ),
    );
}

/// Emit `has_network` / `network_<type>` rustc cfgs from board.toml.
/// board.toml is the single source of truth; Rust code gates on these cfgs
/// rather than on Cargo features.
pub fn emit_network_cfgs(board: &Option<ResolvedBoard>) {
    const KNOWN_NETWORK_TYPES: &[&str] = &["cyw43"];

    println!("cargo:rustc-check-cfg=cfg(has_network)");
    for t in KNOWN_NETWORK_TYPES {
        println!("cargo:rustc-check-cfg=cfg(network_{t})");
    }

    let Some(p) = props(board) else { return };
    if p.get("has_network").map(String::as_str) != Some("true") {
        return;
    }
    println!("cargo:rustc-cfg=has_network");

    if let Some(t) = p.get("network_type") {
        if !KNOWN_NETWORK_TYPES.contains(&t.as_str()) {
            panic!("board.toml: unknown network_type '{t}' (known: {KNOWN_NETWORK_TYPES:?})");
        }
        println!("cargo:rustc-cfg=network_{t}");
    }
}

/// Emit `OUT_DIR/sensor_table.rs` plus the `sensor_<kind>` / `any_sensor`
/// cfgs from board.toml `[[sensor]]` entries.
pub fn emit_sensor_config(out: &Path, board: &Option<ResolvedBoard>) {
    println!("cargo:rustc-check-cfg=cfg(any_sensor)");
    println!("cargo:rustc-check-cfg=cfg(sensor_bme688)");
    println!("cargo:rustc-check-cfg=cfg(sensor_ltr559)");

    let empty: Vec<config::SensorDecl> = Vec::new();
    let sensors = board.as_ref().map(|b| &b.cfg.sensors).unwrap_or(&empty);

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

    write_generated(out, "sensor_table.rs", &code);
}

/// Emit `OUT_DIR/button_config.rs` (table of `(pin, LV_KEY_*, keycode)`) plus
/// the `has_buttons` cfg.
///
/// The pin stays in the shared table on purpose: it is the join key between
/// `GpioEvent.pin` and Android keycodes, the sim's keyboard emulation reads
/// the same table, and button init already speaks only contract GPIO. Pins
/// are opaque `u8`s to shared code. See design §3.D.
pub fn emit_button_config(out: &Path, board: &Option<ResolvedBoard>) {
    println!("cargo:rustc-check-cfg=cfg(has_buttons)");

    let empty: Vec<config::ButtonDecl> = Vec::new();
    let buttons = board.as_ref().map(|b| &b.cfg.buttons).unwrap_or(&empty);

    if !buttons.is_empty() {
        println!("cargo:rustc-cfg=has_buttons");
    }

    let mut code = String::from("// Generated by build.rs — do not edit\n\n");
    // Referenced where `use crate::lvgl_ffi::*;` is in scope (LV_KEY_*).
    code.push_str("pub const BUTTONS: &[(u8, u32, i32)] = &[\n");
    for b in buttons {
        code.push_str(&format!(
            "    ({}, LV_KEY_{}, {}),\n",
            b.pin, b.lv_key, b.keycode
        ));
    }
    code.push_str("];\n");

    write_generated(out, "button_config.rs", &code);
}

/// Emit background thread pool constants from board.toml's optional
/// `[background_pool]` section. Missing keys fall back to defaults
/// (4 workers, the JVM priority tier, 4 KiB stack, 32-deep queue).
pub fn emit_background_pool_config(out: &Path, board: &Option<ResolvedBoard>) {
    const DEFAULT_THREADS: u32 = 4;
    // `picodroid_core::task_priority::PRIORITY_JVM_NORM`. Every task that
    // interprets Java must share one priority or the lock-free shared heap
    // is unsound (a `task_priority` test cross-checks the two numbers).
    const JVM_TIER: u32 = 15;
    const DEFAULT_PRIORITY: u32 = JVM_TIER;
    const DEFAULT_STACK_BYTES: u32 = 4096;
    const DEFAULT_QUEUE_DEPTH: u32 = 32;

    let pool = board.as_ref().and_then(|b| b.cfg.background_pool.as_ref());
    let read = |key: &str, default: u32| -> u32 {
        pool.and_then(|p| p.get(key))
            .map(|v| {
                v.parse::<u32>()
                    .unwrap_or_else(|_| panic!("[background_pool] {key}: int"))
            })
            .unwrap_or(default)
    };

    let threads = read("threads", DEFAULT_THREADS);
    let priority = read("priority", DEFAULT_PRIORITY);
    let stack_bytes = read("stack_bytes", DEFAULT_STACK_BYTES);
    let queue_depth = read("queue_depth", DEFAULT_QUEUE_DEPTH);

    assert!(
        priority == JVM_TIER,
        "[background_pool] priority must be {JVM_TIER}: every task that interprets Java \
         shares one FreeRTOS priority (docs/parity-audit.md THR-06), got {priority}"
    );
    assert!(
        (1..=32).contains(&threads),
        "[background_pool] threads must be in 1..=32, got {threads}"
    );

    write_generated(
        out,
        "background_pool_config.rs",
        &format!(
            "// Generated by build.rs — do not edit\n\n\
             pub const POOL_THREADS: u32 = {threads};\n\
             pub const POOL_PRIORITY: u8 = {priority};\n\
             pub const POOL_STACK_BYTES: u32 = {stack_bytes};\n\
             pub const POOL_QUEUE_DEPTH: u32 = {queue_depth};\n"
        ),
    );
}

/// Emit `OUT_DIR/sleep_config.rs` from the optional top-level
/// `idle_timeout_ms` key in board.toml. `0` disables sleep (`None`); an
/// absent key falls back to 60_000 ms. Consumed by `lifecycle.rs`.
pub fn emit_sleep_config(out: &Path, board: &Option<ResolvedBoard>) {
    const DEFAULT_IDLE_TIMEOUT_MS: u64 = 60_000;
    let ms = props(board)
        .and_then(|p| p.get("idle_timeout_ms"))
        .map(|v| v.parse::<u64>().expect("idle_timeout_ms: int"))
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_MS);
    let value = if ms == 0 {
        "None".to_string()
    } else {
        format!("Some({ms})")
    };
    write_generated(
        out,
        "sleep_config.rs",
        &format!(
            "// Generated by build.rs — do not edit\n\
             pub const IDLE_TIMEOUT_MS: Option<u64> = {value};\n"
        ),
    );
}

/// Emit `OUT_DIR/handle_table_config.rs` from the optional top-level
/// `handle_slots` key: the LVGL widget handle table's slot count
/// (concurrently-live widgets, not cumulative). Absent → 256. Must be a
/// power of two in 32..=4096 (the table decodes the slot index by mask).
pub fn emit_handle_table_config(out: &Path, board: &Option<ResolvedBoard>) {
    const DEFAULT_HANDLE_SLOTS: usize = 256;
    let slots = props(board)
        .and_then(|p| p.get("handle_slots"))
        .map(|v| v.parse::<usize>().expect("handle_slots: int"))
        .unwrap_or(DEFAULT_HANDLE_SLOTS);
    assert!(
        slots.is_power_of_two() && (32..=4096).contains(&slots),
        "handle_slots = {slots}: must be a power of two in 32..=4096"
    );
    write_generated(
        out,
        "handle_table_config.rs",
        &format!(
            "// Generated by build.rs — do not edit\n\
             pub const HANDLE_SLOTS: usize = {slots};\n"
        ),
    );
}

/// Emit the JVM tunables that become `pub const`s:
/// `jvm_state_config.rs` (activity stack + pending-op queue depth) and
/// `jvm_prereserve_config.rs` (boot-time heap pre-reservation).
///
/// Canonical guide: `website/src/content/docs/reference/jvm-tunables.md`.
/// The three JVM-*crate* knobs are emitted as `cargo:rustc-env` by
/// [`emit_jvm_env_vars`], which only the platform crate calls.
pub fn emit_jvm_state_config(out: &Path, board: &Option<ResolvedBoard>) {
    let jvm = board.as_ref().and_then(|b| b.cfg.jvm.as_ref());

    let activity_stack_depth = read_jvm_u32(
        jvm,
        "activity_stack_depth",
        &jvm_defaults::ACTIVITY_STACK_DEPTH,
    );
    let pending_op_queue = read_jvm_u32(jvm, "pending_op_queue", &jvm_defaults::PENDING_OP_QUEUE);

    write_generated(
        out,
        "jvm_state_config.rs",
        &format!(
            "// Generated by build.rs — do not edit.\n\n\
             pub const ACTIVITY_STACK_DEPTH: usize = {activity_stack_depth};\n\
             pub const PENDING_OP_QUEUE_DEPTH: usize = {pending_op_queue};\n"
        ),
    );

    let obj = read_jvm_u32(
        jvm,
        "prereserve_obj_chunks",
        &jvm_defaults::PRERESERVE_OBJ_CHUNKS,
    );
    let arr = read_jvm_u32(
        jvm,
        "prereserve_arr_chunks",
        &jvm_defaults::PRERESERVE_ARR_CHUNKS,
    );
    let str_chunks = read_jvm_u32(
        jvm,
        "prereserve_str_chunks",
        &jvm_defaults::PRERESERVE_STR_CHUNKS,
    );
    let fields = read_jvm_u32(
        jvm,
        "prereserve_fields_values",
        &jvm_defaults::PRERESERVE_FIELDS_VALUES,
    );
    let arena = read_jvm_u32(
        jvm,
        "prereserve_arena_values",
        &jvm_defaults::PRERESERVE_ARENA_VALUES,
    );
    let arena8 = read_jvm_u32(
        jvm,
        "prereserve_arena8_bytes",
        &jvm_defaults::PRERESERVE_ARENA8_BYTES,
    );

    write_generated(
        out,
        "jvm_prereserve_config.rs",
        &format!(
            "// Generated by build.rs from board.toml [jvm] — do not edit.\n\n\
             pub const PRERESERVE_OBJ_CHUNKS: usize = {obj};\n\
             pub const PRERESERVE_ARR_CHUNKS: usize = {arr};\n\
             pub const PRERESERVE_STR_CHUNKS: usize = {str_chunks};\n\
             pub const PRERESERVE_FIELDS_VALUES: usize = {fields};\n\
             pub const PRERESERVE_ARENA_VALUES: usize = {arena};\n\
             pub const PRERESERVE_ARENA8_BYTES: usize = {arena8};\n"
        ),
    );
}

/// Emit `cargo:rustc-env=PICODROID_JVM_*` for the three knobs that belong to
/// the `pico-jvm` crate, so a direct `cargo build` against a board (without
/// going through `scripts/lib.sh::resolve_board`) still picks them up.
///
/// Only the platform crate calls this: the JVM crate is compiled before its
/// dependents, so these affect the *next* build. The wrapper scripts are the
/// primary path; this is a safety net for direct cargo users.
pub fn emit_jvm_env_vars(board: &Option<ResolvedBoard>) {
    let jvm = board.as_ref().and_then(|b| b.cfg.jvm.as_ref());
    let gc = read_jvm_u32(jvm, "gc_alloc_threshold", &jvm_defaults::GC_ALLOC_THRESHOLD);
    let shift = read_jvm_u32(jvm, "slot_chunk_shift", &jvm_defaults::SLOT_CHUNK_SHIFT);
    let inline = read_jvm_u32(jvm, "inline_array_data", &jvm_defaults::INLINE_ARRAY_DATA);
    println!("cargo:rustc-env=PICODROID_JVM_GC_ALLOC_THRESHOLD={gc}");
    println!("cargo:rustc-env=PICODROID_JVM_SLOT_CHUNK_SHIFT={shift}");
    println!("cargo:rustc-env=PICODROID_JVM_INLINE_ARRAY_DATA={inline}");
}

/// Read an integer key from the optional `[jvm]` table. Missing → default;
/// non-integer or out-of-range → panic citing the field. Default and range
/// come from `build_support/jvm_defaults.rs`, which `jvm/build.rs` also
/// imports — drift between the crates is impossible.
fn read_jvm_u32(
    jvm: Option<&HashMap<String, String>>,
    key: &str,
    spec: &jvm_defaults::JvmTunable,
) -> u32 {
    let Some(raw) = jvm.and_then(|t| t.get(key)) else {
        return spec.default;
    };
    let v: u32 = raw
        .parse()
        .unwrap_or_else(|_| panic!("[jvm] {key} = {raw:?}: expected u32"));
    if v < spec.min || v > spec.max {
        panic!(
            "[jvm] {key} = {v}: out of range (allowed {min}..={max})",
            min = spec.min,
            max = spec.max,
        );
    }
    v
}

/// Emit `OUT_DIR/display_dims.rs` — the four board-neutral display constants,
/// without any of the pin/SPI wiring that `config::emit_display_config`
/// carries. Shared crates need the geometry (band buffer sizing, coordinate
/// clamping); only the family HAL needs the pins.
pub fn emit_display_dims(out: &Path, board: &Option<ResolvedBoard>) {
    println!("cargo:rustc-check-cfg=cfg(has_display)");

    let display = board.as_ref().and_then(|b| b.cfg.display.as_ref());
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
    } else {
        // Must match config::emit_display_config's boardless defaults.
        code.push_str("pub const SCREEN_WIDTH: u16 = 320;\n");
        code.push_str("pub const SCREEN_HEIGHT: u16 = 240;\n");
        code.push_str("pub const BAND_HEIGHT: usize = 20;\n");
        code.push_str("pub const SCROLL_LIMIT: u8 = 30;\n");
    }

    write_generated(out, "display_dims.rs", &code);
}

/// Emit the `has_touch` cfg without the pin-bearing `touch_config.rs`.
pub fn emit_touch_cfg(board: &Option<ResolvedBoard>) {
    println!("cargo:rustc-check-cfg=cfg(has_touch)");
    if board.as_ref().and_then(|b| b.cfg.touch.as_ref()).is_some() {
        println!("cargo:rustc-cfg=has_touch");
    }
}

/// Cross-check board.toml capabilities against the Cargo features the binary
/// crate forwarded, so a board that gains a sensor without the matching
/// `picodroid-core/sensor-*` forwarding fails the build instead of silently
/// compiling the driver out.
///
/// Only meaningful in the shared crate (the platform crate is the one doing
/// the forwarding). Skipped entirely for boardless builds.
pub fn assert_forwarded_features_match(board: &Option<ResolvedBoard>) {
    let Some(b) = board else { return };

    let check = |kind: &str, from_board: bool, feature: &str| {
        let from_feature = std::env::var(format!(
            "CARGO_FEATURE_{}",
            feature.to_uppercase().replace('-', "_")
        ))
        .is_ok();
        if from_board != from_feature {
            panic!(
                "board '{board}' declares {kind}={from_board} but picodroid-core \
                 feature '{feature}' is {state}.\n\
                 Fix the forwarding in platforms/<family>/Cargo.toml: the board-* \
                 feature must enable picodroid-core/{feature}.",
                board = b.name,
                state = if from_feature { "on" } else { "off" },
            );
        }
    };

    for kind in ["bme688", "ltr559"] {
        let declared = b.cfg.sensors.iter().any(|s| s.kind == kind);
        check(kind, declared, &format!("sensor-{kind}"));
    }

    let net = b.cfg.props.get("network_type").map(String::as_str);
    check("network_type=cyw43", net == Some("cyw43"), "network-cyw43");
}
