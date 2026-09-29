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
//! The system keyboard tracks the currently-bound EditText's Java
//! ObjectRef so the OK key can dispatch `EditText.fireEditorAction`
//! through [`drain_editor_action`] without a per-instance Java listener.

use crate::lvgl_ffi::*;

use super::super::animations;
use super::super::events;
use super::super::handle_table;
use super::super::lifecycle;
use super::super::listener_map::{warn_full, PtrMap, Upsert};
use crate::util::local::Core0;
use crate::util::local_ring::LocalRing;
use core::cell::Cell;

// ── READY event ring buffer (per-instance only) ─────────────────────────────

const READY_QUEUE_SIZE: usize = 16;
// SAFETY: filled by LVGL callbacks and drained by natives, all on JVM tasks.
static READY_QUEUE: Core0<LocalRing<usize, READY_QUEUE_SIZE>> =
    unsafe { Core0::new(LocalRing::new(0)) };

const MAX_KEYBOARDS: usize = 4;
// SAFETY: a listener registry, reached only from JVM tasks.
static KEYBOARD_HANDLE_MAP: Core0<PtrMap<MAX_KEYBOARDS>> = unsafe { Core0::new(PtrMap::new()) };

unsafe extern "C" fn map_delete_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) } as usize;
    KEYBOARD_HANDLE_MAP.remove(obj)
}

unsafe extern "C" fn keyboard_ready_cb(e: *mut lv_event_t) {
    let obj = unsafe { lv_event_get_target_obj(e) };
    READY_QUEUE.push(obj as usize);
}

// ── System keyboard singleton ───────────────────────────────────────────────
//
// Lazy-created on the first show_system_for() call; persists for the rest
// of the app run. The visible-state mirror is tracked in Rust because the
// existing FFI doesn't expose `lv_obj_has_flag` and growing the surface
// for one read isn't worth it.

const SYSTEM_KEYBOARD_SLIDE_DURATION_MS: u32 = 200;

/// The system keyboard's height: seven twelfths of the display, kept
/// between 140 px (the 240 px panels, where the form keeps the top 100 px)
/// and 200 px (a 480 px panel needs no more for four rows of keys).
fn system_keyboard_height() -> i32 {
    (crate::hal::display::HEIGHT as i32 * 7 / 12).clamp(140, 200)
}

/// Where the keyboard rests: flush with the bottom edge, full width.
fn system_keyboard_rest_y() -> i32 {
    crate::hal::display::HEIGHT as i32 - system_keyboard_height()
}

/// Off the bottom edge, where the slide starts.
fn system_keyboard_offscreen_y() -> i32 {
    crate::hal::display::HEIGHT as i32
}

// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD: Core0<Cell<*mut lv_obj_t>> =
    unsafe { Core0::new(Cell::new(core::ptr::null_mut())) };
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_HANDLE: Core0<Cell<i32>> = unsafe { Core0::new(Cell::new(0)) };
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_VISIBLE: Core0<Cell<bool>> = unsafe { Core0::new(Cell::new(false)) };
/// Java `ObjectRef` of the EditText that triggered the most recent show.
/// Read by [`drain_editor_action`] after the OK key fires.
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_BOUND_ET: Core0<Cell<u16>> = unsafe { Core0::new(Cell::new(0)) };
/// Raw `lv_textarea` the keyboard is currently bound to (mirrors LVGL's internal
/// `kb->ta`). The keyboard outlives any one Activity (it is parented to the
/// screen, not the content view), so when that textarea's Activity is torn down
/// this must be cleared *before* the textarea is freed — otherwise the next
/// `lv_keyboard_set_textarea` defocuses a dangling textarea (use-after-free).
/// See [`unbind_if_deleting`].
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_BOUND_TA: Core0<Cell<*mut lv_obj_t>> =
    unsafe { Core0::new(Cell::new(core::ptr::null_mut())) };
