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
//! and `max_installed_apps`, and may name a `boot_package` (D7). With the
//! MCU defaults alone the result is
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
    /// board.toml `boot_package`: the package that boots when it is
    /// installed (docs/designs/multi-app-2026-09.md D7). `None` when the
    /// board names nothing.
    pub boot_package: Option<String>,
    /// Bytes of the volume the apps may not eat into (`fs_system_reserve_kb`,
    /// docs/designs/multi-app-2026-09.md D10).
    pub fs_system_reserve: u64,
    /// Bytes one app's data directory may hold (`app_data_cap_kb`); 0 = no cap.
    pub app_data_cap: u64,
    pub ram_origin: u64,
    pub ram_len: u64,
    /// The module's QSPI PSRAM behind the second chip select, when the MCU
    /// toml declares one (`psram_kb`, `psram_origin`); both 0 otherwise.
    /// Geometry only: which tenant lives there is board policy
    /// (`lv_mem_in_psram`), and nothing is linked into the region — its first
    /// tenant takes the address as a constant
    /// (docs/designs/psram-lvgl-fluid-scroll-2026-09.md §3).
    pub psram_origin: u64,
    pub psram_len: u64,
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

/// A key a board may tune and the MCU may default; `default` otherwise.
fn tunable_or(
    mcu: &HashMap<String, String>,
    board: Option<&HashMap<String, String>>,
    key: &str,
    default: u64,
) -> u64 {
    board
        .and_then(|b| int(b, key))
        .or_else(|| int(mcu, key))
        .unwrap_or(default)
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
    let boot_package = board
        .and_then(|b| b.get("boot_package"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // The storage policy (D10): a reserve the apps may not eat into, and a
    // cap on one app's directory — a quarter of the volume unless set.
    let fs_system_reserve = tunable_or(mcu, board, "fs_system_reserve_kb", 64) * 1024;
    let app_data_cap = tunable_or(mcu, board, "app_data_cap_kb", fs_len / 4096) * 1024;

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
    assert!(
        fs_system_reserve < fs_len,
        "flash layout: fs_system_reserve_kb ({} KB) must be smaller than fs_kb ({} KB)",
        fs_system_reserve / 1024,
        fs_len / 1024
    );
    assert!(
        app_data_cap == 0 || app_data_cap + fs_system_reserve <= fs_len,
        "flash layout: app_data_cap_kb ({} KB) plus fs_system_reserve_kb ({} KB) exceed fs_kb ({} KB)",
        app_data_cap / 1024,
        fs_system_reserve / 1024,
        fs_len / 1024
    );
    let psram_len = int(mcu, "psram_kb").unwrap_or(0) * 1024;
    let psram_origin = if psram_len > 0 {
        required(mcu, "psram_origin", mcu_path)
    } else {
        0
    };
    assert!(
        psram_len.is_multiple_of(SECTOR),
        "flash layout: psram_kb must be a multiple of 4 KB, got {psram_len} bytes"
    );
    assert!(
        psram_len == 0 || psram_origin >= flash_origin + flash_len,
        "flash layout: psram_origin {psram_origin:#x} overlaps the flash window ending at {:#x}",
        flash_origin + flash_len
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
        boot_package,
        fs_system_reserve,
        app_data_cap,
        ram_origin,
        ram_len,
        psram_origin,
        psram_len,
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
        if self.psram_len > 0 {
            s += &format!(
                "    PSRAM      : ORIGIN = {:#010x}, LENGTH = {:#x}    /* QSPI PSRAM, XIP window 1; nothing is placed here */\n",
                self.psram_origin, self.psram_len
            );
        }
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
             pub const MAX_INSTALLED_APPS: usize = {};\n\
             /// board.toml `boot_package`, when the board names one (D7).\n\
             #[allow(dead_code)]\n\
             pub const BOOT_PACKAGE: Option<&str> = {};\n\
             /// Bytes of the volume kept for the system: an app's write that would dip\n\
             /// below it is refused (`fs_system_reserve_kb`, D10).\n\
             #[allow(dead_code)]\n\
             pub const FS_SYSTEM_RESERVE_BYTES: usize = {};\n\
             /// Bytes one app's data directory may hold; 0 = unlimited (`app_data_cap_kb`, D10).\n\
             #[allow(dead_code)]\n\
             pub const APP_DATA_CAP_BYTES: usize = {};\n\
             /// The QSPI PSRAM window (`psram_kb`, `psram_origin`); both 0 on a module without one.\n\
             #[allow(dead_code)]\n\
             pub const PSRAM_ORIGIN: usize = {:#x};\n\
             #[allow(dead_code)]\n\
             pub const PSRAM_LEN: usize = {:#x};\n",
            self.flash_origin,
            self.program_len,
            self.fs_origin - self.flash_origin,
            self.fs_len,
            self.region_origin - self.flash_origin,
            self.region_len,
            self.max_installed_apps,
            match &self.boot_package {
                Some(p) => format!("Some({p:?})"),
                None => "None".to_string(),
            },
            self.fs_system_reserve,
            self.app_data_cap,
            self.psram_origin,
            self.psram_len,
        )
    }
}

/// Write `OUT_DIR/flash_layout.rs` and emit the `has_multi_app` and
/// `has_psram` cfgs.
pub fn emit(out: &Path, layout: &FlashLayout) {
    fs::write(out.join("flash_layout.rs"), layout.rust_consts())
        .unwrap_or_else(|e| panic!("write flash_layout.rs: {e}"));
    println!("cargo:rustc-check-cfg=cfg(has_multi_app)");
    if layout.max_installed_apps > 1 {
        println!("cargo:rustc-cfg=has_multi_app");
    }
    println!("cargo:rustc-check-cfg=cfg(has_psram)");
    if layout.psram_len > 0 {
        println!("cargo:rustc-cfg=has_psram");
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

    /// The RP2350B module: 16 MB of flash and 8 MB of PSRAM on chip select 1.
    fn rp2350b() -> HashMap<String, String> {
        let mut m = rp2350();
        m.insert("flash_kb".into(), "16384".into());
        m.insert("psram_kb".into(), "8192".into());
        m.insert("psram_origin".into(), "0x11000000".into());
        m
    }

    #[test]
    fn a_module_with_psram_gets_a_region_and_the_constants() {
        let l = compute(&rp2350b(), None, "rp2350b.toml");
        assert_eq!(l.psram_origin, 0x1100_0000);
        assert_eq!(l.psram_len, 8 * 1024 * 1024);
        let x = l.render_memory_x();
        assert!(
            x.contains("PSRAM      : ORIGIN = 0x11000000, LENGTH = 0x800000"),
            "{x}"
        );
        let c = l.rust_consts();
        assert!(
            c.contains("pub const PSRAM_ORIGIN: usize = 0x11000000;"),
            "{c}"
        );
        assert!(c.contains("pub const PSRAM_LEN: usize = 0x800000;"), "{c}");
    }

    #[test]
    fn a_module_without_psram_renders_no_region() {
        let l = compute(&rp2350(), None, "rp2350.toml");
        assert_eq!((l.psram_origin, l.psram_len), (0, 0));
        assert!(!l.render_memory_x().contains("PSRAM"));
        assert!(l
            .rust_consts()
            .contains("pub const PSRAM_LEN: usize = 0x0;"));
    }

    #[test]
    #[should_panic(expected = "psram_origin")]
    fn psram_without_an_origin_is_refused() {
        let mut m = rp2350();
        m.insert("psram_kb".into(), "8192".into());
        compute(&m, None, "rp2350.toml");
    }

    #[test]
    #[should_panic(expected = "overlaps the flash window")]
    fn psram_inside_the_flash_window_is_refused() {
        let mut m = rp2350b();
        m.insert("psram_origin".into(), "0x10800000".into());
        compute(&m, None, "rp2350b.toml");
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
    fn boot_package_is_optional_and_passes_through() {
        let l = compute(&rp2350(), None, "rp2350.toml");
        assert_eq!(l.boot_package, None);
        assert!(
            l.rust_consts()
                .contains("pub const BOOT_PACKAGE: Option<&str> = None;"),
            "{}",
            l.rust_consts()
        );
        let board = props(&[("boot_package", "com.example.kiosk")]);
        let l = compute(&rp2350(), Some(&board), "rp2350.toml");
        assert_eq!(l.boot_package.as_deref(), Some("com.example.kiosk"));
        assert!(l
            .rust_consts()
            .contains("pub const BOOT_PACKAGE: Option<&str> = Some(\"com.example.kiosk\");"));
        // A blank value is the same as no value.
        let blank = props(&[("boot_package", "  ")]);
        assert_eq!(
            compute(&rp2350(), Some(&blank), "rp2350.toml").boot_package,
            None
        );
    }

    #[test]
    fn storage_policy_keys_default_and_override() {
        // Defaults: a 64 KB reserve, a cap of a quarter of the volume.
        let l = compute(&rp2350(), None, "rp2350.toml");
        assert_eq!(l.fs_system_reserve, 64 * 1024);
        assert_eq!(l.app_data_cap, 256 * 1024 / 4);
        let consts = l.rust_consts();
        assert!(consts.contains("pub const FS_SYSTEM_RESERVE_BYTES: usize = 65536;"));
        assert!(consts.contains("pub const APP_DATA_CAP_BYTES: usize = 65536;"));
        // The board may size the volume and set both; 0 lifts the cap.
        let board = props(&[
            ("fs_kb", "512"),
            ("fs_system_reserve_kb", "32"),
            ("app_data_cap_kb", "0"),
        ]);
        let l = compute(&rp2350(), Some(&board), "rp2350.toml");
        assert_eq!(l.fs_system_reserve, 32 * 1024);
        assert_eq!(l.app_data_cap, 0);
        // A bigger volume moves the default cap with it.
        let board = props(&[("fs_kb", "512")]);
        assert_eq!(
            compute(&rp2350(), Some(&board), "rp2350.toml").app_data_cap,
            128 * 1024
        );
    }

    #[test]
    #[should_panic(expected = "fs_system_reserve_kb")]
    fn a_reserve_the_size_of_the_volume_is_refused() {
        let board = props(&[("fs_system_reserve_kb", "256")]);
        compute(&rp2350(), Some(&board), "rp2350.toml");
    }

    #[test]
    #[should_panic(expected = "app_data_cap_kb")]
    fn a_cap_past_the_volume_is_refused() {
        let board = props(&[("app_data_cap_kb", "200")]);
        compute(&rp2350(), Some(&board), "rp2350.toml");
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
