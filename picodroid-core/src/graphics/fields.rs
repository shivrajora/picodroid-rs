// SPDX-License-Identifier: GPL-3.0-only
/// Field indices for picodroid.view.View and subclasses.
///
/// Slot numbering follows the JVM `field_slot()` convention: superclass fields
/// come first (root-to-leaf), so `View.nativeHandle` is always slot 0 for
/// every widget subclass.
pub mod view {
    /// `lv_obj_t*` cast to `i32` (declared in `View.java`).
    pub const NATIVE_HANDLE: usize = 0;
    // Slot 1 is `onKeyListener`; a View subclass's own fields therefore start
    // at slot 2 (Button.onClickListener, LinearLayout.orientation and
    // ToggleButton.onCheckedChangeListener each sit there). None of those are
    // named in this file — they are read and written from Java only, so no
    // Rust code ever needs their index.
}

pub mod display {
    pub const WIDTH: usize = 0;
    pub const HEIGHT: usize = 1;
}

pub mod motion_event {
    pub const ACTION: usize = 0;
    /// View-relative X / Y (Android's getX/getY).
    pub const X: usize = 1;
    pub const Y: usize = 2;
    /// Tick-clock millis. This JVM uses one slot per field regardless of
    /// type, so a `long` field gets the next sequential slot.
    pub const EVENT_TIME: usize = 3;
    /// Screen-absolute X / Y (Android's getRawX/getRawY). Declared after
    /// eventTime in MotionEvent.java so these slots come last.
    pub const RAW_X: usize = 4;
    pub const RAW_Y: usize = 5;
}

pub mod key_event {
    pub const ACTION: usize = 0;
    pub const KEY_CODE: usize = 1;
}

/// `picodroid.app.AlertDialog` is **not** a View subclass — slot numbering
/// starts from its own first declared field.
pub mod alert_dialog {
    /// Scrim handle returned by `lvgl::widgets::alert_dialog::create`.
    pub const NATIVE_HANDLE: usize = 0;
    // Slots 1 and 2 are the positive / negative button `Runnable`s, read
    // from Java only. A new field starts at slot 3.
}

/// `picodroid.widget.Snackbar` is **not** a View subclass — slot numbering
/// starts from its own first declared field.
pub mod snackbar {
    /// Bar handle returned by `lvgl::widgets::snackbar::create`.
    pub const NATIVE_HANDLE: usize = 0;
    // Slot 1 is the action lozenge's `Runnable`, read from Java only. A new
    // field starts at slot 2.
}