/// The keyboard was just shown and should take the keypad focus once the
/// key that opened it has been released (`take_pending_focus`).
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_FOCUS_PENDING: Core0<Cell<bool>> = unsafe { Core0::new(Cell::new(false)) };
/// The key the keypad's walk starts on: the first character of the layout
/// the keyboard was shown in — `q` of the text layout (key 0 is its mode
/// switch), `1` of the digit pad.
// SAFETY: widget-layer state, reached only from JVM tasks.
static SYSTEM_KEYBOARD_FIRST_KEY: Core0<Cell<u32>> = unsafe { Core0::new(Cell::new(1)) };

/// Pending editor-action record drained from the JVM event pump in
/// `lifecycle::dispatch_editor_actions`. Single-slot: the OK key can only
/// fire once per visibility cycle, and the slot is consumed before the
/// next show.
#[derive(Copy, Clone)]
pub struct EditorActionRecord {
    pub edit_text_ref: u16,
    pub action_id: i32,
}

// SAFETY: widget-layer state, reached only from JVM tasks.
static PENDING_EDITOR_ACTION: Core0<Cell<Option<EditorActionRecord>>> =
    unsafe { Core0::new(Cell::new(None)) };

unsafe extern "C" fn system_keyboard_ready_cb(_e: *mut lv_event_t) {
    // OK key on the system keyboard: queue an editor-action for dispatch
    // and dismiss. The Java listener may return `true` to suppress the
    // dismiss — that decision is made by the dispatch site after Java
    // returns, so we hide first and the dispatch site re-shows if needed.
    // For v1 we always hide; suppressing dismiss is a follow-up if any
    // app actually requires it.
    if SYSTEM_KEYBOARD_BOUND_ET.get() != 0 {
        PENDING_EDITOR_ACTION.set(Some(EditorActionRecord {
            edit_text_ref: SYSTEM_KEYBOARD_BOUND_ET.get(),
            action_id: 6, // EditorInfo.IME_ACTION_DONE
        }));
    }
    hide_system();
}

unsafe extern "C" fn screen_press_outside_cb(e: *mut lv_event_t) {
    let target = unsafe { lv_event_get_target_obj(e) };
    if target.is_null() {
        return;
    }
    let kb = SYSTEM_KEYBOARD.get();
    if kb.is_null() {
        return;
    }
    // Walk parent chain — if we hit the keyboard, this press was on the
    // keyboard itself or one of its keys, so do nothing. Otherwise the
    // tap landed outside and we dismiss.
    let mut cur: *mut lv_obj_t = target;
    while !cur.is_null() {
        if cur == kb {
            return;
        }
        cur = unsafe { lv_obj_get_parent(cur) };
    }
    hide_system();
}

unsafe fn ensure_system_keyboard() -> *mut lv_obj_t {
    unsafe {
        if !SYSTEM_KEYBOARD.get().is_null() {
            return SYSTEM_KEYBOARD.get();
        }
        let scr = lifecycle::screen_ptr();
        let kb = lv_keyboard_create(scr);
        // The constructor aligns a keyboard to the parent's BOTTOM_MID, which
        // turns every `set_y` into an offset *below* the bottom edge: at rest
        // y=100 the keyboard used to sit at 200, mostly off screen. Back to
        // the top-left origin, then an explicit size + position so we don't
        // depend on LVGL's default heuristics (which vary by version): full
        // width, flush with the bottom edge. On a 240x240 panel this leaves
        // the top 100px for the form.
        lv_obj_set_align(kb, LV_ALIGN_TOP_LEFT);
        lv_obj_set_pos(kb, 0, system_keyboard_rest_y());
        lv_obj_set_size(
            kb,
            crate::hal::display::WIDTH as i32,
            system_keyboard_height(),
        );
        lv_obj_set_style_bg_opa(kb, LV_OPA_COVER, 0);
        // Hidden until the first show_system_for call.
        lv_obj_add_flag(kb, LV_OBJ_FLAG_HIDDEN);
        lv_obj_add_event_cb(
            kb,
            Some(system_keyboard_ready_cb),
            LV_EVENT_READY,
            core::ptr::null_mut(),
        );
        SYSTEM_KEYBOARD.set(kb);
        // Register the keyboard in the handle table so `animations::start`
        // can locate it by handle on each tick.
        SYSTEM_KEYBOARD_HANDLE.set(handle_table::register(kb));
        kb
    }
}

