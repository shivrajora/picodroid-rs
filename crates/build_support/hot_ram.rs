// SPDX-License-Identifier: GPL-3.0-only
//! Hot code into SRAM, per board (docs/designs/sram-hotpath-2026-09.md).
//!
//! On a chip that executes from external flash through a small cache
//! (the RP2350's 16 KB XIP cache against a UI working set several times
//! that), the functions a page turn runs most are fetched from flash on
//! nearly every call. A board opts in with `hot_ram_kb = N` in its
//! board.toml, which does three things at once:
//!
//! - the Rust side: the chip's Cargo feature forwards `pico-jvm/hot-in-ram`,
//!   which tags the interpreter's invoke, frame, field and constant-pool
//!   helpers with `link_section = ".data.hot"` (the platform build asserts
//!   the key and the feature agree, as it does for `jvm_loop_ram_kb`);
//! - the C side: [`retarget`] renames the `.text.<fn>` section of every
//!   function named in the family's `hot-ram-lvgl.txt` and
//!   `hot-ram-freertos.txt` (next to the MCU tomls) to `.data.hot.<fn>` in
//!   the built archive, so no LVGL or FreeRTOS source changes;
//! - the arena: `N` KB come out of the FreeRTOS heap the device links
//!   (`board_cfg::mcu_arena_kb`), exactly as `jvm_loop_ram_kb` does for the
//!   loop, since `.data` grows by the code's size.
//!
//! `cortex-m-rt`'s `.data` output section collects `*(.data .data.*)`, is
//! copied from flash at reset and already holds executable code (the flash
//! routines, `Executor::run`), so nothing in the linker scripts changes.
//! Measured on `pico_display2_w` (2026-09-26): a third less CPU per page
//! turn, the Java-side spans of a turn 43 % shorter, for 38 KB (debug) of
//! `.data`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The board's `hot_ram_kb` (0 when absent): RAM the hot sets take out of
/// the arena, and the switch for the C-side retargeting.
pub fn board_hot_ram_kb(props: &HashMap<String, String>, what: &str) -> u32 {
    props
        .get("hot_ram_kb")
        .map(|v| {
            v.trim()
                .parse()
                .unwrap_or_else(|e| panic!("{what}: 'hot_ram_kb' not a number: {e}"))
        })
        .unwrap_or(0)
}

/// `hot-ram-<which>.txt` beside the MCU toml: one C function name per line,
/// `#` comments and blank lines ignored.
pub fn list_path(mcu_toml_path: &str, which: &str) -> PathBuf {
    Path::new(mcu_toml_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("hot-ram-{which}.txt"))
}

/// Read a hot list: the function names, in file order.
pub fn read_list(list: &Path) -> Vec<String> {
    println!("cargo:rerun-if-changed={}", list.display());
    let text = std::fs::read_to_string(list)
        .unwrap_or_else(|e| panic!("hot_ram: cannot read {}: {e}", list.display()));
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Move every function named in `list` from `.text.<fn>` to
/// `.data.hot.<fn>` inside `archive`, in place. The archive must have been
/// compiled with `-ffunction-sections` (cc-rs's default), so each function
/// is its own input section; a name the archive does not define is a no-op,
/// and a `static` several files define under one name moves in all of
/// them. Returns the number of names applied.
pub fn retarget(archive: &Path, list: &Path) -> usize {
    let names = read_list(list);
    assert!(
        archive.exists(),
        "hot_ram: archive {} does not exist (retarget after the C build)",
        archive.display()
    );
    if names.is_empty() {
        return 0;
    }
    let mut cmd = std::process::Command::new("arm-none-eabi-objcopy");
    for n in &names {
        cmd.arg("--rename-section")
            .arg(format!(".text.{n}=.data.hot.{n}"));
    }
    let status = cmd
        .arg(archive)
        .status()
        .unwrap_or_else(|e| panic!("hot_ram: cannot run arm-none-eabi-objcopy: {e}"));
    assert!(
        status.success(),
        "hot_ram: objcopy --rename-section failed on {}",
        archive.display()
    );
    names.len()
}
