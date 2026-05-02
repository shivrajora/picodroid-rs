//! Board-level setup: linker script / memory.x placement, board module
//! generation.

use std::collections::HashMap;
use std::env;
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

/// Emit `OUT_DIR/board_imports.rs` so [`src/boards/mod.rs`] can `include!` it.
/// When a board is active, the file declares
/// `#[path = ".../boards/<name>/mod.rs"] pub mod board_specific;` and
/// re-exports its contents. Otherwise the file is empty — `src/main.rs` only
/// pulls in the `boards` module on hardware builds, so an empty include is a
/// valid no-op for sim/test builds with no active board.
pub fn emit_board_imports(out: &Path, active_board: Option<&str>) {
    let path = out.join("board_imports.rs");
    let mut f = File::create(&path).expect("create board_imports.rs");
    if let Some(name) = active_board {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let board_mod = format!("{manifest}/boards/{name}/mod.rs");
        // The path may not exist for boards that don't need any Rust glue —
        // emit empty in that case.
        if Path::new(&board_mod).is_file() {
            writeln!(
                f,
                "#[path = {board_mod:?}]\npub mod board_specific;\n\
                 #[allow(unused_imports)]\npub use board_specific::*;"
            )
            .unwrap();
            println!("cargo:rerun-if-changed={board_mod}");
            return;
        }
    }
    // Empty file — valid no-op include.
    writeln!(f, "// No active board — empty no-op include.").unwrap();
}
