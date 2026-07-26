// SPDX-License-Identifier: GPL-3.0-only
//! Registers this crate's GC root providers with `picodroid_core::gc_roots`.
//!
//! Every object that native code holds but Java does not — a View referenced
//! only by a listener map, a bound Service, the Display singleton — must be
//! reported to the collector or it is swept while still live. Missing one is
//! silent: the object disappears, its slot is reused, and the failure shows
//! up much later as dead input or `NoSuchMethod`.
//!
//! This list is deliberately one flat enumeration in one file. As the
//! extraction moves modules into `picodroid-core`, each provider's line moves
//! from here to that crate's own registration in the *same commit as the
//! file* — so the union of registered providers never changes, even though
//! its two halves do. [`EXPECTED_PROVIDERS`] pins the total across those
//! moves.
//!
//! See `docs/designs/shared-core-extraction.md` §3.G.

/// Total providers this crate registers.
///
/// Asserted by [`tests::registers_expected_provider_count`]. When a module
/// moves to `picodroid-core`, this drops by exactly the number of providers
/// that went with it — and the corresponding constant on the core side rises
/// by the same amount. A change here that is not matched there means a
/// provider was dropped on the floor.
#[cfg_attr(not(test), allow(dead_code))] // asserted by the guard below
pub const EXPECTED_PROVIDERS: usize = 16;

/// Register every root provider. Call before the first class load, and
/// therefore before any GC can run.
///
/// Idempotent: `run_jvm_with` re-runs on a PDB app reload, and registering
/// twice would visit every root twice — harmless but wasteful. The providers
/// themselves are stateless `fn` pointers, so nothing needs re-registering
/// when the app changes. A plain load/store rather than a compare-and-swap:
/// thumbv6m has no CAS, and boot is single-threaded by construction.
///
/// `cfg(not(test))` because every module named below pulls in LVGL and the
/// HAL. The completeness guard in this file's `tests` module is a source
/// scan, so it still covers this list under `scripts/test.sh`.
#[cfg(not(test))]
pub fn register_all() {
    use core::sync::atomic::{AtomicBool, Ordering};

    use picodroid_core::gc_roots;

    use crate::system::picodroid::graphics::lvgl::{events, widgets};

    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.load(Ordering::Relaxed) {
        return;
    }
    REGISTERED.store(true, Ordering::Relaxed);

    // Providers owned by picodroid-core register themselves.
    picodroid_core::gc_root_registration::register_all();

    // Widget/listener maps: the map *is* the only reference to these Views,
    // so the map is the root.
    gc_roots::register_object_refs(events::visit_view_listener_roots);
    gc_roots::register_object_refs(widgets::button::visit_click_listener_roots);
    gc_roots::register_object_refs(widgets::button::visit_long_click_listener_roots);
    gc_roots::register_object_refs(widgets::list_view::visit_item_click_listener_roots);
    gc_roots::register_object_refs(widgets::alert_dialog::visit_dialog_obj_roots);
    gc_roots::register_object_refs(widgets::switch::visit_checked_change_listener_roots);
    gc_roots::register_object_refs(widgets::check_box::visit_checked_change_listener_roots);
    gc_roots::register_object_refs(widgets::radio_button::visit_checked_change_listener_roots);
    gc_roots::register_object_refs(widgets::toggle_button::visit_checked_change_listener_roots);
    gc_roots::register_object_refs(widgets::edit_text::visit_editor_action_listener_roots);
    gc_roots::register_object_refs(widgets::edit_text::visit_text_changed_listener_roots);
    // NumberPicker registers every instance, not just listener holders: the
    // obj_ref is the fireStep dispatch target.
    gc_roots::register_object_refs(widgets::number_picker::visit_picker_roots);

    // Modules that own their own native object references.
    gc_roots::register(crate::system::picodroid::graphics::display::visit_gc_roots);
    gc_roots::register(crate::system::picodroid::hardware::sensors::visit_gc_roots);
    gc_roots::register(crate::service_lifecycle::visit_gc_roots);
    gc_roots::register(crate::lifecycle::visit_gc_roots);
}

#[cfg(test)]
mod tests {
    //! Completeness guard.
    //!
    //! `register_all` itself cannot run under `cfg(test)` — the modules it
    //! names are `cfg(not(test))`, because they pull in LVGL and the HAL. So
    //! the guard works on the source instead: every `visit_*roots` function
    //! defined anywhere in this crate must appear in `register_all`'s body.
    //!
    //! That catches the failure this whole design exists to prevent: adding
    //! a listener map with a root visitor and forgetting to register it, or
    //! moving a module to `picodroid-core` and dropping its registration on
    //! the way. Both compile and run fine, then collect live objects.

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

    /// Every `pub fn visit_*roots` defined in this crate, as `file::fn_name`.
    fn defined_providers(src_root: &Path) -> BTreeSet<(String, String)> {
        let mut files = Vec::new();
        rust_sources(src_root, &mut files);
        let mut found = BTreeSet::new();
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for line in text.lines() {
                let line = line.trim();
                let Some(rest) = line.strip_prefix("pub fn visit_") else {
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

    #[test]
    fn every_root_provider_is_registered() {
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let registration = std::fs::read_to_string(src_root.join("gc_root_registration.rs"))
            .expect("read gc_root_registration.rs");
        // Only the registration calls, not this test module's own text.
        let body = registration
            .split("#[cfg(test)]")
            .next()
            .expect("registration body precedes the test module");

        let defined = defined_providers(&src_root);
        assert!(
            !defined.is_empty(),
            "found no visit_*roots definitions — the scanner is broken, not the code"
        );

        let missing: Vec<_> = defined
            .iter()
            .filter(|(module, name)| {
                // `visit_gc_roots` is defined in four modules, so an
                // unqualified name match would let one registration satisfy
                // all of them. Require the qualified path, with the leading
                // `::` so `lifecycle::` is not matched by the tail of
                // `service_lifecycle::`.
                let ambiguous = defined.iter().filter(|(_, n)| n == name).count() > 1;
                if ambiguous {
                    !body.contains(&format!("::{module}::{name}"))
                } else {
                    !body.contains(name.as_str())
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
            super::EXPECTED_PROVIDERS,
            "provider count changed: {} defined, EXPECTED_PROVIDERS = {}. \
             If a module moved to picodroid-core, lower this by exactly the \
             number that went with it and raise the core-side constant to match.",
            defined.len(),
            super::EXPECTED_PROVIDERS
        );
    }
}
