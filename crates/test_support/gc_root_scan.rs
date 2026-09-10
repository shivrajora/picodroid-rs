// SPDX-License-Identifier: GPL-3.0-only
//! Source scan backing each crate's GC-root completeness guard.
//!
//! `#[path]`-included by both `platforms/rp/src/gc_root_registration.rs` and
//! `picodroid-core/src/gc_root_registration.rs`, so the two guards cannot
//! drift from each other — the same hazard `build_support/board_cfg.rs`
//! exists to avoid on the build-script side.
//!
//! Why a *source* scan rather than calling `register_all` and counting: the
//! modules that own providers are `cfg(not(test))` (they pull in LVGL and the
//! HAL), so under `cargo test` there is nothing to call. The text is the only
//! thing both configurations share.
//!
//! What it catches: a `visit_*roots` function that exists but is never
//! registered. That compiles, links, and runs — and then collects live
//! objects, which surfaces much later as dead input or `NoSuchMethod`. It is
//! the single failure mode this project has been burned by most often.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Every `pub fn visit_*roots` defined under `src_root`, as `(module, fn)`.
pub fn defined_providers(src_root: &Path) -> BTreeSet<(String, String)> {
    let mut files = Vec::new();
    rust_sources(src_root, &mut files);
    let mut found = BTreeSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            // Leading `///` means a doc comment mentioning one, not a
            // definition.
            let Some(rest) = line.trim().strip_prefix("pub fn visit_") else {
                continue;
            };
            let Some(name) = rest.split('(').next() else {
                continue;
            };
            if !name.ends_with("roots") {
                continue;
            }
            // `foo/mod.rs` is module `foo`, not module `mod`.
            let stem = file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let module = if stem == "mod" {
                file.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
            } else {
                stem
            };
            found.insert((module.to_string(), format!("visit_{name}")));
        }
    }
    found
}

/// The registration calls in `gc_root_registration.rs`, as one blob.
///
/// Only lines that actually call `register`/`register_object_refs` — not the
/// whole file. That keeps the guard indifferent to where the test module
/// sits, and excludes the cross-crate `register_all()` delegation (no `(`
/// directly after `register`), which forwards to the other crate's list
/// rather than naming a provider.
fn registration_body(src_root: &Path) -> String {
    let text = std::fs::read_to_string(src_root.join("gc_root_registration.rs"))
        .expect("read gc_root_registration.rs");
    text.lines()
        .filter(|l| l.contains("register(") || l.contains("register_object_refs("))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert every provider defined under `src_root` is registered, and that the
/// count still matches `expected`.
pub fn check(src_root: &Path, expected: usize) {
    let body = registration_body(src_root);
    let defined = defined_providers(src_root);
    // Finding nothing is a broken scanner *only* if this crate is supposed to
    // own providers. Once the extraction has moved them all to the other
    // crate, `expected == 0` and an empty scan is the correct answer — the
    // count assertion below still pins it, so a provider reappearing here
    // unregistered is caught.
    assert!(
        expected == 0 || !defined.is_empty(),
        "found no visit_*roots definitions under {} but EXPECTED_PROVIDERS is \
         {expected} — the scanner is broken, not the code",
        src_root.display()
    );

    let missing: Vec<_> = defined
        .iter()
        .filter(|(module, name)| {
            // Match a path segment (`::name`), not a bare textual
            // occurrence, so a passing mention in a trailing comment on some
            // other registration line cannot satisfy a provider.
            //
            // A name defined in more than one module additionally needs its
            // module qualifier, or one registration would satisfy them all —
            // `visit_gc_roots` and `visit_checked_change_listener_roots` are
            // each defined several times. The leading `::` matters most
            // there: without it `lifecycle::visit_gc_roots` is matched by the
            // tail of `service_lifecycle::visit_gc_roots`, so registering one
            // would silently cover the other.
            let ambiguous = defined.iter().filter(|(_, n)| n == name).count() > 1;
            if ambiguous {
                !body.contains(&format!("::{module}::{name}"))
            } else {
                !body.contains(&format!("::{name}"))
            }
        })
        .collect();

    assert!(
        missing.is_empty(),
        "GC root providers defined but never registered: {missing:?}\n\
         An unregistered provider is swept-while-live, which fails silently \
         at runtime. Add it to register_all() and bump EXPECTED_PROVIDERS."
    );

    assert_eq!(
        defined.len(),
        expected,
        "provider count changed: {} defined, EXPECTED_PROVIDERS = {}. \
         A provider moving between crates must lower one crate's constant \
         and raise the other's by the same amount — the union never changes \
         size. Changing only one side means a provider was dropped.",
        defined.len(),
        expected
    );
}
