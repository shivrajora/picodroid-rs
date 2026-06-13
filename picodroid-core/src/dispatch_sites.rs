// SPDX-License-Identifier: GPL-3.0-only
//! Widget-callback dispatch site registry.
//!
//! Every `dispatch_*` function in `lifecycle.rs` invokes a fixed Java `fire*`
//! method on a framework class (View.fireClick, ToggleButton.fireCheckedChanged,
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
pub const EXECUTORS_DISPATCH: usize = 7;
pub const ALERT_DIALOG: usize = 8;
// Activity lifecycle fallbacks — used when an Activity subclass doesn't
// declare the lifecycle method and the framework loop must fall back to
// the default (no-op) impl on `picodroid/app/Activity`.
pub const ACTIVITY_ON_CREATE: usize = 9;
pub const ACTIVITY_ON_START: usize = 10;
pub const ACTIVITY_ON_RESUME: usize = 11;
pub const ACTIVITY_ON_PAUSE: usize = 12;
pub const ACTIVITY_ON_STOP: usize = 13;
pub const ACTIVITY_ON_DESTROY: usize = 14;
pub const ACTIVITY_ON_BACK_PRESSED: usize = 15;
pub const VIEW_TOUCH: usize = 16;
pub const KEYBOARD_READY: usize = 17;
// Service lifecycle fallbacks — used when a Service subclass doesn't
// declare a lifecycle method and the framework loop must fall back to
// the default (no-op) impl on `picodroid/app/Service`.
pub const SERVICE_ON_CREATE: usize = 18;
pub const SERVICE_ON_START_COMMAND: usize = 19;
pub const SERVICE_ON_BIND: usize = 20;
pub const SERVICE_ON_UNBIND: usize = 21;
pub const SERVICE_ON_DESTROY: usize = 22;
pub const EDIT_TEXT_EDITOR_ACTION: usize = 23;
pub const SNACKBAR: usize = 24;
pub const DATE_PICKER: usize = 25;
pub const TIME_PICKER: usize = 26;
pub const VIEW_SWIPE: usize = 27;
pub const SWIPE_REFRESH: usize = 28;
pub const LIST_VIEW_ITEM_CLICK: usize = 29;
pub const VIEW_FOCUS_CHANGE: usize = 30;
pub const NUMBER_PICKER_STEP: usize = 31;
/// `onRestart` — runs between onStop-state and onStart when an Activity
/// returns to the foreground after the one above it finished (Android's
/// stopped→restarted edge). Appended so earlier indices stay stable.
pub const ACTIVITY_ON_RESTART: usize = 32;
/// SeekBar press/release edges → `OnSeekBarChangeListener.onStartTrackingTouch`
/// / `onStopTrackingTouch`, fanned out by `fireTrackingTouch(boolean)`.
pub const SEEK_BAR_TRACKING: usize = 33;
/// Textarea content changes → `TextWatcher.afterTextChanged`, fanned out by
/// `EditText.fireTextChanged()` (which re-reads getText() itself).
pub const EDIT_TEXT_TEXT_CHANGED: usize = 34;

/// `(original_framework_class, fire_method)` pairs. Order must match the
/// index constants above.
pub const DISPATCH_SITES: &[(&str, &str)] = &[
    ("picodroid/view/View", "fireClick"),
    ("picodroid/widget/CompoundButton", "fireCheckedChanged"),
    ("picodroid/widget/CompoundButton", "fireCheckedChanged"),
    ("picodroid/widget/CompoundButton", "fireCheckedChanged"),
    ("picodroid/widget/SeekBar", "fireProgressChanged"),
    ("picodroid/widget/Spinner", "fireItemSelected"),
    ("picodroid/view/View", "fireKey"),
    // Main-executor + background-pool drain invoke this static bridge,
    // which then calls `r.run()` via bytecode so lambda proxies resolve
    // through the interpreter's invokeinterface path.
    ("picodroid/concurrent/Executors", "dispatchRunnable"),
    ("picodroid/app/AlertDialog", "fireButtonClick"),
    ("picodroid/app/Activity", "onCreate"),
    ("picodroid/app/Activity", "onStart"),
    ("picodroid/app/Activity", "onResume"),
    ("picodroid/app/Activity", "onPause"),
    ("picodroid/app/Activity", "onStop"),
    ("picodroid/app/Activity", "onDestroy"),
    ("picodroid/app/Activity", "onBackPressed"),
    ("picodroid/view/View", "fireTouch"),
    ("picodroid/widget/Keyboard", "fireReady"),
    ("picodroid/app/Service", "onCreate"),
    ("picodroid/app/Service", "onStartCommand"),
    ("picodroid/app/Service", "onBind"),
    ("picodroid/app/Service", "onUnbind"),
    ("picodroid/app/Service", "onDestroy"),
    ("picodroid/widget/EditText", "fireEditorAction"),
    ("picodroid/widget/Snackbar", "fireActionClick"),
    ("picodroid/widget/DatePicker", "fireDateChanged"),
    ("picodroid/widget/TimePicker", "fireTimeChanged"),
    ("picodroid/view/View", "fireSwipe"),
    ("picodroid/widget/SwipeRefreshLayout", "fireRefresh"),
    ("picodroid/widget/ListView", "fireItemClick"),
    ("picodroid/view/View", "fireFocusChange"),
    ("picodroid/widget/NumberPicker", "fireStep"),
    ("picodroid/app/Activity", "onRestart"),
    ("picodroid/widget/SeekBar", "fireTrackingTouch"),
    ("picodroid/widget/EditText", "fireTextChanged"),
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
