// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `picodroid.widget.Keyboard` plus the auto-show "system
//! keyboard" singleton tapping into `EditText`.
//!
//! Two paths share this module:
//!
//! - **Explicit Keyboard widget**: each `new Keyboard()` from Java calls
//!   [`create`], which produces a fresh `lv_keyboard` parented to the
//!   active screen. Apps own its lifetime, position, and mode. READY
//!   events route to the per-instance Java listener via the ring buffer
//!   below (mirroring Button's click queue).
//! - **System keyboard**: a single shared `lv_keyboard` lazy-created on
//!   the first `EditText` tap (when auto-show is enabled). Reused across
//!   every EditText for the lifetime of the app run. Dismissed by BACK,
//!   the OK key, a tap outside the keyboard, or `EditText.hideKeyboard()`.
//!   Slides up from the screen edge on show; hide is instant.
//!
//! The OK key names the field the system keyboard types into by handle,
//! so `lifecycle::dispatch_editor_actions` reaches its Java `EditText`
//! through the handle table's binding and calls `fireEditorAction` there,
//! without a per-instance Java listener.

use crate::lvgl_ffi::*;

use super::super::animations;
use super::super::events;
use super::super::handle_table;
use super::super::lifecycle;
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

// ── READY event ring buffer (per-instance only) ─────────────────────────────

const READY_QUEUE_SIZE: usize = 16;

lv_event_cb! {
    fn keyboard_ready_cb(e) {
        let handle = handle_table::handle_of(e.target());
        if handle != 0 {
            STATE.ready_queue.push(handle);
        }
    }
}

// ── System keyboard singleton ───────────────────────────────────────────────
//
// Lazy-created on the first show_system_for() call; persists for the rest
// of the app run. The visible-state mirror is tracked in Rust because the
// existing FFI doesn't expose `lv_obj_has_flag` and growing the surface
// for one read isn't worth it.

const SYSTEM_KEYBOARD_SLIDE_DURATION_MS: u32 = 200;

/// The system keyboard's height: seven twelfths of the window, kept
/// between 140 px (the 240 px panels, where the form keeps the top 100 px)
/// and 200 px (a 480 px panel needs no more for four rows of keys).
fn system_keyboard_height() -> i32 {
    (lifecycle::window_size().1 * 7 / 12).clamp(140, 200)
}

/// Where the keyboard rests: flush with the bottom edge, full width.
fn system_keyboard_rest_y() -> i32 {
    lifecycle::window_size().1 - system_keyboard_height()
}

/// Off the bottom edge, where the slide starts.
fn system_keyboard_offscreen_y() -> i32 {
    lifecycle::window_size().1
}

/// Pending editor-action record drained from the JVM event pump in
/// `lifecycle::dispatch_editor_actions`. Single-slot: the OK key can only
/// fire once per visibility cycle, and the slot is consumed before the
/// next show.
#[derive(Copy, Clone)]
pub struct EditorActionRecord {
    /// The field's handle; its Java `EditText` is the handle table's binding.
    pub edit_text: i32,
    pub action_id: i32,
}

struct State {
    /// READY events of the per-instance keyboards, each the keyboard's handle.
    ready_queue: LocalRing<i32, READY_QUEUE_SIZE>,
    /// The system keyboard, `None` until the first show. Kept as an `Obj`
    /// across natives because nothing deletes it: it lives on the top
    /// layer, which nothing cleans, and a reset only forgets it.
    system: Cell<Option<Obj>>,
    system_handle: Cell<i32>,
    visible: Cell<bool>,
    /// Raw `lv_textarea` the keyboard is currently bound to (mirrors LVGL's
    /// internal `kb->ta`). The keyboard outlives any one Activity (it is
    /// parented to the screen, not the content view), so when that
    /// textarea's Activity is torn down this must be cleared *before* the
    /// textarea is freed — otherwise the next `lv_keyboard_set_textarea`
    /// defocuses a dangling textarea (use-after-free). See
    /// [`unbind_if_deleting`].
    bound_ta: Cell<*mut lv_obj_t>,
    /// The keyboard was just shown and should take the keypad focus once the
    /// key that opened it has been released (`take_pending_focus`).
    focus_pending: Cell<bool>,
    /// The key the keypad's walk starts on: the first character of the
    /// layout the keyboard was shown in — `q` of the text layout (key 0 is
    /// its mode switch), `1` of the digit pad.
    first_key: Cell<u32>,
    pending_editor_action: Cell<Option<EditorActionRecord>>,
}

