// SPDX-License-Identifier: GPL-3.0-only
//! `pdb input …` — inject synthetic input into a real device, the picodroid
//! analog of `adb shell input tap|swipe|keyevent`. The host sends a compact
//! verb over PDB (`CMD_INPUT`); the device turns it into HAL-level input so the
//! whole on-device pipeline runs unchanged. Keycode→pin resolution happens on
//! the device (board-specific), so the host stays board-agnostic — exactly like
//! Android, where `input keyevent` sends a keycode the device resolves.

use std::process;
use std::time::Duration;

use crate::protocol::{recv_response, send_frame, status_str, CMD_INPUT, STATUS_OK};
use pdb_protocol::input::{InputEvent, MAX_INPUT_PAYLOAD};
use pdb_protocol::keycodes::{self, dpad_keycode};
use pdb_protocol::KEY_META_DOWN_UP;

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

/// Resolve a keyevent argument to an Android keycode: a known name (with or
/// without `KEYCODE_`, case-insensitive — the shared table the sim's control
/// channel also uses) or a bare integer (forwarded verbatim, like Android's
/// `input keyevent 19`).
fn keycode_from_arg(arg: &str) -> Option<i32> {
    keycodes::keycode_from_name(arg).or_else(|| arg.trim().parse::<i32>().ok())
}

/// Wire bytes of one event, via the shared encoder the device's decoder is
/// tested against.
fn payload_of(ev: InputEvent) -> Vec<u8> {
    let mut buf = [0u8; MAX_INPUT_PAYLOAD];
    let n = ev.encode(&mut buf);
    buf[..n].to_vec()
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
            payload_of(InputEvent::Key {
                keycode: code,
                meta: KEY_META_DOWN_UP,
            })
        }
        "dpad" => {
            let Some(code) = rest.first().and_then(|a| dpad_keycode(a)) else {
                eprintln!("error: input dpad needs <up|down|left|right|center>");
                eprint!("{INPUT_USAGE}");
                process::exit(1);
            };
            payload_of(InputEvent::Key {
                keycode: code,
                meta: KEY_META_DOWN_UP,
            })
        }
        "back" => payload_of(InputEvent::Key {
            keycode: keycodes::KEYCODE_BACK,
            meta: KEY_META_DOWN_UP,
        }),
        "tap" => {
            let x = parse_int("x", rest.first());
            let y = parse_int("y", rest.get(1));
            payload_of(InputEvent::Tap { x, y })
        }
        "swipe" => {
            let x1 = parse_int("x1", rest.first());
            let y1 = parse_int("y1", rest.get(1));
            let x2 = parse_int("x2", rest.get(2));
            let y2 = parse_int("y2", rest.get(3));
            let duration_ms = rest
                .get(4)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(DEFAULT_SWIPE_MS)
                .min(MAX_SWIPE_MS);
            payload_of(InputEvent::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration_ms,
            })
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

    // Name and dpad resolution are pinned in `pdb_protocol::keycodes`; the
    // bare-integer fallback is this CLI's own convenience.
    #[test]
    fn keycode_arg_accepts_names_and_bare_integers() {
        assert_eq!(keycode_from_arg("KEYCODE_DPAD_UP"), Some(19));
        assert_eq!(keycode_from_arg("23"), Some(23)); // bare integer
        assert_eq!(keycode_from_arg("nope"), None);
    }

    // Payload byte layouts are pinned in `pdb_protocol::input`, where the
    // encoder now lives; what is left here is the CLI's verb → event mapping.

    #[test]
    fn build_payload_dispatches_by_verb() {
        use pdb_protocol::{INPUT_KEY, INPUT_SWIPE, INPUT_TAP};
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
