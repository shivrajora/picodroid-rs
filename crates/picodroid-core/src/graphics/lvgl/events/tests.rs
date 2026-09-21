// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[cfg(has_buttons)]
#[test]
fn pin_to_keycode_roundtrips_declared_pins() {
    for &(pin, _, keycode) in BUTTONS {
        assert_eq!(pin_to_keycode(pin), Some(keycode));
    }
}

#[test]
fn pin_to_keycode_returns_none_for_unmapped() {
    assert_eq!(pin_to_keycode(99), None);
}

#[cfg(has_buttons)]
#[test]
fn keycode_to_pin_roundtrips_declared_keycodes() {
    // keycode → pin must resolve back to a button that carries that keycode
    // (first-declared wins if a keycode is shared, matching the impl).
    for &(_, _, keycode) in BUTTONS {
        let pin = keycode_to_pin(keycode).expect("declared keycode resolves");
        assert_eq!(pin_to_keycode(pin), Some(keycode));
    }
}

#[test]
fn keycode_to_pin_returns_none_for_unmapped() {
    // -1 is never a valid Android keycode, so no board declares it.
    assert_eq!(keycode_to_pin(-1), None);
}

/// Handle `0` maps to a null `lv_obj` on both the 32-bit and 64-bit handle
/// tables, so the focus helpers must short-circuit on it without touching
/// LVGL group state. Guards the null-handle path that protects sim builds
/// and any pre-launch caller.
#[test]
fn focus_helpers_short_circuit_on_null_handle() {
    set_view_focusable(0, true);
    set_view_focusable(0, false);
    assert!(
        !request_view_focus(0),
        "requestFocus on a null handle must report no focus"
    );
}

#[cfg(has_buttons)]
#[test]
fn key_event_queue_roundtrips_in_fifo_order() {
    reset_key_event_queue();
    push_key_event_raw(12, false);
    push_key_event_raw(13, true);
    push_key_event_raw(14, false);

    let a = drain_key_event().unwrap();
    assert_eq!(a.pin, 12);
    assert!(!a.rising);
    let b = drain_key_event().unwrap();
    assert_eq!(b.pin, 13);
    assert!(b.rising);
    let c = drain_key_event().unwrap();
    assert_eq!(c.pin, 14);
    assert!(!c.rising);
    assert!(drain_key_event().is_none());
}

#[cfg(has_buttons)]
#[test]
fn key_event_queue_wraps_around() {
    reset_key_event_queue();
    for cycle in 0..4 {
        for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
            push_key_event_raw(i as u8, cycle % 2 == 0);
        }
        for i in 0..KEY_EVENT_QUEUE_SIZE - 1 {
            let e = drain_key_event().unwrap();
            assert_eq!(e.pin, i as u8);
        }
        assert!(drain_key_event().is_none());
    }
}