// SAFETY: shown, hidden and bound by natives, read by LVGL's READY and
// press callbacks and by the keypad read callback, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        ready_queue: LocalRing::new(0),
        system: Cell::new(None),
        system_handle: Cell::new(0),
        visible: Cell::new(false),
        bound_ta: Cell::new(core::ptr::null_mut()),
        focus_pending: Cell::new(false),
        // Set by every show before `focus_pending` can be read.
        first_key: Cell::new(0),
        pending_editor_action: Cell::new(None),
    })
};

lv_event_cb! {
    fn system_keyboard_ready_cb(_e) {
        // OK key on the system keyboard: queue an editor-action for dispatch
        // and dismiss. The Java listener may return `true` to suppress the
        // dismiss — that decision is made by the dispatch site after Java
        // returns, so we hide first and the dispatch site re-shows if needed.
        // For v1 we always hide; suppressing dismiss is a follow-up if any
        // app actually requires it.
        let edit_text = bound_ta().map_or(0, handle_table::handle_of);
        if edit_text != 0 {
            STATE.pending_editor_action.set(Some(EditorActionRecord {
                edit_text,
                action_id: 6, // EditorInfo.IME_ACTION_DONE
            }));
        }
        hide_system();
    }
}

/// `LV_EVENT_PRESSED` on the pointer device while the keyboard is up
/// (`events::attach_pointer_press_hook`): the pressed object rides as the
/// event's parameter. A press on the keyboard or one of its keys, or on
/// the field it is typing into, keeps it up; a press anywhere else — the
/// app's own views included — dismisses it, as Android's does.
///
/// # Safety
///
/// Only LVGL calls this, as an `LV_EVENT_PRESSED` callback on the pointer
/// input device, with the event it is dispatching.
unsafe extern "C" fn pointer_press_outside_cb(e: *mut lv_event_t) {
    // SAFETY: LVGL runs this input-device callback with the event it is
    // dispatching; for `LV_EVENT_PRESSED` on a pointer device the parameter
    // is the object pressed, which LVGL holds for the dispatch.
    let Some(target) = (unsafe { Obj::from_raw(lv_event_get_param(e).cast()) }) else {
        return;
    };
    let Some(kb) = STATE.system.get() else {
        return;
    };
    let ta = STATE.bound_ta.get();
    let mut cur = Some(target);
    while let Some(o) = cur {
        if o == kb || (!ta.is_null() && o.as_ptr() == ta) {
            return;
        }
        cur = lv_obj_get_parent(o);
    }
    hide_system();
}

/// The textarea the system keyboard types into, if any.
fn bound_ta() -> Option<Obj> {
    // SAFETY: `bound_ta` names a live textarea or is null: it is set from
    // the live field `show_system_for` binds, and cleared by
    // `unbind_if_deleting`, which the framework's delete paths
    // (`view_ops::delete_obj`, which `window::sync` uses for the old compat
    // window too, and `remove_all_children`) run before LVGL frees the
    // field.
    unsafe { Obj::from_raw(STATE.bound_ta.get()) }
}

