// SPDX-License-Identifier: GPL-3.0-only
//! Native method implementations for `picodroid.view.View`.
//!
//! Cross-widget setters route through the [`super::gfx::Gfx`] trait via
//! `with_gfx`; the LVGL specifics live in `lvgl::view_ops`. The helpers
//! (`extract_native_handle`, `java_str_to_cstr`, `resolve`) and the key
//! listener registry stay here because they bridge the JVM heap to the
//! widget call sites — they are not LVGL-specific in a way the trait
//! could hide cleanly.

use pico_jvm::heap::StringTable;
use pico_jvm::names::c;
use pico_jvm::object_heap::ObjectHeap;
use pico_jvm::types::{JvmError, Value};

use super::fields;
use super::gfx::{Handle, ViewProperty, Visibility};
use super::lvgl::events as lvgl_events;
use super::lvgl::widgets::button as lvgl_button;
use super::lvgl::with_gfx;

// ---------------------------------------------------------------------------
// Helpers shared with widgets/*.rs
// ---------------------------------------------------------------------------

/// Extract the `nativeHandle` ID from `this` (args[0]).
pub fn extract_native_handle(args: &[Value], objects: &ObjectHeap) -> Result<i32, JvmError> {
    let obj_idx = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    match objects.get_field(obj_idx, fields::view::NATIVE_HANDLE) {
        Some(Value::Int(handle)) => Ok(handle),
        _ => Err(JvmError::InvalidReference),
    }
}

/// Extract the `nativeHandle` ID from a `View` argument at the given position.
pub fn extract_handle_at(
    args: &[Value],
    arg_idx: usize,
    objects: &ObjectHeap,
) -> Result<i32, JvmError> {
    let obj_idx = match args.get(arg_idx) {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    match objects.get_field(obj_idx, fields::view::NATIVE_HANDLE) {
        Some(Value::Int(handle)) => Ok(handle),
        _ => Err(JvmError::InvalidReference),
    }
}

/// The body of every `set*Listener` native: `this` (args[0]) is both the
/// Java object the widget's events go to and the view whose handle names
/// the widget. The handle table binds the pair (`handle_table::bind`), then
/// `attach` wires the kind's LVGL trampoline — or [`NO_ATTACH`] for a kind
/// whose widget already emits (the compound buttons' VALUE_CHANGED, the
/// keypad's keys). One out-of-line copy for the ~19 registration natives,
/// which each carried the argument and handle reads.
///
/// The listener itself is a Java field on the view, and the view is kept
/// alive by whatever Java reaches it through (its parent's child list, a
/// field): the binding is not a GC root, so there is no table to fill and
/// no cap (docs/designs/view-listeners-2026-10.md).
#[inline(never)]
pub fn register_listener_with(
    args: &[Value],
    objects: &ObjectHeap,
    attach: fn(i32),
) -> Result<Option<Value>, JvmError> {
    let obj_ref = match args.first() {
        Some(Value::ObjectRef(idx)) => *idx,
        _ => return Err(JvmError::InvalidReference),
    };
    let id = extract_native_handle(args, objects)?;
    super::lvgl::handle_table::bind(id, obj_ref);
    attach(id);
    Ok(None)
}

/// [`register_listener_with`]'s `attach` for a kind that needs no LVGL
/// trampoline of its own.
pub const NO_ATTACH: fn(i32) = |_| {};

/// Extract a Java `String` argument as a Rust `&str`. Returned reference
/// is valid for the lifetime of `strings`. A `null` reads as `""`: Android's
/// text setters (`setText(null)`, `setHint(null)`, `addItem(null)`) clear
/// the text, and the uncatchable `InvalidReference` this used to be ended
/// the whole callback ("Activity lifecycle error", the rest of `onCreate`
/// skipped). The Canvas text natives, where Android throws, use
/// [`extract_string_non_null_at`].
pub fn extract_string_at<'s>(
    args: &[Value],
    arg_idx: usize,
    strings: &'s StringTable,
) -> Result<&'s str, JvmError> {
    let idx = match args.get(arg_idx) {
        Some(Value::Reference(idx)) => *idx,
        Some(Value::Null) => return Ok(""),
        _ => return Err(JvmError::InvalidReference),
    };
    strings.resolve(idx).ok_or(JvmError::InvalidReference)
}

