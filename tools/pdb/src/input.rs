// SPDX-License-Identifier: GPL-3.0-only
//! `pdb input …` — inject synthetic input into a real device, the picodroid
//! analog of `adb shell input tap|swipe|keyevent`. The host sends a compact
//! verb over PDB (`CMD_INPUT`); the device turns it into HAL-level input so the
//! whole on-device pipeline runs unchanged. Keycode→pin resolution happens on
//! the device (board-specific), so the host stays board-agnostic — exactly like
//! Android, where `input keyevent` sends a keycode the device resolves.

use std::process;
use std::time::Duration;

use crate::protocol::{
    recv_response, send_frame, status_str, CMD_INPUT, INPUT_KEY, INPUT_SWIPE, INPUT_TAP,
    KEY_META_DOWN_UP, STATUS_OK,
};

const BAUD_RATE: u32 = 115_200;
/// Generous — a swipe blocks the device handler until the gesture completes.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Clamp host-requested swipe duration so we never wait past `TIMEOUT`.
const MAX_SWIPE_MS: u32 = 5_000;
const DEFAULT_SWIPE_MS: u32 = 300;

const INPUT_USAGE: &str = "\
Usage: pdb input <command> [args]

  keyevent <KEYCODE|number>        Press+release a key (e.g. KEYCODE_DPAD_UP, 19)
  dpad <up|down|left|right|center> Convenience wrapper for the D-pad keyevents
  back                             Convenience wrapper for KEYCODE_BACK
  tap <x> <y>                      Tap the touchscreen at (x, y)
  swipe <x1> <y1> <x2> <y2> [ms]   Swipe from (x1,y1) to (x2,y2) over [ms] (default 300)
";

/// Android `KeyEvent` keycode names these boards can plausibly carry. Accepted
/// with or without the `KEYCODE_` prefix, case-insensitively; a bare integer is
/// also accepted (forwarded verbatim, like Android's `input keyevent 19`).
const KEYCODES: &[(&str, i32)] = &[
    ("HOME", 3),
    ("BACK", 4),
    ("DPAD_UP", 19),
    ("DPAD_DOWN", 20),
    ("DPAD_LEFT", 21),
    ("DPAD_RIGHT", 22),
    ("DPAD_CENTER", 23),
    ("ENTER", 66),
    ("MENU", 82),
];

/// Resolve a keyevent argument to an Android keycode: a known name (with or
/// without `KEYCODE_`) or a bare integer.
fn keycode_from_arg(arg: &str) -> Option<i32> {
    let up = arg.trim().to_ascii_uppercase();
    let name = up.strip_prefix("KEYCODE_").unwrap_or(&up);
    if let Some(&(_, code)) = KEYCODES.iter().find(|&&(n, _)| n == name) {
        return Some(code);
    }
    arg.trim().parse::<i32>().ok()
}

fn dpad_keycode(dir: &str) -> Option<i32> {
    match dir.trim().to_ascii_lowercase().as_str() {
        "up" => Some(19),
        "down" => Some(20),
        "left" => Some(21),
        "right" => Some(22),
        "center" | "enter" | "ok" => Some(23),
        _ => None,
    }
}

// ── Payload encoders ─────────────────────────────────────────────────────────

fn encode_key(keycode: i32, meta: u8) -> Vec<u8> {
    let mut p = Vec::with_capacity(6);
    p.push(INPUT_KEY);
    p.extend_from_slice(&keycode.to_le_bytes());
    p.push(meta);
    p
}

fn encode_tap(x: i32, y: i32) -> Vec<u8> {
    let mut p = Vec::with_capacity(9);
    p.push(INPUT_TAP);
    p.extend_from_slice(&x.to_le_bytes());
    p.extend_from_slice(&y.to_le_bytes());
    p
}

fn encode_swipe(x1: i32, y1: i32, x2: i32, y2: i32, dur_ms: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(21);
    p.push(INPUT_SWIPE);
    for v in [x1, y1, x2, y2] {
        p.extend_from_slice(&v.to_le_bytes());
    }
    p.extend_from_slice(&dur_ms.to_le_bytes());
    p
}

// ── CLI ──────────────────────────────────────────────────────────────────────

