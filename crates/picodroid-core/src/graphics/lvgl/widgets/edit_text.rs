// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `EditText` (LVGL `lv_textarea`).
//!
//! Auto-show keyboard wiring lives here too: every textarea created
//! through this module gets an `LV_EVENT_PRESSED` trampoline registered
//! that — gated by the per-handle [`AUTOSHOW_DISABLED`] map — calls
//! [`super::keyboard::show_system_for`] when the user taps it.

use crate::lvgl_ffi::*;

use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use super::keyboard;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use crate::util::local_set::LocalSet;

// ── Auto-show opt-out registry ──────────────────────────────────────────────
//
// Default is "auto-show enabled" — we track *opt-outs* (the negative
// sense) so a freshly-created EditText behaves correctly without any
// register call from the Java side. Only EditTexts that explicitly call
// `setShowKeyboardOnTouch(false)` end up in this list.

const MAX_AUTOSHOW_OPTOUTS: usize = 16;
struct State {
    autoshow_disabled: LocalSet<usize, MAX_AUTOSHOW_OPTOUTS>,
    numeric_fields: LocalSet<usize, MAX_NUMERIC_FIELDS>,
    editor_action_map: PtrMap<MAX_EDITOR_ACTION_LISTENERS>,
    text_watch_map: PtrMap<MAX_TEXT_WATCHERS>,
    text_queue: LocalRing<usize, TEXT_QUEUE_SIZE>,
}

// SAFETY: filled by natives and LVGL's textarea callbacks, drained by the
// framework loop, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        autoshow_disabled: LocalSet::new(0),
        numeric_fields: LocalSet::new(0),
        editor_action_map: PtrMap::new(),
        text_watch_map: PtrMap::new(),
        text_queue: LocalRing::new(0),
    })
};

fn is_autoshow_disabled(raw_ptr: usize) -> bool {
    STATE.autoshow_disabled.contains(raw_ptr)
}

// ── Numeric-input registry (raw lv_obj_t* of EditTexts that want a number pad) ──
//
// Populated by `EditText.setInputType(TYPE_CLASS_NUMBER)`. The shared system
// keyboard reads this in `show_system_for` to pick LV_KEYBOARD_MODE_NUMBER vs
// the default text layout for the field it's binding to.

const MAX_NUMERIC_FIELDS: usize = 16;

/// Whether the textarea at `raw_ptr` was marked numeric (digits-only keypad).
pub(in crate::graphics) fn is_numeric(raw_ptr: usize) -> bool {
    STATE.numeric_fields.contains(raw_ptr)
}

/// Mark/clear the EditText `id` as numeric. Idempotent in both directions.
pub(in crate::graphics) fn set_numeric(id: i32, numeric: bool) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    let raw_ptr = o.addr();
    // A full registry drops the request, as it always has.
    if numeric {
        STATE.numeric_fields.insert(raw_ptr);
    } else {
        STATE.numeric_fields.remove(raw_ptr);
    }
}

// ── EditorActionListener registry (raw lv_obj_t* → Java EditText obj_ref) ───
//
// Populated by [`register_editor_action_listener`], called from
// `EditText.setOnEditorActionListener`. EditTexts without a listener have
// no entry; their auto-show calls pass `obj_ref = 0`, and the system
// keyboard's OK callback will skip editor-action dispatch.

const MAX_EDITOR_ACTION_LISTENERS: usize = 16;

fn lookup_editor_action_obj(raw_ptr: usize) -> u16 {
    STATE.editor_action_map.lookup(raw_ptr).unwrap_or(0)
}

pub(in crate::graphics) fn register_editor_action_listener(id: i32, obj_ref: u16) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    if let Upsert::Full = STATE.editor_action_map.upsert(o.addr(), obj_ref) {
        warn_full("editor-action");
    }
}

// ── TextWatcher registry (raw lv_obj_t* → Java EditText obj_ref) + queue ────
//
// Populated by [`register_text_changed_listener`], called from
// `EditText.addTextChangedListener`. The LV_EVENT_VALUE_CHANGED callback is
// installed lazily on first registration so unwatched fields never enqueue.