/// [`extract_string_at`] for a String Android refuses as `null`
/// (`Canvas.drawText`, `Paint.measureText`): a catchable
/// `NullPointerException` instead.
pub fn extract_string_non_null_at<'s>(
    args: &[Value],
    arg_idx: usize,
    strings: &'s StringTable,
    objects: &mut ObjectHeap,
) -> Result<&'s str, JvmError> {
    if matches!(args.get(arg_idx), Some(Value::Null)) {
        let idx = objects
            .alloc(c::java_lang_NullPointerException)
            .ok_or(JvmError::StackOverflow)?;
        return Err(JvmError::Exception(idx));
    }
    extract_string_at(args, arg_idx, strings)
}

#[inline]
pub(in crate::graphics) fn arg_int(args: &[Value], i: usize) -> Result<i32, JvmError> {
    match args.get(i) {
        Some(Value::Int(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

#[inline]
fn arg_float(args: &[Value], i: usize) -> Result<f32, JvmError> {
    match args.get(i) {
        Some(Value::Float(v)) => Ok(*v),
        _ => Err(JvmError::InvalidReference),
    }
}

// ---------------------------------------------------------------------------
// View native methods — all routed through Gfx::* via with_gfx.
// ---------------------------------------------------------------------------

/// `View.setPosition(int x, int y)`
pub fn set_position(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let x = arg_int(args, 1)?;
    let y = arg_int(args, 2)?;
    with_gfx(|g| g.set_pos(Handle::from_java(id), x, y));
    Ok(None)
}

/// `View.setSize(int width, int height)`
pub fn set_size(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let w = arg_int(args, 1)?;
    let h = arg_int(args, 2)?;
    with_gfx(|g| g.set_size(Handle::from_java(id), w, h));
    Ok(None)
}

/// `View.nativeSetBackgroundColor(int argb)`
pub fn set_bg_color(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = arg_int(args, 1)? as u32;
    with_gfx(|g| g.set_bg_color(Handle::from_java(id), argb));
    Ok(None)
}

/// `View.nativeSetBackgroundTint(int argb)`
pub fn set_bg_tint(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let argb = arg_int(args, 1)? as u32;
    with_gfx(|g| g.set_bg_tint(Handle::from_java(id), argb));
    Ok(None)
}

/// `View.setVisibility(int visibility)` — Android constants
/// (0=VISIBLE, 4=INVISIBLE, 8=GONE). The pre-v0.11 SDK shipped 1/2 for
/// INVISIBLE/GONE; those values are still accepted so a PAPK built against
/// the old constants keeps working on newer firmware.
pub fn set_visibility(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let v = match arg_int(args, 1)? {
        0 => Visibility::Visible,
        1 | 4 => Visibility::Invisible,
        _ => Visibility::Gone,
    };
    with_gfx(|g| g.set_visibility(Handle::from_java(id), v));
    Ok(None)
}

/// Shared body for `View.nativeGetLeft/GetTop/GetWidth/GetHeight` — reads the
/// laid-out frame (forcing a layout pass) and returns one component.
fn frame_component(
    args: &[Value],
    objects: &ObjectHeap,
    pick: fn((i32, i32, i32, i32)) -> i32,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let frame = with_gfx(|g| g.frame(Handle::from_java(id)));
    Ok(Some(Value::Int(pick(frame))))
}

pub fn get_left(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    frame_component(args, objects, |f| f.0)
}

pub fn get_top(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    frame_component(args, objects, |f| f.1)
}

pub fn get_width(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    frame_component(args, objects, |f| f.2)
}

pub fn get_height(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    frame_component(args, objects, |f| f.3)
}

/// `View.isInTouchMode()`: a build fact — true unless a button moves the
/// focus ring (`has_nav_keys`, docs/designs/app-portability-2026-10.md K6).
pub fn is_in_touch_mode() -> Result<Option<Value>, JvmError> {
    Ok(Some(Value::Int(i32::from(
        !crate::board_cfg::input::HAS_NAV_KEYS,
    ))))
}

/// Decode a `ViewPropertyAnimator.PROPERTY_*` code (the Java side shares them
/// between the animator and `View`'s transform accessors) into the trait's
/// backend-neutral enum. Alpha / x / y have their own setters and never come
/// through here.
fn view_property(code: i32) -> Result<ViewProperty, JvmError> {
    match code {
        3 => Ok(ViewProperty::TranslationX),
        4 => Ok(ViewProperty::TranslationY),
        5 => Ok(ViewProperty::Rotation),
        6 => Ok(ViewProperty::ScaleX),
        7 => Ok(ViewProperty::ScaleY),
        _ => Err(JvmError::InvalidReference),
    }
}

/// `View.nativeSetProperty(int property, float value)` — backs
/// `setTranslationX/Y`, `setRotation`, `setScaleX/Y`.
pub fn set_property(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let prop = view_property(arg_int(args, 1)?)?;
    let value = arg_float(args, 2)?;
    with_gfx(|g| g.set_view_property(Handle::from_java(id), prop, value));
    Ok(None)
}

/// `View.nativeGetProperty(int property)` — backs the matching getters, and
/// reads the alpha back after a cancelled animation (code 0, which has no
/// setter here).
pub fn get_property(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let prop = match arg_int(args, 1)? {
        0 => ViewProperty::Alpha,
        code => view_property(code)?,
    };
    let value = with_gfx(|g| g.get_view_property(Handle::from_java(id), prop));
    Ok(Some(Value::Float(value)))
}

/// `View.setPadding(int left, int top, int right, int bottom)`
pub fn set_padding(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let left = arg_int(args, 1)?;
    let top = arg_int(args, 2)?;
    let right = arg_int(args, 3)?;
    let bottom = arg_int(args, 4)?;
    with_gfx(|g| g.set_padding(Handle::from_java(id), left, top, right, bottom));
    Ok(None)
}

/// `View.setEnabled(boolean enabled)`
pub fn set_enabled(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let on = arg_int(args, 1)? != 0;
    with_gfx(|g| g.set_enabled(Handle::from_java(id), on));
    Ok(None)
}

/// `View.nativeSetMinimumSize(int width, int height)`: the floors
/// `setMinimumWidth` / `setMinimumHeight` set, -1 for an axis left alone.
pub fn set_minimum_size(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let width = arg_int(args, 1)?;
    let height = arg_int(args, 2)?;
    with_gfx(|g| g.set_min_size(Handle::from_java(id), width, height));
    Ok(None)
}

/// `View.nativeSetMaxWidth(int)`: `TextView.setMaxWidth`'s ceiling.
pub fn set_max_width(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let width = arg_int(args, 1)?;
    with_gfx(|g| g.set_max_width(Handle::from_java(id), width));
    Ok(None)
}

/// `View.nativeSetKeepScreenOn(boolean)`: hold the display on while this
/// view lives (power.rs; docs/designs/app-portability-2026-10.md K5).
pub fn set_keep_screen_on(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let on = arg_int(args, 1)? != 0;
    super::lvgl::keep_on::set(id, on);
    Ok(None)
}

/// `View.setAlpha(float alpha)`
pub fn set_alpha(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let alpha = (arg_float(args, 1)? * 255.0) as u8;
    with_gfx(|g| g.set_alpha(Handle::from_java(id), alpha));
    Ok(None)
}

/// `View.nativeClose()` — the widget-freeing half of `View.close()`. The
/// Java side has already left any parent's child list (a parented view goes
/// through `ViewGroup.removeView` instead and never reaches here).
pub fn close(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    with_gfx(|g| g.delete(Handle::from_java(id)));
    Ok(None)
}

// ---------------------------------------------------------------------------
// Listener Java bindings — the trampolines live in `lvgl::events` and
// `lvgl::widgets::button`.
// ---------------------------------------------------------------------------

/// `View.nativeRegisterKeyListener()` — binds this View to its widget so the
/// framework event loop can dispatch `fireKey()` when the widget is
/// LVGL-focused. No trampoline: the shared keypad indev feeds the key queue.
pub fn register_key_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, NO_ATTACH)
}

/// `View.nativeSetFocusable(boolean)` — add this view to (or remove it from)
/// the active Activity's keypad focus group, so it becomes eligible for
/// hardware-key focus. Mirrors `android.view.View#setFocusable`.
pub fn set_focusable(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let on = arg_int(args, 1)? != 0;
    lvgl_events::set_view_focusable(id, on);
    Ok(None)
}

/// `View.nativeRequestFocus()` — focus this view in the active keypad group.
/// Returns 1 if it became the focused view, 0 otherwise (mirrors
/// `android.view.View#requestFocus` returning a boolean).
pub fn request_focus(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let focused = lvgl_events::request_view_focus(id);
    Ok(Some(Value::Int(if focused { 1 } else { 0 })))
}

/// `View.nativeRegisterTouchListener()` — binds this View as the
/// touch-event receiver and wires the LVGL press/release/long-press
/// callbacks. Side effect: flips on `LV_OBJ_FLAG_CLICKABLE` so passive
/// widgets (TextView, layouts) actually receive hit-tested events.
pub fn register_touch_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, lvgl_events::attach_touch)
}

