// SPDX-License-Identifier: GPL-3.0-only
//! Where flash regions sit, computed once from the MCU and board tomls and
//! rendered both as the linker script's `MEMORY {}` block and as Rust
//! constants — so the linker, the firmware and the size gate in
//! `scripts/lib.sh` (which repeats the same subtraction) cannot disagree
//! (docs/designs/multi-app-2026-09.md D2).
//!
//! Top-down from the end of flash:
//!
//! ```text
//! [BOOT2 (rp2040)][FLASH: program image ...][FS_FLASH][PAPK_FLASH: app region]
//! ```
//!
//! The MCU toml carries the defaults (`flash_origin`, `flash_kb`,
//! `ram_origin`, `ram_kb`, `boot2_bytes`, `fs_kb`, `app_region_kb`,
//! `max_installed_apps`); board.toml may override `fs_kb`, `app_region_kb`
//! and `max_installed_apps`. With the MCU defaults alone the result is
//! byte-identical to the linker scripts that used to hard-code it — pinned
//! by the tests at the bottom, which `platforms/rp/src/main.rs` includes.
//!
//! Region names are a contract: `FLASH` is where `build_support::freertos`
//! places `.init_array`, `PAPK_FLASH` is where `embed_papk_flash_init` links
//! the baked app, and `__fs_start`/`__fs_end` are what the family's
//! `fs_region_bounds` reads.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// One erase sector; every region length is a multiple of it.
pub const SECTOR: u64 = 4096;

/// The resolved geometry, absolute addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashLayout {
    pub flash_origin: u64,
    pub flash_len: u64,
    /// Bytes reserved for the second-stage bootloader in front of the
    /// program image (rp2040: 0x100; 0 elsewhere).
    pub boot2_bytes: u64,
    pub program_origin: u64,
    pub program_len: u64,
    pub fs_origin: u64,
    pub fs_len: u64,
    /// The app region (`PAPK_FLASH`): installed apps as self-describing runs.
    pub region_origin: u64,
    pub region_len: u64,
    /// Directory capacity; 1 means a single-app board.
    pub max_installed_apps: u64,
    pub ram_origin: u64,
    pub ram_len: u64,
}

fn int(props: &HashMap<String, String>, key: &str) -> Option<u64> {
    let raw = props.get(key)?.trim();
    let parsed = match raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => raw.parse::<u64>(),
    };
    Some(parsed.unwrap_or_else(|e| panic!("flash layout: `{key} = {raw}` is not an integer: {e}")))
}

fn required(mcu: &HashMap<String, String>, key: &str, mcu_path: &str) -> u64 {
    int(mcu, key).unwrap_or_else(|| panic!("MCU toml missing '{key}': {mcu_path}"))
}

/// A key a board may tune; the MCU toml holds the default.
fn tunable(
    mcu: &HashMap<String, String>,
    board: Option<&HashMap<String, String>>,
    key: &str,
    mcu_path: &str,
) -> u64 {
    board
        .and_then(|b| int(b, key))
        .unwrap_or_else(|| required(mcu, key, mcu_path))
}

