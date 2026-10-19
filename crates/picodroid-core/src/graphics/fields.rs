// SPDX-License-Identifier: GPL-3.0-only
/// Field indices for picodroid.view.View and subclasses.
///
/// Slot numbering follows the JVM `field_slot()` convention: superclass fields
/// come first (root-to-leaf), so `View.nativeHandle` is always slot 0 for
/// every widget subclass, and a `long` or `double` field takes two slots, so
/// the field after one starts one higher than its declaration index. Every
/// constant here is checked against the class files by
/// `native_field_tables_tests`.
pub mod view {
    /// `lv_obj_t*` cast to `i32` (declared in `View.java`).
    pub const NATIVE_HANDLE: usize = 0;
    // View.java declares the parent back-pointer (slot 1), the listeners
    // (2-7) and the layout params (8) after `nativeHandle`, then the fields
    // the getters read, which the inflater's native path writes for a view
    // it made (24 instance fields in all); a subclass's own fields — e.g.
    // `CompoundButton.onCheckedChangeListener` — follow them. Fields not
    // named here are read and written from Java only.
    //
    // The listener fields are read, never written, natively: the key router
    // and the long-press query ask whether the focused view has one before
    // paying for an upcall that would return false.
    /// `OnKeyListener onKeyListener`.
    pub const ON_KEY_LISTENER: usize = 2;
    /// `OnTouchListener onTouchListener`.
    pub const ON_TOUCH_LISTENER: usize = 3;
    /// `OnSwipeListener onSwipeListener`.
    pub const ON_SWIPE_LISTENER: usize = 4;
    /// `OnClickListener onClickListener`.
    pub const ON_CLICK_LISTENER: usize = 5;
    /// `OnLongClickListener onLongClickListener`.
    pub const ON_LONG_CLICK_LISTENER: usize = 6;
    /// `OnFocusChangeListener onFocusChangeListener`.
    pub const ON_FOCUS_CHANGE_LISTENER: usize = 7;
    /// `boolean focusable`.
    pub const FOCUSABLE: usize = 10;
    /// `boolean keepScreenOn`.
    pub const KEEP_SCREEN_ON: usize = 11;
    pub const MIN_WIDTH: usize = 12;
    pub const MIN_HEIGHT: usize = 13;
    /// `int visibility`.
    pub const VISIBILITY: usize = 14;
    /// `boolean enabled`.
    pub const ENABLED: usize = 15;
    /// `float alpha`.
    pub const ALPHA: usize = 16;
    /// `int id`.
    pub const ID: usize = 17;
}

/// `picodroid.widget.TextView`'s own fields, after `View`'s 24: the cached
/// values its getters and dedupes read, written by the inflater's native
/// path for a label it made.
pub mod text_view {
    /// `int mLineMode`: the packed ellipsize kind, single-line flag,
    /// max-lines count and two flags (`inflate_scan::pack_line_mode`).
    pub const LINE_MODE: usize = 24;
    pub const MAX_WIDTH: usize = 25;
    /// `float mTextSize`.
    pub const TEXT_SIZE: usize = 26;
    /// `String mText`.
    pub const TEXT: usize = 27;
    pub const TEXT_COLOR: usize = 28;
    pub const GRAVITY: usize = 29;
}

/// `picodroid.view.LayoutInflater`: the read position and child count its
/// `one` keeps, and the `LayoutParams` words and hand-back count the native
/// path leaves for it, in declaration order after `mContext` and `mFactory`.
pub mod layout_inflater {
    /// `int mPos`.
    pub const POS: usize = 3;
    /// `int mChildCount`.
    pub const CHILD_COUNT: usize = 4;
    pub const WIDTH: usize = 11;
    pub const HEIGHT: usize = 12;
    /// `float mWeight`.
    pub const WEIGHT: usize = 13;
    pub const LAYOUT_GRAVITY: usize = 14;
    pub const MARGIN_LEFT: usize = 15;
    pub const MARGIN_TOP: usize = 16;
    pub const MARGIN_RIGHT: usize = 17;
    pub const MARGIN_BOTTOM: usize = 18;
    /// `int mDeferredCount`: pairs written to `mDeferred`.
    pub const DEFERRED_COUNT: usize = 19;
    /// `int mRelativeFlags` and `int mAnchor0..7`: a `RelativeLayout`
    /// child's rules, written only for one (declared after the inflater's
    /// other fields, so the slots follow them).
    pub const RELATIVE_FLAGS: usize = 31;
    pub const ANCHOR0: usize = 32;
}

