//! Widget-callback dispatch site registry.
//!
//! Every `dispatch_*` function in `lifecycle.rs` invokes a fixed Java `fire*`
//! method on a framework class (Button.fireClick, ToggleButton.fireCheckedChanged,
//! …). Under `--shrink` the loaded class is renamed (e.g. `picodroid/widget/
//! ToggleButton` → `a/AP`), so passing the original name to
//! `jvm.invoke_instance` fails silently. The fix routes each name through
//! `shrink_names::shrink_class`.
//!
//! This module exists so the set of (class, method) pairs is declared ONCE.
//! `lifecycle.rs` indexes into [`DISPATCH_SITES`] via the per-site constants
//! below. The test in this module then iterates the same table and asserts
//! every entry resolves to a real method in the loaded framework under the
//! active shrink map — exactly what the event loop needs at runtime.
//!
//! Adding a new widget callback = append one entry + add one index
//! constant; the test auto-covers it.

// Indices read by `lifecycle.rs::dispatch_*`; unused from test builds, where
// `mod lifecycle` is cfg'd out but the const table is still iterated.
#![allow(dead_code)]

pub const BUTTON: usize = 0;
pub const TOGGLE_BUTTON: usize = 1;
pub const SWITCH: usize = 2;
pub const CHECKBOX: usize = 3;
pub const SEEK_BAR: usize = 4;
pub const SPINNER: usize = 5;
pub const VIEW_KEY: usize = 6;

/// `(original_framework_class, fire_method)` pairs. Order must match the
/// index constants above.
pub const DISPATCH_SITES: &[(&str, &str)] = &[
    ("picodroid/widget/Button", "fireClick"),
    ("picodroid/widget/ToggleButton", "fireCheckedChanged"),
    ("picodroid/widget/Switch", "fireCheckedChanged"),
    ("picodroid/widget/CheckBox", "fireCheckedChanged"),
    ("picodroid/widget/SeekBar", "fireProgressChanged"),
    ("picodroid/widget/Spinner", "fireItemSelected"),
    ("picodroid/view/View", "fireKey"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::class_file::ClassFile;

    /// Regression guard for the `--shrink`-breaks-callbacks bug (commit
    /// eba57c3). For every dispatch site, assert that
    /// `shrink_class(original)` returns the name of a loaded framework
    /// class *and* that class declares the expected `fire*` method.
    /// Run under both `PICODROID_SHRINK=0` and `PICODROID_SHRINK=1` (see
    /// `scripts/test.sh`).
    #[test]
    fn every_site_resolves_under_active_shrink_map() {
        let classes: Vec<ClassFile> = crate::framework_classes::FRAMEWORK_CLASSES
            .iter()
            .map(|b| ClassFile::parse(b).expect("parse framework class"))
            .collect();

        for &(orig, method) in DISPATCH_SITES {
            let shrunk = crate::shrink_names::shrink_class(orig);
            let cf = classes
                .iter()
                .find(|cf| cf.class_name() == Some(shrunk.as_bytes()))
                .unwrap_or_else(|| {
                    panic!(
                        "dispatch site '{orig}' -> shrunk '{shrunk}': no loaded \
                         framework class matches (would silently drop every callback \
                         at runtime — check `shrink_class` table and framework build)"
                    )
                });
            let has_method = cf
                .methods()
                .iter()
                .any(|m| cf.cp_utf8(m.name_index) == Some(method.as_bytes()));
            assert!(
                has_method,
                "'{shrunk}' (from '{orig}') is missing method '{method}' — \
                 `lifecycle::dispatch_*` would fail MethodNotFound at runtime"
            );
        }
    }
}
