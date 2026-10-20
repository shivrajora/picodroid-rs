// SPDX-License-Identifier: GPL-3.0-only
//! The time service's two formats (docs/designs/time-service-2026-10.md):
//! the `/system/time` store record and the zone ids
//! `AlarmManager.setTimeZone` takes.
//!
//! The store is one 8-byte record, written to a temporary file and renamed
//! over the old one (as `power.rs` and `hal::wifi` do): `[i32 LE offset
//! minutes][u8 auto time][3 reserved]`. Missing or short: UTC, automatic.
//! The zone is a fixed offset, never a region: there is no tz database on
//! the device (picoclock-roadmap R4), so daylight saving is a setting the
//! user moves. The service itself — the atomics, the store on the volume,
//! the sync task — is `picodroid_core::time_service`.

/// The record's size.
pub const RECORD_LEN: usize = 8;

/// The widest zone offsets in use: UTC-12:00 (Baker Island) to UTC+14:00
/// (Line Islands). `ZoneOffset` allows ±18 h; the setter keeps to what a
/// user could mean.
pub const MIN_OFFSET_MINUTES: i32 = -12 * 60;
pub const MAX_OFFSET_MINUTES: i32 = 14 * 60;

/// A decoded store record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub offset_minutes: i32,
    pub auto_time: bool,
}

impl Record {
    pub const DEFAULT: Record = Record {
        offset_minutes: 0,
        auto_time: true,
    };

    pub fn encode(&self) -> [u8; RECORD_LEN] {
        let mut out = [0u8; RECORD_LEN];
        out[..4].copy_from_slice(&self.offset_minutes.to_le_bytes());
        out[4] = u8::from(self.auto_time);
        out
    }

    /// `None` for a short record or an offset outside the allowed range —
    /// both fall back to the default rather than to a bad zone.
    pub fn decode(bytes: &[u8]) -> Option<Record> {
        if bytes.len() < RECORD_LEN {
            return None;
        }
        let offset_minutes = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if !valid_offset(offset_minutes) {
            return None;
        }
        Some(Record {
            offset_minutes,
            auto_time: bytes[4] != 0,
        })
    }
}

/// Whether `minutes` is an offset the setter accepts.
pub fn valid_offset(minutes: i32) -> bool {
    (MIN_OFFSET_MINUTES..=MAX_OFFSET_MINUTES).contains(&minutes)
}

/// A zone id as `AlarmManager.setTimeZone` takes it, as minutes east of
/// UTC: `UTC`, `GMT`, `UT`, `Z`, or any of those followed by an offset, or
/// a bare offset `+h`, `+hh`, `+hh:mm`, `+hhmm`. The forms `ZoneId.of`
/// accepts; a region id is `None`, since there is no tz database to turn
/// it into an offset. Range-checked.
pub fn parse_offset_minutes(id: &str) -> Option<i32> {
    let id = id.trim();
    if id.is_empty() {
        return None;
    }
    let rest = if id == "Z" {
        return Some(0);
    } else if let Some(r) = id.strip_prefix("UTC") {
        r
    } else if let Some(r) = id.strip_prefix("GMT") {
        r
    } else if let Some(r) = id.strip_prefix("UT") {
        r
    } else {
        id
    };
    if rest.is_empty() {
        return Some(0);
    }
    let (sign, digits) = match rest.as_bytes()[0] {
        b'+' => (1, &rest[1..]),
        b'-' => (-1, &rest[1..]),
        _ => return None,
    };
    // str::parse accepts a sign of its own, which would let "GMT+-1" and
    // "++5" through; the groups after the sign are plain digits (ZoneId.of
    // rejects anything else).
    if !digits.bytes().all(|b| b.is_ascii_digit() || b == b':') {
        return None;
    }
    let (h, m) = match digits.len() {
        1 | 2 => (digits.parse::<i32>().ok()?, 0),
        4 => (
            digits[..2].parse::<i32>().ok()?,
            digits[2..].parse::<i32>().ok()?,
        ),
        5 if digits.as_bytes()[2] == b':' => (
            digits[..2].parse::<i32>().ok()?,
            digits[3..].parse::<i32>().ok()?,
        ),
        _ => return None,
    };
    if m >= 60 {
        return None;
    }
    let minutes = sign * (h * 60 + m);
    valid_offset(minutes).then_some(minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trips() {
        let r = Record {
            offset_minutes: 330,
            auto_time: false,
        };
        assert_eq!(Record::decode(&r.encode()), Some(r));
        let neg = Record {
            offset_minutes: -570,
            auto_time: true,
        };
        assert_eq!(Record::decode(&neg.encode()), Some(neg));
    }

    #[test]
    fn short_or_out_of_range_records_are_refused() {
        assert_eq!(Record::decode(&[1, 2, 3]), None);
        let wide = Record {
            offset_minutes: 15 * 60,
            auto_time: true,
        };
        assert_eq!(Record::decode(&wide.encode()), None);
        let wide = Record {
            offset_minutes: -13 * 60,
            auto_time: true,
        };
        assert_eq!(Record::decode(&wide.encode()), None);
    }

    #[test]
    fn zone_ids_parse_as_fixed_offsets() {
        assert_eq!(parse_offset_minutes("UTC"), Some(0));
        assert_eq!(parse_offset_minutes("GMT"), Some(0));
        assert_eq!(parse_offset_minutes("Z"), Some(0));
        assert_eq!(parse_offset_minutes("GMT+05:30"), Some(330));
        assert_eq!(parse_offset_minutes("UTC-8"), Some(-480));
        assert_eq!(parse_offset_minutes("+01:00"), Some(60));
        assert_eq!(parse_offset_minutes("-0330"), Some(-210));
        assert_eq!(parse_offset_minutes("UT+14:00"), Some(840));
        assert_eq!(parse_offset_minutes("Europe/London"), None);
        assert_eq!(parse_offset_minutes("GMT+15"), None);
        assert_eq!(parse_offset_minutes("+01:60"), None);
        assert_eq!(parse_offset_minutes("+1:00"), None);
        // A second sign inside the digit groups is not an offset.
        assert_eq!(parse_offset_minutes("GMT+-1"), None);
        assert_eq!(parse_offset_minutes("++5"), None);
        assert_eq!(parse_offset_minutes("+-130"), None);
        assert_eq!(parse_offset_minutes("+-1:30"), None);
        assert_eq!(parse_offset_minutes(""), None);
    }
}