pub mod display {
    pub const WIDTH: usize = 0;
    pub const HEIGHT: usize = 1;
    /// The panel's physical pixel pitch (`[display] dpi`), what
    /// `Display.getMetrics` hands out as `xdpi`/`ydpi`.
    pub const DPI: usize = 2;
}

pub mod motion_event {
    pub const ACTION: usize = 0;
    /// View-relative X / Y (Android's getX/getY).
    pub const X: usize = 1;
    pub const Y: usize = 2;
    /// Tick-clock millis: a `long`, so slots 3 and 4.
    pub const EVENT_TIME: usize = 3;
    /// Screen-absolute X / Y (Android's getRawX/getRawY). Declared after
    /// eventTime in MotionEvent.java so these slots come last.
    pub const RAW_X: usize = 5;
    pub const RAW_Y: usize = 6;
    /// Slots the recycled instance is allocated with.
    pub const SLOTS: usize = RAW_Y + 1;
}

pub mod key_event {
    pub const ACTION: usize = 0;
    pub const KEY_CODE: usize = 1;
    /// `int repeatCount`: 0 for the press, then 1, 2, … for each auto-repeat
    /// while the key stays held (`lvgl/key_repeat.rs`).
    pub const REPEAT_COUNT: usize = 2;
    /// `int flags`: the dispatcher writes `FLAG_LONG_PRESS` on the first
    /// repeat and 0 otherwise; `KeyEvent.dispatch` adds the tracking and
    /// cancel bits in Java. Rewritten on every edge, so nothing carries
    /// between edges except through a Java `DispatcherState`.
    pub const FLAGS: usize = 3;
    /// `long downTime`: elapsedRealtime millis of the press; slots 4 and 5.
    pub const DOWN_TIME: usize = 4;
    /// `long eventTime`: elapsedRealtime millis of this edge; slots 6 and 7.
    pub const EVENT_TIME: usize = 6;
    /// Slots the recycled instance is allocated with (the last long takes two).
    pub const SLOTS: usize = EVENT_TIME + 2;
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

/// `picodroid.content.Intent` is **not** a View — slot numbering starts from
/// its own first declared field. The extras `Bundle` sits between the two named
/// here, and the broadcast `action` (slot 3, read from Java only) follows
/// `packageName`; `Intent.java` keeps that order for this reason.
pub mod intent {
    /// `targetClassName`: the class a same-app launch pushes.
    pub const TARGET_CLASS_NAME: usize = 0;
    /// `packageName`: the package a cross-package launch leaves for.
    pub const PACKAGE: usize = 2;
}

/// `picodroid.graphics.Canvas` is **not** a View. `View.nativeBeginDraw` writes
/// all three while `onDraw` runs and `nativeEndDraw` zeroes them, so a canvas
/// used outside `onDraw` holds handle 0 and draws nothing.
pub mod canvas {
    /// The drawing view's widget handle; 0 outside `onDraw`.
    pub const NATIVE_HANDLE: usize = 0;
    pub const WIDTH: usize = 1;
    pub const HEIGHT: usize = 2;
}

/// `picodroid.graphics.Paint`, read by every `Canvas` draw native. The enums
/// are stored as ordinals so these are all `int`s and `float`s.
pub mod paint {
    /// `int color`, 0xAARRGGBB.
    pub const COLOR: usize = 0;
    /// `float strokeWidth`; 0 is a hairline.
    pub const STROKE_WIDTH: usize = 1;
    /// `int style`: `Paint.Style` ordinal (FILL, STROKE, FILL_AND_STROKE).
    pub const STYLE: usize = 2;
    /// `int strokeCap`: `Paint.Cap` ordinal (BUTT, ROUND, SQUARE).
    pub const STROKE_CAP: usize = 3;
    /// `float textSize`, pixels.
    pub const TEXT_SIZE: usize = 4;
    /// `int textAlign`: `Paint.Align` ordinal (LEFT, CENTER, RIGHT).
    pub const TEXT_ALIGN: usize = 5;
}