fn ensure_system_keyboard() -> Option<Obj> {
    if let Some(kb) = STATE.system.get() {
        return Some(kb);
    }
    // On the overlay layer, above the screen: the keyboard belongs to the
    // framework, not to the content root a `setContentView` replaces, and
    // it must not pan with an oversized root. See `lifecycle::overlay_layer`.
    let kb = lifecycle::overlay_layer().and_then(|o| lv_keyboard_create(o))?;
    // The constructor aligns a keyboard to the parent's BOTTOM_MID, which
    // turns every `set_y` into an offset *below* the bottom edge: at rest
    // y=100 the keyboard used to sit at 200, mostly off screen. Back to
    // the top-left origin, then an explicit size + position so we don't
    // depend on LVGL's default heuristics (which vary by version): full
    // width, flush with the bottom edge. On a 240x240 panel this leaves
    // the top 100px for the form.
    lv_obj_set_align(kb, LV_ALIGN_TOP_LEFT);
    lv_obj_set_pos(kb, 0, system_keyboard_rest_y());
    lv_obj_set_size(kb, lifecycle::window_size().0, system_keyboard_height());
    lv_obj_set_style_bg_opa(kb, LV_OPA_COVER, 0);
    // Hidden until the first show_system_for call.
    lv_obj_add_flag(kb, LV_OBJ_FLAG_HIDDEN);
    add_event_cb(kb, system_keyboard_ready_cb, LV_EVENT_READY);
    STATE.system.set(Some(kb));
    // Register the keyboard in the handle table so `animations::start`
    // can locate it by handle on each tick.
    STATE.system_handle.set(handle_table::register(kb));
    Some(kb)
}

/// Show the system keyboard, binding it to `ta`. Lazy-creates the
/// keyboard on the first call. Slides up from the screen edge on a
/// hidden→visible transition; a re-show while already visible re-runs
/// the slide for visual feedback. Called from the EditText auto-show
/// trampoline in [`super::edit_text`], its only caller — hence
/// `pub(in crate::graphics)` rather than `pub`: `ta` must be an EditText's
/// `lv_textarea`, and nothing outside this tree should be able to hand
/// one in.
pub(in crate::graphics) fn show_system_for(ta: Obj) {
    let Some(kb) = ensure_system_keyboard() else {
        return;
    };
    // SAFETY: `kb` is the system keyboard, made by `lv_keyboard_create`;
    // `ta` is the textarea of the EditText whose press callback calls this
    // (the callback's own object).
    unsafe { lv_keyboard_set_textarea(kb, Some(ta)) };
    STATE.bound_ta.set(ta.as_ptr()); // mirror kb->ta for use-after-delete cleanup

    // Pick the keypad layout for the field being bound. EditTexts flagged
    // numeric (setInputType TYPE_CLASS_NUMBER) get the digit pad; everything
    // else gets the default text layout. Set every show because the system
    // keyboard is shared across fields.
    let numeric = super::edit_text::is_numeric(ta.addr());
    let mode = if numeric {
        LV_KEYBOARD_MODE_NUMBER
    } else {
        LV_KEYBOARD_MODE_TEXT_LOWER
    };
    // SAFETY: `kb` is an `lv_keyboard` (above).
    unsafe { lv_keyboard_set_mode(kb, mode) };
    STATE.first_key.set(if numeric { 0 } else { 1 });
    // Visibility flag must be cleared *before* starting the y-anim,
    // otherwise the first frame paints at the off-screen y position
    // while still HIDDEN — fine — and then becomes visible mid-slide
    // which produces a pop. Clearing first means LVGL renders every
    // frame of the slide.
    lv_obj_remove_flag(kb, LV_OBJ_FLAG_HIDDEN);
    lv_obj_set_y(kb, system_keyboard_offscreen_y());
    animations::start(
        STATE.system_handle.get(),
        /* PROPERTY_Y */ 2,
        system_keyboard_offscreen_y(),
        system_keyboard_rest_y(),
        SYSTEM_KEYBOARD_SLIDE_DURATION_MS,
        /* INTERP_LINEAR */ 0,
    );
    STATE.visible.set(true);
    // The soft-nav control sits where the keyboard's bottom-left key
    // does; the keyboard has its own dismiss affordances.
    super::super::soft_nav::set_hidden(true);
    super::super::menu_button::set_hidden(true);
    // On a keypad board the keyboard takes the focus while it shows:
    // `keypad_remap` then turns PREV/NEXT into LEFT/RIGHT, which walk
    // its keys, and ENTER presses the selected one (LVGL's own path).
    // Not yet, though: the ENTER that opened it is still down, and its
    // release would land on the keyboard's first key. `keypad_read_cb`
    // moves the focus on its next quiet pass (`take_pending_focus`).
    STATE.focus_pending.set(true);
    // Press-outside dismiss, attached now that the press that opened
    // us has been and gone (we are shown from the field's click, after
    // LVGL's pass): the hook sees only the presses that follow.
    // SAFETY: `pointer_press_outside_cb` is written for the pointer device's
    // PRESSED event: it reads the parameter as the pressed object and never
    // asks for a target.
    unsafe { events::attach_pointer_press_hook(Some(pointer_press_outside_cb)) };
}