/// `View.nativeRegisterSwipeListener()` — binds this View as the
/// swipe-gesture receiver and registers the LVGL `LV_EVENT_GESTURE` callback
/// on the underlying object. Note: the widget itself doesn't need to be
/// clickable for gestures to fire (LVGL routes gestures via the indev,
/// not the hit-test path), so we don't touch the CLICKABLE flag here.
pub fn register_swipe_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, lvgl_events::attach_swipe)
}

/// `View.nativeRegisterClickListener()` — binds this View as the click
/// receiver, sets the LVGL CLICKABLE flag and attaches the click-event
/// trampoline so any view (not just Button) becomes clickable. Any number
/// of views may have one: the binding lives in the widget's own slot.
pub fn register_click_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, lvgl_button::attach_click)
}

/// `View.nativeRegisterLongClickListener()` — instance method; binds `this`
/// as the long-click receiver and wires the LVGL long-press trampoline.
pub fn register_long_click_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, lvgl_button::attach_long_click)
}

/// `View.performLongClickNative()` — synthesize an LVGL long-press for
/// scripted tests (the public `performLongClick` is pure Java).
pub fn perform_long_press(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_button::perform_long_press(id);
    Ok(None)
}

/// `View.nativeIsFocused()` — whether this view is the active keypad group's
/// focused widget. Returns 1 if focused, 0 otherwise. Mirrors
/// `android.view.View#isFocused`.
pub fn is_focused(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let focused = lvgl_events::view_is_focused(id);
    Ok(Some(Value::Int(if focused { 1 } else { 0 })))
}