/// Show the system keyboard, binding it to `ta` and recording the Java
/// EditText `obj_ref` for later editor-action dispatch. Lazy-creates the
/// keyboard on the first call. Slides up from the screen edge on a
/// hidden→visible transition; a re-show while already visible re-runs
/// the slide for visual feedback. Called from the EditText auto-show
/// trampoline in [`super::edit_text`], its only caller — hence
/// `pub(in crate::graphics)` rather than `pub`: `ta` is an LVGL object
/// pointer from the handle table, and nothing outside this tree should be
/// able to hand one in.
pub(in crate::graphics) fn show_system_for(ta: *mut lv_obj_t, et_obj_ref: u16) {
    if ta.is_null() {
        return;
    }
    unsafe {
        let kb = ensure_system_keyboard();
        lv_keyboard_set_textarea(kb, ta);
        SYSTEM_KEYBOARD_BOUND_TA.set(ta); // mirror kb->ta for use-after-delete cleanup
                                          // Pick the keypad layout for the field being bound. EditTexts flagged
                                          // numeric (setInputType TYPE_CLASS_NUMBER) get the digit pad; everything
                                          // else gets the default text layout. Set every show because the system
                                          // keyboard is shared across fields.
        let numeric = super::edit_text::is_numeric(ta as usize);
        let mode = if numeric {
            LV_KEYBOARD_MODE_NUMBER
        } else {
            LV_KEYBOARD_MODE_TEXT_LOWER
        };
        lv_keyboard_set_mode(kb, mode);
        SYSTEM_KEYBOARD_FIRST_KEY.set(if numeric { 0 } else { 1 });
        SYSTEM_KEYBOARD_BOUND_ET.set(et_obj_ref);
        // Visibility flag must be cleared *before* starting the y-anim,
        // otherwise the first frame paints at the off-screen y position
        // while still HIDDEN — fine — and then becomes visible mid-slide
        // which produces a pop. Clearing first means LVGL renders every
        // frame of the slide.
        lv_obj_remove_flag(kb, LV_OBJ_FLAG_HIDDEN);
        lv_obj_set_y(kb, system_keyboard_offscreen_y());
        animations::start(
            SYSTEM_KEYBOARD_HANDLE.get(),
            /* PROPERTY_Y */ 2,
            system_keyboard_offscreen_y(),
            system_keyboard_rest_y(),
            SYSTEM_KEYBOARD_SLIDE_DURATION_MS,
            /* INTERP_LINEAR */ 0,
        );
        SYSTEM_KEYBOARD_VISIBLE.set(true);
        // On a keypad board the keyboard takes the focus while it shows:
        // `keypad_remap` then turns PREV/NEXT into LEFT/RIGHT, which walk
        // its keys, and ENTER presses the selected one (LVGL's own path).
        // Not yet, though: the ENTER that opened it is still down, and its
        // release would land on the keyboard's first key. `keypad_read_cb`
        // moves the focus on its next quiet pass (`take_pending_focus`).
        SYSTEM_KEYBOARD_FOCUS_PENDING.set(true);
        // Attach press-outside dismiss after the EditText's PRESSED event
        // has finished bubbling — the screen-level callback will not
        // receive the same press that opened us.
        events::attach_screen_press_hook(Some(screen_press_outside_cb));
    }
}