/// Hide the system keyboard if currently visible. Returns `true` if a
/// hide actually happened — used by the BACK-key intercept in
/// `lifecycle.rs::dispatch_key_events` to decide whether to consume the
/// event.
pub fn hide_system() -> bool {
    let Some(kb) = STATE.system.get() else {
        return false;
    };
    if !STATE.visible.get() {
        return false;
    }
    // Cancel any in-flight slide so the keyboard's saved y doesn't
    // creep back to the rest position after the next show. The next
    // show_system_for re-snaps y=offscreen explicitly anyway, so
    // this is belt-and-braces.
    animations::cancel(STATE.system_handle.get());
    lv_obj_add_flag(kb, LV_OBJ_FLAG_HIDDEN);
    STATE.visible.set(false);
    super::super::soft_nav::set_hidden(false);
    super::super::menu_button::set_hidden(false);
    STATE.focus_pending.set(false);
    events::detach_pointer_press_hook();
    // The field it was typing into takes the keypad focus back. The
    // bound textarea is live: `unbind_if_deleting` clears it before its
    // Activity's tree is freed.
    events::unfocus_system_keyboard(kb, bound_ta());
    true
}

/// The keyboard waiting to take the keypad focus, if one is, and the key
/// its walk starts on: shown by a key press whose release has now been
/// read. Clears the wait; the caller (`keypad_read_cb`, on a pass with no
/// edge and no key down) does the focusing.
pub fn take_pending_focus() -> Option<(Obj, u32)> {
    if !STATE.focus_pending.get() || !STATE.visible.get() {
        return None;
    }
    STATE.focus_pending.set(false);
    Some((STATE.system.get()?, STATE.first_key.get()))
}

/// Whether the system keyboard is showing and holds the keypad focus: the
/// condition under which `keypad_read_cb` walks its keys with PREV/NEXT.
pub fn is_system_keyboard_focused() -> bool {
    let Some(kb) = STATE.system.get() else {
        return false;
    };
    STATE.visible.get() && lv_group_get_default().and_then(|o| lv_group_get_focused(o)) == Some(kb)
}

/// The LVGL key a keypad edge becomes while the system keyboard has the
/// focus: PREV and NEXT walk the key grid as LEFT and RIGHT (a button
/// matrix moves its selection only on those, and the keypad indev turns
/// PREV/NEXT into focus moves before the widget sees them). `None` leaves
/// the key as it is.
pub fn keypad_remap(key: u32) -> Option<u32> {
    if !is_system_keyboard_focused() {
        return None;
    }
    match key {
        LV_KEY_PREV => Some(LV_KEY_LEFT),
        LV_KEY_NEXT => Some(LV_KEY_RIGHT),
        _ => None,
    }
}

/// Insert `text` into the field the system keyboard is typing into, as a
/// burst of key presses would — the simulator's `input text` verb. False
/// when the keyboard is not showing.
pub fn type_text(text: &str) -> bool {
    if !STATE.visible.get() {
        return false;
    }
    let Some(ta) = bound_ta() else {
        return false;
    };
    // SAFETY: the bound field is an EditText's `lv_textarea`
    // (`show_system_for`); the text is NUL-terminated and LVGL copies it.
    super::text_view::with_cstr(text, |p| unsafe { lv_textarea_add_text(ta, p.as_ptr()) });
    true
}