/// Parse the `input` subcommand + args into a wire payload, or exit with usage.
fn build_payload(args: &[String]) -> Vec<u8> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[args.len().min(1)..];

    let parse_int = |label: &str, s: Option<&String>| -> i32 {
        match s.and_then(|v| v.parse::<i32>().ok()) {
            Some(v) => v,
            None => {
                eprintln!("error: input {sub}: expected integer for {label}");
                eprint!("{INPUT_USAGE}");
                process::exit(1);
            }
        }
    };

    match sub {
        "keyevent" => {
            let Some(code) = rest.first().and_then(|a| keycode_from_arg(a)) else {
                eprintln!("error: input keyevent needs a <KEYCODE|number>");
                eprint!("{INPUT_USAGE}");
                process::exit(1);
            };
            encode_key(code, KEY_META_DOWN_UP)
        }
        "dpad" => {
            let Some(code) = rest.first().and_then(|a| dpad_keycode(a)) else {
                eprintln!("error: input dpad needs <up|down|left|right|center>");
                eprint!("{INPUT_USAGE}");
                process::exit(1);
            };
            encode_key(code, KEY_META_DOWN_UP)
        }
        "back" => encode_key(4, KEY_META_DOWN_UP),
        "tap" => {
            let x = parse_int("x", rest.first());
            let y = parse_int("y", rest.get(1));
            encode_tap(x, y)
        }
        "swipe" => {
            let x1 = parse_int("x1", rest.first());
            let y1 = parse_int("y1", rest.get(1));
            let x2 = parse_int("x2", rest.get(2));
            let y2 = parse_int("y2", rest.get(3));
            let dur = rest
                .get(4)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(DEFAULT_SWIPE_MS)
                .min(MAX_SWIPE_MS);
            encode_swipe(x1, y1, x2, y2, dur)
        }
        "" => {
            eprint!("{INPUT_USAGE}");
            process::exit(1);
        }
        other => {
            eprintln!("error: unknown input command '{other}'");
            eprint!("{INPUT_USAGE}");
            process::exit(1);
        }
    }
}

pub fn run(port_name: &str, args: &[String]) {
    let payload = build_payload(args);

    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = send_frame(port.as_mut(), CMD_INPUT, &payload) {
        eprintln!("error: INPUT send failed: {e}");
        process::exit(1);
    }

    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, _)) => {}
        Ok((status, msg)) => {
            let detail = String::from_utf8_lossy(&msg);
            if detail.is_empty() {
                eprintln!("error: INPUT returned {}", status_str(status));
            } else {
                eprintln!("error: INPUT returned {} ({detail})", status_str(status));
            }
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: INPUT recv failed: {e}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycode_name_variants_resolve() {
        assert_eq!(keycode_from_arg("KEYCODE_DPAD_UP"), Some(19));
        assert_eq!(keycode_from_arg("dpad_up"), Some(19)); // no prefix, lowercase
        assert_eq!(keycode_from_arg("BACK"), Some(4));
        assert_eq!(keycode_from_arg("23"), Some(23)); // bare integer
        assert_eq!(keycode_from_arg("nope"), None);
    }

    #[test]
    fn dpad_directions_map() {
        assert_eq!(dpad_keycode("up"), Some(19));
        assert_eq!(dpad_keycode("CENTER"), Some(23));
        assert_eq!(dpad_keycode("sideways"), None);
    }

    #[test]
    fn key_payload_layout() {
        // [INPUT_KEY][keycode i32 LE][meta]
        let p = encode_key(19, KEY_META_DOWN_UP);
        assert_eq!(p, vec![INPUT_KEY, 19, 0, 0, 0, KEY_META_DOWN_UP]);
    }

    #[test]
    fn tap_payload_layout() {
        let p = encode_tap(100, 200);
        assert_eq!(p[0], INPUT_TAP);
        assert_eq!(&p[1..5], &100i32.to_le_bytes());
        assert_eq!(&p[5..9], &200i32.to_le_bytes());
    }

    #[test]
    fn swipe_payload_layout_and_len() {
        let p = encode_swipe(1, 2, 3, 4, 300);
        assert_eq!(p.len(), 21);
        assert_eq!(p[0], INPUT_SWIPE);
        assert_eq!(&p[17..21], &300u32.to_le_bytes());
    }

    #[test]
    fn build_payload_dispatches_by_verb() {
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(build_payload(&s(&["back"]))[0], INPUT_KEY);
        assert_eq!(build_payload(&s(&["tap", "10", "20"]))[0], INPUT_TAP);
        assert_eq!(
            build_payload(&s(&["swipe", "1", "2", "3", "4"]))[0],
            INPUT_SWIPE
        );
        // default swipe duration applied when omitted
        let p = build_payload(&s(&["swipe", "1", "2", "3", "4"]));
        assert_eq!(&p[17..21], &DEFAULT_SWIPE_MS.to_le_bytes());
    }
}