/// Resolve the layout for `mcu` (its toml, parsed) with `board`'s overrides.
/// Panics with the offending key on any geometry that could not link or
/// that the region allocator could not use.
pub fn compute(
    mcu: &HashMap<String, String>,
    board: Option<&HashMap<String, String>>,
    mcu_path: &str,
) -> FlashLayout {
    let flash_origin = required(mcu, "flash_origin", mcu_path);
    let flash_len = required(mcu, "flash_kb", mcu_path) * 1024;
    let ram_origin = required(mcu, "ram_origin", mcu_path);
    let ram_len = required(mcu, "ram_kb", mcu_path) * 1024;
    let boot2_bytes = int(mcu, "boot2_bytes").unwrap_or(0);
    let fs_len = tunable(mcu, board, "fs_kb", mcu_path) * 1024;
    let region_len = tunable(mcu, board, "app_region_kb", mcu_path) * 1024;
    let max_installed_apps = tunable(mcu, board, "max_installed_apps", mcu_path);

    assert!(
        (1..=64).contains(&max_installed_apps),
        "flash layout: max_installed_apps must be 1..=64, got {max_installed_apps}"
    );
    for (name, len) in [
        ("flash_kb", flash_len),
        ("fs_kb", fs_len),
        ("app_region_kb", region_len),
    ] {
        assert!(
            len > 0 && len % SECTOR == 0,
            "flash layout: {name} must be a non-zero multiple of 4 KB, got {len} bytes"
        );
    }
    assert!(
        region_len >= 2 * SECTOR,
        "flash layout: app_region_kb must hold a meta sector and at least one data sector"
    );
    let region_origin = flash_origin + flash_len - region_len;
    let fs_origin = region_origin
        .checked_sub(fs_len)
        .filter(|fs| *fs > flash_origin + boot2_bytes)
        .unwrap_or_else(|| {
            panic!(
                "flash layout: fs_kb + app_region_kb ({} KB) leave no program region in {} KB of flash",
                (fs_len + region_len) / 1024,
                flash_len / 1024
            )
        });
    let program_origin = flash_origin + boot2_bytes;
    FlashLayout {
        flash_origin,
        flash_len,
        boot2_bytes,
        program_origin,
        program_len: fs_origin - program_origin,
        fs_origin,
        fs_len,
        region_origin,
        region_len,
        max_installed_apps,
        ram_origin,
        ram_len,
    }
}

