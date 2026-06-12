// SPDX-License-Identifier: GPL-3.0-only
// Boards without buttons (`cfg(not(has_buttons))`) include this module but
// never call into it from `events.rs`. The dead_code lint can't see the
// has_buttons-gated callsite, so suppress it at module scope rather than
// duplicating the cfg gate on every item (same pattern as `key_filter.rs`).
#![cfg_attr(not(any(has_buttons, test)), allow(dead_code))]
//! Keypad edit mode for value widgets (NumberPicker).
//!
//! LVGL's keypad indev hard-disables group editing mode ("Editing is not used
//! by KEYPAD", lv_indev.c) — LV_KEY_PREV/NEXT *always* move focus. And the
//! key pipeline feeds the LVGL indev and the Java key queue in lockstep from
//! `events::keypad_read_cb`, so neither side can veto the other. This module
//! is the single interception point both paths share: while a NumberPicker is
//! being edited, PREV/NEXT become step requests (+1/-1) instead of focus
//! moves, and ENTER/ESC leave edit mode without reaching the Java BACK chain
//! (no accidental `Activity.finish()`).
//!
//! Flow on a 4-button board (A=PREV, B=NEXT, X=ENTER, Y=ESC):
//! - X on a focused picker enters edit mode (LV_STATE_EDITED outline).
//! - A/B step the value; focus does not move.
//! - X commits, Y cancels-out of edit mode; both just exit (values apply per
//!   step — nothing persists until the app saves).
//!
//! Orphaned key edges are safe by construction: the Java path's
//! `KeyPressFilter` drops releases whose press it never saw, and LVGL's
//! keypad acts on press edges, so a swallowed press followed by a forwarded
//! release is a no-op on both sides. Only ENTER/ESC need explicit
//! release-latches because their *presses* toggle mode while their releases
//! arrive after the state change.
//!
//! Pure logic; no `static mut`. `events.rs` owns one instance behind its own
//! synchronisation; tests construct their own.

use crate::lvgl_ffi::{LV_KEY_ENTER, LV_KEY_ESC, LV_KEY_NEXT, LV_KEY_PREV};

/// What `keypad_read_cb` should do with one key edge.
pub struct Decision {
    /// Key to report to the LVGL keypad indev, or `None` to swallow.
    pub lvgl_key: Option<u32>,
    /// Whether to forward the raw event to the Java-visible key queue.
    pub forward_java: bool,
    /// Step request for the edited picker: `(raw lv_obj ptr, +1 | -1)`.
    pub step: Option<(usize, i32)>,
}

impl Decision {
    fn pass(key: u32) -> Self {
        Decision {
            lvgl_key: Some(key),
            forward_java: true,
            step: None,
        }
    }

    fn swallow() -> Self {
        Decision {
            lvgl_key: None,
            forward_java: false,
            step: None,
        }
    }

    fn step(obj: usize, direction: i32) -> Self {
        Decision {
            lvgl_key: None,
            forward_java: false,
            step: Some((obj, direction)),
        }
    }
}

/// Edit-mode state change the caller must mirror onto the widget
/// (add/remove `LV_STATE_EDITED`). `Entered`/`Exited` only ever carry the
/// currently-focused object, so the pointer is live.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Transition {
    None,
    Entered(usize),
    Exited(usize),
}

#[derive(Default)]
pub struct EditMode {
    /// Raw `lv_obj_t*` of the picker being edited; 0 = inactive.
    active_obj: usize,
    /// Eat the ENTER release matching a mode-toggling ENTER press.
    swallow_enter_release: bool,
    /// Eat the ESC release matching an exit-on-ESC press (otherwise it
    /// reaches the Java queue as a KEYCODE_BACK and pops the Activity).
    swallow_esc_release: bool,
}

impl EditMode {
    pub const fn new() -> Self {
        Self {
            active_obj: 0,
            swallow_enter_release: false,
            swallow_esc_release: false,
        }
    }

    /// Filter one key edge. `focused` is the active group's focused widget
    /// (0 if none) and `focused_is_picker` whether it is a registered
    /// NumberPicker.
    pub fn filter(
        &mut self,
        key: u32,
        pressed: bool,
        focused: usize,
        focused_is_picker: bool,
    ) -> (Decision, Transition) {
        // Defensive abandon if focus moved under us without the DEFOCUSED
        // trampoline having fired (it normally calls `notify_gone` first and
        // clears LV_STATE_EDITED itself, so no transition is emitted here).
        if self.active_obj != 0 && self.active_obj != focused {
            self.active_obj = 0;
        }

        if self.active_obj == 0 {
            if key == LV_KEY_ENTER {
                if pressed && focused != 0 && focused_is_picker {
                    self.active_obj = focused;
                    self.swallow_enter_release = true;
                    return (Decision::swallow(), Transition::Entered(focused));
                }
                if !pressed && self.swallow_enter_release {
                    self.swallow_enter_release = false;
                    return (Decision::swallow(), Transition::None);
                }
            }
            if key == LV_KEY_ESC && !pressed && self.swallow_esc_release {
                self.swallow_esc_release = false;
                return (Decision::swallow(), Transition::None);
            }
            return (Decision::pass(key), Transition::None);
        }

        // Edit mode active; self.active_obj == focused.
        let obj = self.active_obj;
        match key {
            LV_KEY_PREV => (
                if pressed {
                    Decision::step(obj, 1)
                } else {
                    Decision::swallow()
                },
                Transition::None,
            ),
            LV_KEY_NEXT => (
                if pressed {
                    Decision::step(obj, -1)
                } else {
                    Decision::swallow()
                },
                Transition::None,
            ),
            LV_KEY_ENTER => {
                if pressed {
                    self.active_obj = 0;
                    self.swallow_enter_release = true;
                    return (Decision::swallow(), Transition::Exited(obj));
                }
                (Decision::swallow(), Transition::None)
            }
            LV_KEY_ESC => {
                if pressed {
                    self.active_obj = 0;
                    self.swallow_esc_release = true;
                    return (Decision::swallow(), Transition::Exited(obj));
                }
                (Decision::swallow(), Transition::None)
            }
            _ => (Decision::pass(key), Transition::None),
        }
    }

