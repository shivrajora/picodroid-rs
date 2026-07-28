// SPDX-License-Identifier: GPL-3.0-only
//! The `CMD_INPUT` payload — a synthetic input event.
//!
//! The one payload that travels host→device: the host encodes with
//! [`InputEvent::encode`], the device decodes with [`InputEvent::decode`].
//! What an event *does* — keycode→pin resolution, coordinate clamping,
//! whether the board has a touch panel at all — is the device's business and
//! stays in `picodroid-core`; this module owns only the bytes after the
//! subtype constants declared in the crate root.

use crate::{INPUT_KEY, INPUT_SWIPE, INPUT_TAP, KEY_META_DOWN_UP};

/// Largest `CMD_INPUT` payload: SWIPE = 1 subtype + 4×i32 + u32 = 21 bytes,
/// rounded up. Size drain buffers with this.
pub const MAX_INPUT_PAYLOAD: usize = 24;

/// One synthetic input event, exactly as it crosses the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    /// A key, by Android keycode; `meta` per the crate's `KEY_META_*`.
    Key {
        keycode: i32,
        meta: u8,
    },
    Tap {
        x: i32,
        y: i32,
    },
    Swipe {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Empty,
    /// Payload too short for its subtype; carries the subtype byte so the
    /// device can name the verb in its error message.
    Short(u8),
    UnknownSubtype(u8),
}

fn i32_at(b: &[u8], off: usize) -> Option<i32> {
    b.get(off..off + 4)
        .map(|s| i32::from_le_bytes(s.try_into().unwrap()))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

impl InputEvent {
    /// Encode into `out`, returning the number of bytes written.
    pub fn encode(&self, out: &mut [u8; MAX_INPUT_PAYLOAD]) -> usize {
        match *self {
            InputEvent::Key { keycode, meta } => {
                out[0] = INPUT_KEY;
                out[1..5].copy_from_slice(&keycode.to_le_bytes());
                out[5] = meta;
                6
            }
            InputEvent::Tap { x, y } => {
                out[0] = INPUT_TAP;
                out[1..5].copy_from_slice(&x.to_le_bytes());
                out[5..9].copy_from_slice(&y.to_le_bytes());
                9
            }
            InputEvent::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration_ms,
            } => {
                out[0] = INPUT_SWIPE;
                out[1..5].copy_from_slice(&x1.to_le_bytes());
                out[5..9].copy_from_slice(&y1.to_le_bytes());
                out[9..13].copy_from_slice(&x2.to_le_bytes());
                out[13..17].copy_from_slice(&y2.to_le_bytes());
                out[17..21].copy_from_slice(&duration_ms.to_le_bytes());
                21
            }
        }
    }

    /// Decode a payload.
    ///
    /// One deliberate leniency, kept from the original device parser: a KEY
    /// payload may omit its meta byte, which reads as [`KEY_META_DOWN_UP`] —
    /// so a minimal five-byte keyevent from an older or hand-rolled host
    /// still presses and releases.
    pub fn decode(payload: &[u8]) -> Result<Self, InputError> {
        let (&subtype, args) = payload.split_first().ok_or(InputError::Empty)?;
        match subtype {
            INPUT_KEY => {
                let keycode = i32_at(args, 0).ok_or(InputError::Short(INPUT_KEY))?;
                let meta = args.get(4).copied().unwrap_or(KEY_META_DOWN_UP);
                Ok(InputEvent::Key { keycode, meta })
            }
            INPUT_TAP => match (i32_at(args, 0), i32_at(args, 4)) {
                (Some(x), Some(y)) => Ok(InputEvent::Tap { x, y }),
                _ => Err(InputError::Short(INPUT_TAP)),
            },
            INPUT_SWIPE => match (
                i32_at(args, 0),
                i32_at(args, 4),
                i32_at(args, 8),
                i32_at(args, 12),
                u32_at(args, 16),
            ) {
                (Some(x1), Some(y1), Some(x2), Some(y2), Some(duration_ms)) => {
                    Ok(InputEvent::Swipe {
                        x1,
                        y1,
                        x2,
                        y2,
                        duration_ms,
                    })
                }
                _ => Err(InputError::Short(INPUT_SWIPE)),
            },
            other => Err(InputError::UnknownSubtype(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KEY_META_DOWN;

    #[test]
    fn key_payload_layout() {
        // [INPUT_KEY][keycode i32 LE][meta]
        let mut p = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Key {
            keycode: 19,
            meta: KEY_META_DOWN_UP,
        }
        .encode(&mut p);
        assert_eq!(&p[..n], &[INPUT_KEY, 19, 0, 0, 0, KEY_META_DOWN_UP]);
    }

    #[test]
    fn tap_payload_layout() {
        let mut p = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Tap { x: 100, y: 200 }.encode(&mut p);
        assert_eq!(n, 9);
        assert_eq!(p[0], INPUT_TAP);
        assert_eq!(&p[1..5], &100i32.to_le_bytes());
        assert_eq!(&p[5..9], &200i32.to_le_bytes());
    }

    #[test]
    fn swipe_payload_layout_and_len() {
        let mut p = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Swipe {
            x1: 1,
            y1: 2,
            x2: 3,
            y2: 4,
            duration_ms: 300,
        }
        .encode(&mut p);
        assert_eq!(n, 21);
        assert_eq!(p[0], INPUT_SWIPE);
        assert_eq!(&p[1..5], &1i32.to_le_bytes());
        assert_eq!(&p[13..17], &4i32.to_le_bytes());
        assert_eq!(&p[17..21], &300u32.to_le_bytes());
    }

    #[test]
    fn encode_decode_round_trips() {
        let events = [
            InputEvent::Key {
                keycode: 82,
                meta: KEY_META_DOWN,
            },
            InputEvent::Tap { x: -5, y: 319 },
            InputEvent::Swipe {
                x1: 10,
                y1: 20,
                x2: 30,
                y2: 40,
                duration_ms: 5_000,
            },
        ];
        for ev in events {
            let mut p = [0u8; MAX_INPUT_PAYLOAD];
            let n = ev.encode(&mut p);
            assert_eq!(InputEvent::decode(&p[..n]), Ok(ev));
        }
    }

    /// The kept leniency: a KEY payload without its meta byte reads as
    /// press-and-release rather than an error.
    #[test]
    fn a_five_byte_keyevent_defaults_to_down_up() {
        let mut p = [0u8; MAX_INPUT_PAYLOAD];
        let n = InputEvent::Key {
            keycode: 4,
            meta: KEY_META_DOWN,
        }
        .encode(&mut p);
        assert_eq!(
            InputEvent::decode(&p[..n - 1]),
            Ok(InputEvent::Key {
                keycode: 4,
                meta: KEY_META_DOWN_UP
            })
        );
    }

    #[test]
    fn decode_errors_name_the_problem() {
        assert_eq!(InputEvent::decode(&[]), Err(InputError::Empty));
        assert_eq!(
            InputEvent::decode(&[INPUT_KEY, 1, 2]),
            Err(InputError::Short(INPUT_KEY))
        );
        assert_eq!(
            InputEvent::decode(&[INPUT_TAP, 0, 0, 0, 0, 9, 9]),
            Err(InputError::Short(INPUT_TAP))
        );
        let mut swipe = [0u8; 20]; // one byte short of a full swipe
        swipe[0] = INPUT_SWIPE;
        assert_eq!(
            InputEvent::decode(&swipe),
            Err(InputError::Short(INPUT_SWIPE))
        );
        assert_eq!(
            InputEvent::decode(&[0x7F, 0, 0]),
            Err(InputError::UnknownSubtype(0x7F))
        );
    }
}
