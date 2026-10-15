// SPDX-License-Identifier: GPL-3.0-only
//! Every file under `fuzz/regress/<target>/` through its target, on the
//! pinned stable toolchain: the seeds, and the minimized reproducer of every
//! past finding, which its fix commit adds there. A target whose directory
//! is missing or empty fails too — a renamed target must not go quietly
//! unreplayed.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

fn regress_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/regress")
}

#[test]
fn every_regression_input_passes_its_target() {
    let mut failures = Vec::new();
    let mut total = 0usize;
    // `PICODROID_FUZZ_REPLAY=a,b`: only those targets (the Miri pass, which
    // is far too slow for the TLS handshake).
    let only = std::env::var("PICODROID_FUZZ_REPLAY").ok();
    for &(name, target) in fuzz_targets::TARGETS {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|n| n == name))
        {
            continue;
        }
        let dir = regress_dir().join(name);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        assert!(!files.is_empty(), "{}: no inputs", dir.display());
        for file in files {
            let bytes = std::fs::read(&file).unwrap();
            total += 1;
            if catch_unwind(AssertUnwindSafe(|| target(&bytes))).is_err() {
                failures.push(format!("{name}: {}", file.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {total} inputs failed (reproduce one with \
         `scripts/fuzz.sh --target <name> --reproduce <file>`):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// Each directory under `fuzz/regress/` is a target this crate knows, so a
/// target removed from [`fuzz_targets::TARGETS`] takes its inputs with it.
#[test]
fn every_regression_directory_has_a_target() {
    for entry in std::fs::read_dir(regress_dir()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(
            fuzz_targets::TARGETS.iter().any(|(n, _)| *n == name),
            "fuzz/regress/{name} has no target"
        );
    }
}