const MAX_TEXT_WATCHERS: usize = 16;

const TEXT_QUEUE_SIZE: usize = 16;

lv_event_cb! {
    fn textarea_value_changed_cb(e) {
        let ta = e.target();
        // Coalesce: the Java side re-reads the final text at dispatch time,
        // so collapsing a burst of per-keystroke events into one queue entry
        // is lossless and keeps fast typing from overflowing the ring.
        if STATE.text_queue.any(|queued| queued == ta.addr()) {
            return;
        }
        STATE.text_queue.push(ta.addr());
    }
}

pub(in crate::graphics) fn register_text_changed_listener(id: i32, obj_ref: u16) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    match STATE.text_watch_map.upsert(o.addr(), obj_ref) {
        // Event cb already installed.
        Upsert::Updated => {}
        Upsert::Full => warn_full("text-watcher"),
        Upsert::Inserted => add_event_cb(o, textarea_value_changed_cb, LV_EVENT_VALUE_CHANGED),
    }
}

pub fn drain_text_changed_queue() -> Option<usize> {
    STATE.text_queue.pop()
}

pub fn lookup_text_watch_obj(handle: usize) -> Option<u16> {
    STATE.text_watch_map.lookup(handle)
}

/// GC roots for the TextWatcher map — same contract as
/// [`visit_editor_action_listener_roots`].
pub fn visit_text_changed_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.text_watch_map.visit(visit)
}

lv_event_cb! {
    /// Purge the dying textarea from every per-widget registry in this module:
    /// the two listener maps plus the raw-ptr opt-out/numeric lists (a stale ptr
    /// there would apply this field's flags to a recycled address).
    fn edit_text_delete_cb(e) {
        let obj = e.target().addr();
        STATE.editor_action_map.remove(obj);
        STATE.text_watch_map.remove(obj);
        STATE.autoshow_disabled.remove(obj);
        STATE.numeric_fields.remove(obj);
    }
}

lv_event_cb! {
    fn textarea_pressed_cb(e) {
        // The textarea this callback is on (`create` registered it there).
        let ta = e.current_target();
        if is_autoshow_disabled(ta.addr()) {
            return;
        }
        let obj_ref = lookup_editor_action_obj(ta.addr());
        keyboard::show_system_for(ta, obj_ref);
    }
}

pub(in crate::graphics) fn create() -> i32 {
    let Some(ptr) = lifecycle::screen_ptr().and_then(|o| lv_textarea_create(o)) else {
        return 0;
    };
    {
        // EditText is documented as single-line. Without this the textarea is
        // multi-line, and on a keypad board the ENTER (X) that opens the soft
        // keyboard also inserts a newline — the cursor jumps to an empty second
        // line and the field looks cleared (its text becomes e.g. "30\n").
        // SAFETY: `ptr` was just made by `lv_textarea_create`.
        unsafe { lv_textarea_set_one_line(ptr, true) };
        // The theme styles textareas as card+pad_small (10 px pad + 2 px border
        // per side), so an app-set height like 26 px leaves ~2 px of content for
        // a 16 px font line. That vertical overflow draws a scrollbar and feeds
        // a self-sustaining bounce: scroll_to_cusor_pos re-runs on every
        // STYLE_CHANGED, and each animated scroll toggles LV_STATE_SCROLLED,
        // which fires STYLE_CHANGED again (lv_textarea.c:1010, lv_obj.c:1009).
        // pad_ver = 3 makes a 26 px field hold exactly one Montserrat-14 line
        // (26 - 2*3 - 2*2 = 16 = line height), vertically centered, with both
        // scroll checks quiet. Heights below 22 px would overflow again.
        lv_obj_set_style_pad_top(ptr, 3, 0);
        lv_obj_set_style_pad_bottom(ptr, 3, 0);
        // Android EditText never draws scrollbars. SCROLLABLE itself stays set:
        // horizontal scroll-to-cursor must keep working for long one-line text.
        lv_obj_set_scrollbar_mode(ptr, LV_SCROLLBAR_MODE_OFF);
        add_event_cb(ptr, textarea_pressed_cb, LV_EVENT_PRESSED);
        add_event_cb(ptr, edit_text_delete_cb, LV_EVENT_DELETE);
    }
    handle_table::register(ptr)
}