/// `View.nativeRegisterFocusChangeListener()` — binds this View as the
/// focus-change receiver and attaches the LVGL FOCUSED/DEFOCUSED callbacks so
/// the framework loop can dispatch `View.fireFocusChange(boolean)`.
pub fn register_focus_change_listener(
    args: &[Value],
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    register_listener_with(args, objects, lvgl_events::attach_focus_change)
}

/// `View.performClick()` — synthesize a click event on this view.
pub fn perform_click(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    lvgl_button::perform_click(id);
    Ok(None)
}

/// `View.nativeSetWeight(int milli)`: this view's `LinearLayout` weight, in thousandths. Called
/// by `ViewGroup.addView(child, LinearLayout.LayoutParams)` when the params carry a weight.
pub fn set_weight(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let milli = arg_int(args, 1)?;
    with_gfx(|g| g.set_weight(Handle::from_java(id), milli));
    Ok(None)
}

/// `View.nativeSetLayoutGravity(int gravity)`: this view's `layout_gravity` in its `LinearLayout`.
pub fn set_layout_gravity(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let gravity = arg_int(args, 1)?;
    with_gfx(|g| g.set_layout_gravity(Handle::from_java(id), gravity));
    Ok(None)
}

/// `View.nativeSetMargins(int left, int top, int right, int bottom)`
pub fn set_margins(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (l, t, r, b) = (
        arg_int(args, 1)?,
        arg_int(args, 2)?,
        arg_int(args, 3)?,
        arg_int(args, 4)?,
    );
    with_gfx(|g| g.set_margins(Handle::from_java(id), l, t, r, b));
    Ok(None)
}

/// `View.nativeSetRelativeRules(int id, int flags, int[] anchors)`: a `RelativeLayout` child's
/// rules, the anchors by runtime slot (`relative.rs` `A_*`), null for none.
pub fn set_relative_rules(
    args: &[Value],
    arrays: &pico_jvm::array_heap::ArrayHeap,
    objects: &ObjectHeap,
) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (view_id, flags) = (arg_int(args, 1)?, arg_int(args, 2)?);
    let mut anchors = [0i32; 8];
    if let Some(Value::ArrayRef(a)) = args.get(3) {
        for (k, slot) in anchors.iter_mut().enumerate() {
            *slot = arrays.load(*a, k).unwrap_or(0);
        }
    }
    with_gfx(|g| g.set_relative_rules(Handle::from_java(id), view_id, flags, anchors));
    Ok(None)
}

/// `View.nativeSetFrameGravity(int gravity, int dx, int dy)`
pub fn set_frame_gravity(args: &[Value], objects: &ObjectHeap) -> Result<Option<Value>, JvmError> {
    let id = extract_native_handle(args, objects)?;
    let (gravity, dx, dy) = (arg_int(args, 1)?, arg_int(args, 2)?, arg_int(args, 3)?);
    with_gfx(|g| g.set_frame_gravity(Handle::from_java(id), gravity, dx, dy));
    Ok(None)
}

/// Reset the touch-event queue between app runs.
pub fn reset_touch_listener_state() {
    lvgl_events::reset_view_touch_listener_state();
}

/// Reset the swipe-event queue between app runs.
pub fn reset_swipe_listener_state() {
    lvgl_events::reset_view_swipe_listener_state();
}

/// Reset the focus-change-event queue between app runs.
pub fn reset_focus_change_listener_state() {
    lvgl_events::reset_view_focus_listener_state();
}