/// Hide the system keyboard if currently visible. Returns `true` if a
/// hide actually happened — used by the BACK-key intercept in
/// `lifecycle.rs::dispatch_key_events` to decide whether to consume the
/// event.
pub fn hide_system() -> bool {
    unsafe {
        if SYSTEM_KEYBOARD.get().is_null() || !SYSTEM_KEYBOARD_VISIBLE.get() {
            return false;
        }
        // Cancel any in-flight slide so the keyboard's saved y doesn't
        // creep back to the rest position after the next show. The next
        // show_system_for re-snaps y=offscreen explicitly anyway, so
        // this is belt-and-braces.
        animations::cancel(SYSTEM_KEYBOARD_HANDLE.get());
        lv_obj_add_flag(SYSTEM_KEYBOARD.get(), LV_OBJ_FLAG_HIDDEN);
        SYSTEM_KEYBOARD_VISIBLE.set(false);
        SYSTEM_KEYBOARD_BOUND_ET.set(0);
        SYSTEM_KEYBOARD_FOCUS_PENDING.set(false);
        events::detach_screen_press_hook();
        // The field it was typing into takes the keypad focus back. The
        // bound textarea is live: `unbind_if_deleting` clears it before its
        // Activity's tree is freed.
        events::unfocus_system_keyboard(SYSTEM_KEYBOARD.get(), SYSTEM_KEYBOARD_BOUND_TA.get());
        true
    }
}

/// The keyboard waiting to take the keypad focus, if one is, and the key
/// its walk starts on: shown by a key press whose release has now been
/// read. Clears the wait; the caller (`keypad_read_cb`, on a pass with no
/// edge and no key down) does the focusing.
pub fn take_pending_focus() -> Option<(*mut lv_obj_t, u32)> {
    if !SYSTEM_KEYBOARD_FOCUS_PENDING.get() || !SYSTEM_KEYBOARD_VISIBLE.get() {
        return None;
    }
    SYSTEM_KEYBOARD_FOCUS_PENDING.set(false);
    Some((SYSTEM_KEYBOARD.get(), SYSTEM_KEYBOARD_FIRST_KEY.get()))
}

