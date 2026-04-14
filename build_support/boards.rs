//! Board-level setup: linker script / memory.x placement.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Copy the board's (or MCU's default) memory.x into OUT_DIR and add it to
/// the linker search path. Returns nothing; emits `cargo:rustc-link-search`
/// and `cargo:rerun-if-changed` for the source.
pub fn place_memory_x(
    out: &Path,
    board: &HashMap<String, String>,
    mcu_family: &str,
    mcu_name: &str,
) {
    let memory_src = if let Some(ls) = board.get("linker_script") {
        ls.clone()
    } else {
        format!("mcus/{mcu_family}/{mcu_name}.x")
    };
    let memory_bytes =
        fs::read(&memory_src).unwrap_or_else(|e| panic!("Failed to read {memory_src}: {e}"));
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(&memory_bytes)
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed={memory_src}");
}