/// The layout boardless host builds compile against: the RP2350 multi-app
/// geometry, so `cargo test` exercises the directory with several apps.
pub fn boardless() -> FlashLayout {
    let mcu: HashMap<String, String> = [
        ("flash_origin", "0x10000000"),
        ("flash_kb", "4096"),
        ("ram_origin", "0x20000000"),
        ("ram_kb", "520"),
        ("fs_kb", "512"),
        ("app_region_kb", "1536"),
        ("max_installed_apps", "8"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    compute(&mcu, None, "<boardless defaults>")
}

impl FlashLayout {
    /// The `MEMORY {}` block plus the filesystem symbols, to prepend to the
    /// MCU linker script's SECTIONS tail.
    pub fn render_memory_x(&self) -> String {
        let mut s = String::from(
            "/* Generated by build.rs (build_support/flash_layout.rs) from the MCU and board tomls — do not edit */\n\
             MEMORY {\n",
        );
        if self.boot2_bytes > 0 {
            s += &format!(
                "    BOOT2      : ORIGIN = {:#010x}, LENGTH = {:#x}\n",
                self.flash_origin, self.boot2_bytes
            );
        }
        s += &format!(
            "    FLASH      : ORIGIN = {:#010x}, LENGTH = {:#x}    /* program image */\n",
            self.program_origin, self.program_len
        );
        s += &format!(
            "    FS_FLASH   : ORIGIN = {:#010x}, LENGTH = {:#x}    /* LittleFS region ({} x 4KB sectors) */\n",
            self.fs_origin,
            self.fs_len,
            self.fs_len / SECTOR
        );
        s += &format!(
            "    PAPK_FLASH : ORIGIN = {:#010x}, LENGTH = {:#x}    /* app region ({} sectors, up to {} installed apps) */\n",
            self.region_origin,
            self.region_len,
            self.region_len / SECTOR,
            self.max_installed_apps
        );
        s += &format!(
            "    RAM        : ORIGIN = {:#010x}, LENGTH = {:#x}\n",
            self.ram_origin, self.ram_len
        );
        s += "}\n\n__fs_start = ORIGIN(FS_FLASH);\n__fs_end   = ORIGIN(FS_FLASH) + LENGTH(FS_FLASH);\n\n";
        s
    }

    /// The constants the firmware reads, as Rust source. Offsets are
    /// flash-relative (what a ROM erase/program routine takes); the family
    /// adds `FLASH_ORIGIN` for the mapped address. Each crate that includes
    /// the file reads a subset, hence the `allow(dead_code)`.
    pub fn rust_consts(&self) -> String {
        format!(
            "// Generated by build.rs (build_support/flash_layout.rs) from the MCU and board tomls — do not edit\n\
             /// Mapped address of the first flash byte (the XIP base).\n\
             #[allow(dead_code)]\n\
             pub const FLASH_ORIGIN: usize = {:#x};\n\
             /// Bytes the program image may occupy (the `FLASH` linker region).\n\
             #[allow(dead_code)]\n\
             pub const PROGRAM_LEN: usize = {:#x};\n\
             /// LittleFS region, flash-relative.\n\
             #[allow(dead_code)]\n\
             pub const FS_OFFSET: u32 = {:#x};\n\
             #[allow(dead_code)]\n\
             pub const FS_LEN: usize = {:#x};\n\
             /// The app region (`PAPK_FLASH`), flash-relative.\n\
             #[allow(dead_code)]\n\
             pub const PAPK_REGION_OFFSET: u32 = {:#x};\n\
             #[allow(dead_code)]\n\
             pub const PAPK_REGION_LEN: usize = {:#x};\n\
             /// Package directory capacity; 1 on a single-app board.\n\
             #[allow(dead_code)]\n\
             pub const MAX_INSTALLED_APPS: usize = {};\n",
            self.flash_origin,
            self.program_len,
            self.fs_origin - self.flash_origin,
            self.fs_len,
            self.region_origin - self.flash_origin,
            self.region_len,
            self.max_installed_apps
        )
    }
}

/// Write `OUT_DIR/flash_layout.rs` and emit the `has_multi_app` cfg.
pub fn emit(out: &Path, layout: &FlashLayout) {
    fs::write(out.join("flash_layout.rs"), layout.rust_consts())
        .unwrap_or_else(|e| panic!("write flash_layout.rs: {e}"));
    println!("cargo:rustc-check-cfg=cfg(has_multi_app)");
    if layout.max_installed_apps > 1 {
        println!("cargo:rustc-cfg=has_multi_app");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn rp2040() -> HashMap<String, String> {
        props(&[
            ("flash_origin", "0x10000000"),
            ("flash_kb", "2048"),
            ("ram_origin", "0x20000000"),
            ("ram_kb", "256"),
            ("boot2_bytes", "0x100"),
            ("fs_kb", "128"),
            ("app_region_kb", "1024"),
            ("max_installed_apps", "1"),
        ])
    }

    fn rp2350() -> HashMap<String, String> {
        props(&[
            ("flash_origin", "0x10000000"),
            ("flash_kb", "4096"),
            ("ram_origin", "0x20000000"),
            ("ram_kb", "520"),
            ("fs_kb", "256"),
            ("app_region_kb", "1024"),
            ("max_installed_apps", "1"),
        ])
    }

    /// The MCU defaults reproduce the hand-written rp2040.x exactly:
    /// FLASH 896K-0x100 @0x10000100, FS 128K @0x100E0000, slot 1M @0x10100000.
    #[test]
    fn rp2040_defaults_match_the_historical_linker_script() {
        let l = compute(&rp2040(), None, "rp2040.toml");
        assert_eq!(l.program_origin, 0x1000_0100);
        assert_eq!(l.program_len, 896 * 1024 - 0x100);
        assert_eq!(l.fs_origin, 0x100E_0000);
        assert_eq!(l.fs_len, 128 * 1024);
        assert_eq!(l.region_origin, 0x1010_0000);
        assert_eq!(l.region_len, 1024 * 1024);
        assert_eq!(l.max_installed_apps, 1);
        let x = l.render_memory_x();
        assert!(
            x.contains("BOOT2      : ORIGIN = 0x10000000, LENGTH = 0x100"),
            "{x}"
        );
        assert!(
            x.contains("FLASH      : ORIGIN = 0x10000100, LENGTH = 0xdff00"),
            "{x}"
        );
        assert!(
            x.contains("FS_FLASH   : ORIGIN = 0x100e0000, LENGTH = 0x20000"),
            "{x}"
        );
        assert!(
            x.contains("PAPK_FLASH : ORIGIN = 0x10100000, LENGTH = 0x100000"),
            "{x}"
        );
        assert!(
            x.contains("RAM        : ORIGIN = 0x20000000, LENGTH = 0x40000"),
            "{x}"
        );
        assert!(x.contains("__fs_start = ORIGIN(FS_FLASH);"));
        assert!(x.contains("__fs_end   = ORIGIN(FS_FLASH) + LENGTH(FS_FLASH);"));
    }

    /// And rp2350.x: FLASH 2816K @0x10000000, FS 256K @0x102C0000, slot 1M @0x10300000.
    #[test]
    fn rp2350_defaults_match_the_historical_linker_script() {
        let l = compute(&rp2350(), None, "rp2350.toml");
        assert_eq!(l.boot2_bytes, 0);
        assert_eq!(l.program_origin, 0x1000_0000);
        assert_eq!(l.program_len, 2816 * 1024);
        assert_eq!(l.fs_origin, 0x102C_0000);
        assert_eq!(l.region_origin, 0x1030_0000);
        let x = l.render_memory_x();
        assert!(!x.contains("BOOT2"), "{x}");
        assert!(
            x.contains("FLASH      : ORIGIN = 0x10000000, LENGTH = 0x2c0000"),
            "{x}"
        );
    }

    /// A multi-app rp2350 board: the design's D2 geometry.
    #[test]
    fn rp2350_multi_app_board_overrides_give_the_design_layout() {
        let board = props(&[
            ("fs_kb", "512"),
            ("app_region_kb", "1536"),
            ("max_installed_apps", "8"),
        ]);
        let l = compute(&rp2350(), Some(&board), "rp2350.toml");
        assert_eq!(l.program_len, 2048 * 1024);
        assert_eq!(l.fs_origin, 0x1020_0000);
        assert_eq!(l.fs_len, 512 * 1024);
        assert_eq!(l.region_origin, 0x1028_0000);
        assert_eq!(l.region_len, 1536 * 1024);
        assert_eq!(l.max_installed_apps, 8);
        assert_eq!(l.region_origin + l.region_len, 0x1040_0000);
        assert_eq!(boardless(), l, "host builds compile against this layout");
    }

    #[test]
    fn rust_consts_are_flash_relative() {
        let l = compute(&rp2040(), None, "rp2040.toml");
        let s = l.rust_consts();
        assert!(
            s.contains("pub const FLASH_ORIGIN: usize = 0x10000000;"),
            "{s}"
        );
        assert!(s.contains("pub const PROGRAM_LEN: usize = 0xdff00;"), "{s}");
        assert!(s.contains("pub const FS_OFFSET: u32 = 0xe0000;"), "{s}");
        assert!(
            s.contains("pub const PAPK_REGION_OFFSET: u32 = 0x100000;"),
            "{s}"
        );
        assert!(
            s.contains("pub const PAPK_REGION_LEN: usize = 0x100000;"),
            "{s}"
        );
        assert!(
            s.contains("pub const MAX_INSTALLED_APPS: usize = 1;"),
            "{s}"
        );
    }

    #[test]
    #[should_panic(expected = "multiple of 4 KB")]
    fn a_misaligned_region_is_refused() {
        let board = props(&[("app_region_kb", "1023")]);
        compute(&rp2350(), Some(&board), "rp2350.toml");
    }

    #[test]
    #[should_panic(expected = "leave no program region")]
    fn regions_that_swallow_the_program_image_are_refused() {
        let board = props(&[("app_region_kb", "3584"), ("fs_kb", "512")]);
        compute(&rp2350(), Some(&board), "rp2350.toml");
    }

    #[test]
    #[should_panic(expected = "max_installed_apps")]
    fn a_zero_directory_is_refused() {
        let board = props(&[("max_installed_apps", "0")]);
        compute(&rp2350(), Some(&board), "rp2350.toml");
    }
}