    /// The edited widget was defocused or deleted: abandon edit mode without
    /// touching the (possibly freed) object. The DEFOCUSED trampoline clears
    /// LV_STATE_EDITED on the live object itself.
    pub fn notify_gone(&mut self, raw_obj: usize) {
        if self.active_obj == raw_obj {
            self.active_obj = 0;
        }
    }

    #[cfg(test)]
    fn is_active(&self) -> bool {
        self.active_obj != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICKER: usize = 0x1000;
    const OTHER: usize = 0x2000;

    fn enter_edit(em: &mut EditMode) {
        let (d, t) = em.filter(LV_KEY_ENTER, true, PICKER, true);
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        assert_eq!(t, Transition::Entered(PICKER));
        // Matching release is eaten too.
        let (d, t) = em.filter(LV_KEY_ENTER, false, PICKER, true);
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        assert_eq!(t, Transition::None);
    }

    #[test]
    fn enter_on_focused_picker_toggles_edit_mode() {
        let mut em = EditMode::new();
        enter_edit(&mut em);
        assert!(em.is_active());

        let (d, t) = em.filter(LV_KEY_ENTER, true, PICKER, true);
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        assert_eq!(t, Transition::Exited(PICKER));
        let (d, _) = em.filter(LV_KEY_ENTER, false, PICKER, true);
        assert!(d.lvgl_key.is_none());
        assert!(!em.is_active());
    }

    #[test]
    fn enter_on_non_picker_passes_through() {
        let mut em = EditMode::new();
        let (d, t) = em.filter(LV_KEY_ENTER, true, OTHER, false);
        assert_eq!(d.lvgl_key, Some(LV_KEY_ENTER));
        assert!(d.forward_java);
        assert_eq!(t, Transition::None);
        let (d, _) = em.filter(LV_KEY_ENTER, false, OTHER, false);
        assert_eq!(d.lvgl_key, Some(LV_KEY_ENTER));
        assert!(d.forward_java);
    }

    #[test]
    fn prev_next_step_instead_of_navigating_while_editing() {
        let mut em = EditMode::new();
        enter_edit(&mut em);

        let (d, _) = em.filter(LV_KEY_PREV, true, PICKER, true);
        assert_eq!(d.step, Some((PICKER, 1)));
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        let (d, _) = em.filter(LV_KEY_PREV, false, PICKER, true);
        assert!(d.step.is_none() && d.lvgl_key.is_none() && !d.forward_java);

        let (d, _) = em.filter(LV_KEY_NEXT, true, PICKER, true);
        assert_eq!(d.step, Some((PICKER, -1)));
        let (d, _) = em.filter(LV_KEY_NEXT, false, PICKER, true);
        assert!(d.step.is_none() && d.lvgl_key.is_none());
    }

    #[test]
    fn prev_next_navigate_normally_outside_edit_mode() {
        let mut em = EditMode::new();
        let (d, _) = em.filter(LV_KEY_PREV, true, PICKER, true);
        assert_eq!(d.lvgl_key, Some(LV_KEY_PREV));
        assert!(d.forward_java && d.step.is_none());
    }

    #[test]
    fn esc_exits_edit_mode_and_never_leaks_back_to_java() {
        let mut em = EditMode::new();
        enter_edit(&mut em);

        let (d, t) = em.filter(LV_KEY_ESC, true, PICKER, true);
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        assert_eq!(t, Transition::Exited(PICKER));
        assert!(!em.is_active());
        // The matching release must be eaten too — a forwarded BACK release
        // is what triggers Activity.finish() in the Java dispatch chain.
        let (d, _) = em.filter(LV_KEY_ESC, false, PICKER, true);
        assert!(d.lvgl_key.is_none() && !d.forward_java);
        // Subsequent ESC presses pass through (normal back navigation).
        let (d, _) = em.filter(LV_KEY_ESC, true, PICKER, true);
        assert_eq!(d.lvgl_key, Some(LV_KEY_ESC));
        assert!(d.forward_java);
    }

    #[test]
    fn abandons_when_focus_moves_away() {
        let mut em = EditMode::new();
        enter_edit(&mut em);
        // Focus moved (e.g. requestFocus from Java): keys behave as inactive.
        let (d, t) = em.filter(LV_KEY_PREV, true, OTHER, false);
        assert_eq!(d.lvgl_key, Some(LV_KEY_PREV));
        assert!(d.forward_java);
        assert_eq!(t, Transition::None);
        assert!(!em.is_active());
    }

    #[test]
    fn notify_gone_abandons_only_the_active_picker() {
        let mut em = EditMode::new();
        enter_edit(&mut em);
        em.notify_gone(OTHER);
        assert!(em.is_active());
        em.notify_gone(PICKER);
        assert!(!em.is_active());
    }
}
