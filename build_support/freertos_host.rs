// SPDX-License-Identifier: GPL-3.0-only
//! Hosted FreeRTOS kernel compilation — the POSIX port, for the simulator.
//!
//! Separate from `freertos.rs` (the device leg) for two reasons, both
//! structural rather than stylistic:
//!
//! - The device leg drives `freertos_cargo_build::Builder`, which *requires* a
//!   `portable/MemMang/heap_N.c` and adds it unconditionally. The hosted leg
//!   must compile no heap file at all: `pvPortMalloc` / `vPortFree` are Rust
//!   shims that route to the host allocator under `allocator::bypass()`, so
//!   the kernel's host-sized objects never reach the modeled device arena
//!   (`docs/designs/freertos-host-sim.md` §1.1). A plain `cc::Build` is the
//!   only way to leave the heap out.
//! - Nothing here is MCU-specific. There is no `mcus/<mcu>.toml` to read, no
//!   vector aliasing and no linker fragment — just the kernel, the POSIX
//!   port, and the `freertos-rust` shim.
//!
//! Called from `picodroid-core/build.rs` (not a platform's) because the
//! simulator lives in that crate: it holds `hal/sim/`, the allocator the
//! shims bypass, and the `freertos-rust` dependency whose `links = "freertos"`
//! metadata exports `DEP_FREERTOS_SHIM`.

use std::path::{Path, PathBuf};

/// Compile the FreeRTOS kernel + POSIX port + `freertos-rust` shim into
/// `libfreertos.a` and link it into the simulator binary.
///
/// `config_dir` must contain a hosted `FreeRTOSConfig.h`. No heap
/// implementation is compiled — see the module docs.
pub fn build(repo_root: &Path, config_dir: &Path) {
    let kernel = repo_root.join("third_party/FreeRTOS-Kernel");
    let port = kernel.join("portable/ThirdParty/GCC/Posix");

    assert!(
        port.is_dir(),
        "FreeRTOS POSIX port missing: {} — is the FreeRTOS-Kernel submodule checked out?",
        port.display()
    );

    // Exported by freertos-rust-pd's build.rs via its `links = "freertos"`
    // metadata. Its shim.c wraps the kernel macros the Rust bindings call.
    let shim_dir = PathBuf::from(
        std::env::var("DEP_FREERTOS_SHIM")
            .expect("DEP_FREERTOS_SHIM unset — the freertos-rust dependency is not active"),
    );

    let mut b = cc::Build::new();
    b.include(kernel.join("include"));
    b.include(&port);
    b.include(config_dir);

    // The kernel's own translation units are exactly the top-level .c files;
    // `portable/` is walked separately so we pick the POSIX port and nothing
    // else. (No MemMang: see the module docs.)
    for f in c_files_in(&kernel) {
        add_file(&mut b, &f);
    }
    add_file(&mut b, &port.join("port.c"));
    add_file(&mut b, &port.join("utils/wait_for_event.c"));
    add_file(&mut b, &shim_dir.join("shim.c"));

    // The port is pthreads + SIGALRM. `_GNU_SOURCE` is set by port.c itself
    // on Linux; wait_for_event.c needs the pthread link only.
    println!("cargo:rustc-link-lib=pthread");

    // The kernel is third-party C held at a pinned commit: its warnings are
    // not ours to fix, and letting them through unfiltered buries our own.
    b.warnings(false);

    b.compile("freertos");

    println!(
        "cargo:rerun-if-changed={}",
        config_dir.join("FreeRTOSConfig.h").display()
    );
}

fn c_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "c").unwrap_or(false))
        .collect();
    // read_dir order is filesystem-defined; sort so the archive is
    // reproducible across machines.
    files.sort();
    files
}

fn add_file(b: &mut cc::Build, path: &Path) {
    assert!(path.is_file(), "missing C source: {}", path.display());
    println!("cargo:rerun-if-changed={}", path.display());
    b.file(path);
}