/// Whether the system keyboard is showing and holds the keypad focus: the
/// condition under which `keypad_read_cb` walks its keys with PREV/NEXT.
pub fn is_system_keyboard_focused() -> bool {
    unsafe {
        if SYSTEM_KEYBOARD.get().is_null() || !SYSTEM_KEYBOARD_VISIBLE.get() {
            return false;
        }
        let group = lv_group_get_default();
        !group.is_null() && lv_group_get_focused(group) == SYSTEM_KEYBOARD.get()
    }
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
    unsafe {
        if !SYSTEM_KEYBOARD_VISIBLE.get() || SYSTEM_KEYBOARD_BOUND_TA.get().is_null() {
            return false;
        }
        let ta = SYSTEM_KEYBOARD_BOUND_TA.get();
        super::text_view::with_cstr(text, |p| lv_textarea_add_text(ta, p));
    }
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
pub fn unbind_if_deleting(root: *mut lv_obj_t) {
    if root.is_null() {
        return;
    }
    unsafe {
        let kb = SYSTEM_KEYBOARD.get();
        let ta = SYSTEM_KEYBOARD_BOUND_TA.get();
        if kb.is_null() || ta.is_null() {
            return;
        }
        // Walk up from the bound textarea; if we reach `root` it is in the
        // subtree being deleted, so unbind it now (still alive == safe).
        let mut cur = ta;
        while !cur.is_null() {
            if cur == root {
                lv_keyboard_set_textarea(kb, core::ptr::null_mut());
                SYSTEM_KEYBOARD_BOUND_TA.set(core::ptr::null_mut());
                let _ = hide_system();
                return;
            }
            cur = lv_obj_get_parent(cur);
        }
    }
}

/// Pop the pending editor-action, if any. The JVM event pump in
/// `lifecycle.rs` consumes this every tick.
pub fn drain_editor_action() -> Option<EditorActionRecord> {
    // Manual take() — `Option::take` would require &mut to a mutable
    // static, which trips Rust 2024's `static_mut_refs` lint. The record
    // is `Copy`, so a load + store is equivalent and lint-clean.
    let popped = PENDING_EDITOR_ACTION.get();
    PENDING_EDITOR_ACTION.set(None);
    popped
}

// ── Per-instance widget ops (called from widgets/keyboard.rs Java shim) ─────

/// `Keyboard.nativeCreate()` — fresh per-instance keyboard parented to
/// the screen. Distinct from the system keyboard above.
pub(in crate::graphics) fn create() -> i32 {
    let kb = unsafe { lv_keyboard_create(lifecycle::screen_ptr()) };
    unsafe {
        // As for the system keyboard: `View.setPosition` is absolute.
        lv_obj_set_align(kb, LV_ALIGN_TOP_LEFT);
        lv_obj_add_event_cb(
            kb,
            Some(keyboard_ready_cb),
            LV_EVENT_READY,
            core::ptr::null_mut(),
        );
    }
    handle_table::register(kb)
}

pub(in crate::graphics) fn set_textarea(kb_id: i32, ta_id: i32) {
    let kb = handle_table::lookup(kb_id);
    let ta = handle_table::lookup(ta_id);
    if kb.is_null() || ta.is_null() {
        return;
    }
    unsafe { lv_keyboard_set_textarea(kb, ta) };
}

pub(in crate::graphics) fn set_mode(kb_id: i32, mode: lv_keyboard_mode_t) {
    let kb = handle_table::lookup(kb_id);
    if kb.is_null() {
        return;
    }
    unsafe { lv_keyboard_set_mode(kb, mode) };
}

/// Register a Java `Keyboard` object as the READY-listener target for
/// the given instance. Mirrors the Button pattern.
pub(in crate::graphics) fn register_ready_listener(id: i32, obj_ref: u16) {
    let raw_ptr = handle_table::lookup(id) as usize;
    if raw_ptr == 0 {
        return;
    }
    unsafe {
        match KEYBOARD_HANDLE_MAP.upsert(raw_ptr, obj_ref) {
            Upsert::Updated => {}
            Upsert::Full => warn_full("keyboard-ready"),
            Upsert::Inserted => {
                // Unregister on widget delete so a recycled lv_obj address
                // can't alias a dead widget's listener entry.
                lv_obj_add_event_cb(
                    raw_ptr as *mut lv_obj_t,
                    Some(map_delete_cb),
                    LV_EVENT_DELETE,
                    core::ptr::null_mut(),
                );
            }
        }
    }
}

/// Drain one READY event (raw `lv_obj_t*` value) from the per-instance
/// queue. Returns `None` when empty.
pub fn drain_ready_queue() -> Option<usize> {
    READY_QUEUE.pop()
}

/// Look up the Java `Keyboard` object index for a per-instance widget.
pub fn lookup_keyboard_obj(handle: usize) -> Option<u16> {
    KEYBOARD_HANDLE_MAP.lookup(handle)
}

pub fn reset_keyboard_state() {
    KEYBOARD_HANDLE_MAP.reset();
    READY_QUEUE.clear();
    // The screen tree is torn down by handle_table::reset on app
    // reload, so the system keyboard pointer is dangling — drop our
    // cache so the next show recreates from scratch.
    SYSTEM_KEYBOARD.set(core::ptr::null_mut());
    SYSTEM_KEYBOARD_HANDLE.set(0);
    SYSTEM_KEYBOARD_VISIBLE.set(false);
    SYSTEM_KEYBOARD_BOUND_ET.set(0);
    SYSTEM_KEYBOARD_BOUND_TA.set(core::ptr::null_mut());
    SYSTEM_KEYBOARD_FOCUS_PENDING.set(false);
    PENDING_EDITOR_ACTION.set(None);
    // Same lifetime as the system keyboard — the screen press hook is
    // attached only while the keyboard is visible, so the cached cb
    // pointer must die alongside the screen on reload.
    events::reset_screen_press_hook_state();
}
