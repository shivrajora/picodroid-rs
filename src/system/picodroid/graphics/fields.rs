/// Field indices for picodroid.view.View and subclasses.
///
/// Slot numbering follows the JVM `field_slot()` convention: superclass fields
/// come first (root-to-leaf), so `View.nativeHandle` is always slot 0 for
/// every widget subclass.
pub mod view {
    /// `lv_obj_t*` cast to `i32` (declared in `View.java`).
    pub const NATIVE_HANDLE: usize = 0;
}

#[allow(dead_code)]
pub mod button {
    // Inherits slot 0 = nativeHandle from View.
    /// `Runnable` callback (`ObjectRef`) — accessed from Java only.
    pub const ON_CLICK_LISTENER: usize = 1;
}

#[allow(dead_code)]
pub mod linear_layout {
    // Inherits slot 0 = nativeHandle from View.
    /// Orientation: 0 = HORIZONTAL, 1 = VERTICAL — stored from Java only.
    pub const ORIENTATION: usize = 1;
}

#[allow(dead_code)]
pub mod toggle_button {
    // Inherits slot 0 = nativeHandle from View.
    /// `Runnable` callback (`ObjectRef`) — accessed from Java only.
    pub const ON_CHECKED_CHANGE_LISTENER: usize = 1;
}

pub mod display {
    pub const WIDTH: usize = 0;
    pub const HEIGHT: usize = 1;
}

pub mod motion_event {
    pub const ACTION: usize = 0;
    pub const X: usize = 1;
    pub const Y: usize = 2;
}