/// Toggle the auto-show-keyboard-on-touch behavior for `id`. Default
/// after `create` is "enabled"; calling `set_autoshow(id, false)` adds
/// the textarea to the opt-out registry, and `set_autoshow(id, true)`
/// removes it. Idempotent in both directions.
pub(in crate::graphics) fn set_autoshow(id: i32, enabled: bool) {
    let Some(o) = handle_table::get(id) else {
        return;
    };
    let raw_ptr = o.addr();
    // A full registry drops the request, as it always has.
    if enabled {
        STATE.autoshow_disabled.remove(raw_ptr);
    } else {
        STATE.autoshow_disabled.insert(raw_ptr);
    }
}

pub fn reset_edit_text_state() {
    STATE.autoshow_disabled.clear();
    STATE.editor_action_map.reset();
    STATE.numeric_fields.clear();
    STATE.text_watch_map.reset();
    STATE.text_queue.clear();
}

/// Visit the Java `EditText` object ref of every textarea registered for an
/// editor-action listener so the GC keeps it alive. An `EditText` referenced
/// only by this native map (no Java field; `addView` keeps it alive only
/// natively) would otherwise be swept on the first GC, its slot reused, and a
/// later dispatch resolves a dead ref → `NoSuchMethod`. See
/// `widgets::button::visit_click_listener_roots`.
pub fn visit_editor_action_listener_roots(visit: &mut dyn FnMut(u16)) {
    STATE.editor_action_map.visit(visit)
}

/// Mask the field's text (`InputType.TYPE_TEXT_VARIATION_PASSWORD`): LVGL
/// shows bullets, briefly revealing the last character typed, and
/// `getText()` still reads the real text.
pub(in crate::graphics) fn set_password(id: i32, masked: bool) {
    let Some(ta) = handle_table::get(id) else {
        return;
    };
    // SAFETY: an `EditText` native: its constructor gives `View` the
    // `lv_textarea` [`create`] made, and no other handle.
    unsafe { lv_textarea_set_password_mode(ta, masked) };
}

pub(in crate::graphics) fn set_text(id: i32, text: &str) {
    let Some(ta) = handle_table::get(id) else {
        return;
    };
    // SAFETY: an `EditText` native: its constructor gives `View` the
    // `lv_textarea` [`create`] made, and no other handle.
    // The text is NUL-terminated and LVGL copies it.
    super::text_view::with_cstr(text, |p| unsafe { lv_textarea_set_text(ta, p.as_ptr()) });
}

pub(in crate::graphics) fn set_hint(id: i32, hint: &str) {
    let Some(ta) = handle_table::get(id) else {
        return;
    };
    // SAFETY: an `EditText` native: its constructor gives `View` the
    // `lv_textarea` [`create`] made, and no other handle.
    // The text is NUL-terminated and LVGL copies it.
    super::text_view::with_cstr(hint, |p| unsafe {
        lv_textarea_set_placeholder_text(ta, p.as_ptr())
    });
}

/// Read the current textarea content into `dst` (capped at 256 bytes).
/// Returns the byte length written, or `None` if the textarea is empty
/// or LVGL returned a null pointer.
/// Run `f` over the field's current text, whatever its length. `None`
/// when LVGL holds no text for it.
pub(in crate::graphics) fn with_text<R>(id: i32, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
    let ta = handle_table::get(id)?;
    // SAFETY: an `EditText` native: its constructor gives `View` the
    // `lv_textarea` [`create`] made, and no other handle.
    let cstr = unsafe { lv_textarea_get_text(ta) };
    if cstr.is_null() {
        return None;
    }
    // SAFETY: LVGL's own NUL-terminated text buffer, read before anything
    // can edit the field again (`f` reads it into the Java string).
    Some(f(unsafe { super::text_view::cstr_bytes(cstr) }))
}
