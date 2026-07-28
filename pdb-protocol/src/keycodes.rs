// SPDX-License-Identifier: GPL-3.0-only
//! Android `KeyEvent` keycodes these boards can plausibly carry, and the
//! name→number lookup every input front-end wants.
//!
//! Two front-ends speak keycode names — the host CLI (`pdb input keyevent
//! KEYCODE_DPAD_UP`) and the simulator's control channel (`input keyevent
//! DPAD_UP`) — and they must agree, because scripts move between them. The
//! numbers themselves are Android's; the device resolves them to board
//! button pins with its own table (`board_cfg::buttons::keycode_to_pin`),
//! which is board policy and stays out of here.
//!
//! Bare-integer fallback (`input keyevent 19`) is the callers': it is a CLI
//! convenience, not part of the name table.

pub const KEYCODE_HOME: i32 = 3;
pub const KEYCODE_BACK: i32 = 4;
pub const KEYCODE_DPAD_UP: i32 = 19;
pub const KEYCODE_DPAD_DOWN: i32 = 20;
pub const KEYCODE_DPAD_LEFT: i32 = 21;
pub const KEYCODE_DPAD_RIGHT: i32 = 22;
pub const KEYCODE_DPAD_CENTER: i32 = 23;
pub const KEYCODE_ENTER: i32 = 66;
pub const KEYCODE_MENU: i32 = 82;

/// Name → keycode, names without the `KEYCODE_` prefix.
pub const KEYCODES: &[(&str, i32)] = &[
    ("HOME", KEYCODE_HOME),
    ("BACK", KEYCODE_BACK),
    ("DPAD_UP", KEYCODE_DPAD_UP),
    ("DPAD_DOWN", KEYCODE_DPAD_DOWN),
    ("DPAD_LEFT", KEYCODE_DPAD_LEFT),
    ("DPAD_RIGHT", KEYCODE_DPAD_RIGHT),
    ("DPAD_CENTER", KEYCODE_DPAD_CENTER),
    ("ENTER", KEYCODE_ENTER),
    ("MENU", KEYCODE_MENU),
];

/// Resolve a keycode name — with or without the `KEYCODE_` prefix,
/// case-insensitively. Alloc-free, so the sim's `no_std`-shaped siblings can
/// call it too.
pub fn keycode_from_name(name: &str) -> Option<i32> {
    let t = name.trim();
    let t = match t.get(..8) {
        Some(p) if p.eq_ignore_ascii_case("KEYCODE_") => &t[8..],
        _ => t,
    };
    KEYCODES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(t))
        .map(|&(_, code)| code)
}

/// `dpad <dir>` convenience → keycode.
pub fn dpad_keycode(dir: &str) -> Option<i32> {
    let d = dir.trim();
    let m = |s: &str| d.eq_ignore_ascii_case(s);
    if m("up") {
        Some(KEYCODE_DPAD_UP)
    } else if m("down") {
        Some(KEYCODE_DPAD_DOWN)
    } else if m("left") {
        Some(KEYCODE_DPAD_LEFT)
    } else if m("right") {
        Some(KEYCODE_DPAD_RIGHT)
    } else if m("center") || m("enter") || m("ok") {
        Some(KEYCODE_DPAD_CENTER)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycode_name_variants_resolve() {
        assert_eq!(keycode_from_name("KEYCODE_DPAD_UP"), Some(19));
        assert_eq!(keycode_from_name("dpad_up"), Some(19)); // no prefix, lowercase
        assert_eq!(keycode_from_name("keycode_back"), Some(4)); // prefix, lowercase
        assert_eq!(keycode_from_name(" MENU "), Some(82)); // trimmed
        assert_eq!(keycode_from_name("nope"), None);
        assert_eq!(keycode_from_name("23"), None); // integers are the callers'
    }

    #[test]
    fn dpad_directions_map() {
        assert_eq!(dpad_keycode("up"), Some(19));
        assert_eq!(dpad_keycode("CENTER"), Some(23));
        assert_eq!(dpad_keycode("ok"), Some(23));
        assert_eq!(dpad_keycode("sideways"), None);
    }

    /// A multi-byte character across the prefix boundary must not panic the
    /// byte-sliced prefix strip.
    #[test]
    fn non_ascii_names_are_refused_not_panicked() {
        assert_eq!(keycode_from_name("KEYCÖDE_BACK"), None);
        assert_eq!(keycode_from_name("ÄÖÜÄÖÜÄÖ"), None);
    }
}