/// Drop the system keyboard's textarea binding when that textarea (or an
/// ancestor) is about to be deleted. MUST run from the view-delete path
/// *before* the LVGL objects are freed, while the textarea is still valid.
///
/// The keyboard is parented to the screen, so it outlives the Activity whose
/// field it was bound to; that field is freed on the Activity's teardown while
/// `kb->ta` still points at it. The next `show_system_for` then calls
/// `lv_keyboard_set_textarea`, which defocuses the *previous* textarea — a
/// use-after-free segfault (lv_keyboard_set_textarea → lv_obj_remove_state →
/// lv_event_send over a freed event list). Unbinding here, while the textarea
/// is still alive, defuses it. Mirrors [`super::super::animations::cancel_subtree`].
pub fn unbind_if_deleting(root: Obj) {
    let Some(kb) = STATE.system.get() else {
        return;
    };
    let Some(ta) = bound_ta() else {
        return;
    };
    // Walk up from the bound textarea; if we reach `root` it is in the
    // subtree being deleted, so unbind it now (still alive == safe).
    let mut cur = Some(ta);
    while let Some(o) = cur {
        if o == root {
            // SAFETY: `kb` is the system keyboard, made by
            // `lv_keyboard_create`.
            unsafe { lv_keyboard_set_textarea(kb, None) };
            STATE.bound_ta.set(core::ptr::null_mut());
            let _ = hide_system();
            return;
        }
        cur = lv_obj_get_parent(o);
    }
}

/// Pop the pending editor-action, if any. The JVM event pump in
/// `lifecycle.rs` consumes this every tick.
pub fn drain_editor_action() -> Option<EditorActionRecord> {
    STATE.pending_editor_action.take()
}

// ── Per-instance widget ops (called from widgets/keyboard.rs Java shim) ─────

/// `Keyboard.nativeCreate()` — fresh per-instance keyboard parented to
/// the screen. Distinct from the system keyboard above.
pub(in crate::graphics) fn create() -> i32 {
    let Some(kb) = lifecycle::screen_ptr().and_then(|o| lv_keyboard_create(o)) else {
        return 0;
    };
    // As for the system keyboard: `View.setPosition` is absolute.
    lv_obj_set_align(kb, LV_ALIGN_TOP_LEFT);
    add_event_cb(kb, keyboard_ready_cb, LV_EVENT_READY);
    handle_table::register(kb)
}

pub(in crate::graphics) fn set_textarea(kb_id: i32, ta_id: i32) {
    let (Some(kb), Some(ta)) = (handle_table::get(kb_id), handle_table::get(ta_id)) else {
        return;
    };
    // SAFETY: `Keyboard.setEditText(EditText)`: `kb` is a Keyboard's
    // `lv_keyboard` (its constructors pass [`create`]'s), and `ta` an
    // EditText's `lv_textarea` (the Java parameter type).
    unsafe { lv_keyboard_set_textarea(kb, Some(ta)) };
}

pub(in crate::graphics) fn set_mode(kb_id: i32, mode: lv_keyboard_mode_t) {
    let Some(kb) = handle_table::get(kb_id) else {
        return;
    };
    // SAFETY: a `Keyboard` native: its constructors give `View` the
    // `lv_keyboard` [`create`] made, and no other handle.
    unsafe { lv_keyboard_set_mode(kb, mode) };
}

/// Drain one READY event (the keyboard's handle) from the per-instance
/// queue. Returns `None` when empty.
pub fn drain_ready_queue() -> Option<i32> {
    STATE.ready_queue.pop()
}

pub fn reset_keyboard_state() {
    STATE.ready_queue.clear();
    // The system keyboard lives on the overlay layer, which no reload step
    // tears down (`handle_table::reset` forgets handles, it deletes no
    // object), so it is still live here: delete it, through the path every
    // framework delete takes, or each reload that showed one would leave a
    // keyboard in the LVGL pool with its callbacks attached. The next show
    // creates a fresh one.
    if let Some(kb) = STATE.system.take() {
        // SAFETY: `kb` was made by `lv_keyboard_create` and nothing has
        // deleted it since: it is on the overlay layer, which only this
        // function clears, and it is cleared from the cache first.
        unsafe { super::super::view_ops::delete_obj(kb) };
    }
    STATE.system_handle.set(0);
    STATE.visible.set(false);
    STATE.bound_ta.set(core::ptr::null_mut());
    STATE.focus_pending.set(false);
    STATE.pending_editor_action.set(None);
    // The press hook is attached only while the keyboard is visible; the
    // pointer device it sits on outlives the reload, so it is detached
    // rather than forgotten.
    events::detach_pointer_press_hook();
}
