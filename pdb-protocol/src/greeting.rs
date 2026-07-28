// SPDX-License-Identifier: GPL-3.0-only
//! The `CMD_PING` greeting — the first payload every host reads.
//!
//! Additive by design, with the version string as the sentinel: a
//! `picodroid/2.0` host read only the first [`LEGACY_GREETING_LEN`] bytes, so
//! appending the framework-map-version left it working. Newer hosts read the
//! tail, and detect `2.0` to refuse an install that would need an SWD
//! reflash. Keep additions at the end for the same reason.
//!
//! ```text
//! [14] version string, NUL-padded            ("picodroid/2.1\0")
//! [4]  max PAPK size in bytes, u32 LE
//! [1]  framework-map-version length
//! [N]  framework-map-version bytes
//! ```
//!
//! The device encodes with [`encode`]; the host decodes with
//! [`Greeting::parse`]. What a version *means* — which ones are refused, and
//! with what message — is host policy and stays in `tools/pdb`; this module
//! owns only the bytes.

/// What current firmware answers to `CMD_PING`. Bumping this is a protocol
/// change, not an edit: the golden test pins the greeting prefix per version.
pub const PROTOCOL_VERSION: &str = "picodroid/2.1";

/// Firmware that predates the framework-map-version field. Hosts detect it by
/// exact match — see [`Greeting::is_legacy`].
pub const LEGACY_VERSION: &str = "picodroid/2.0";

/// Every greeting version starts with this; one that doesn't is not a
/// picodroid device.
pub const VERSION_PREFIX: &str = "picodroid/";

/// Width of the NUL-padded version field.
pub const VERSION_FIELD_LEN: usize = 14;

/// The bytes a `picodroid/2.0` host reads: version field + max-PAPK word.
/// Frozen — additions go after the length-prefixed tail, never here.
pub const LEGACY_GREETING_LEN: usize = VERSION_FIELD_LEN + 4;

/// Cap on the framework-map-version tail; [`encode`] truncates beyond it.
pub const FMV_MAX: usize = 64;

/// Largest greeting [`encode`] can produce — size wire buffers with this.
pub const GREETING_MAX: usize = LEGACY_GREETING_LEN + 1 + FMV_MAX;

/// Encode the greeting for this firmware into `out`, returning the number of
/// bytes written. `framework_map_version` beyond [`FMV_MAX`] bytes is
/// truncated rather than refused — a long version string is a build oddity,
/// not a reason to stop answering pings.
pub fn encode(max_papk: u32, framework_map_version: &[u8], out: &mut [u8; GREETING_MAX]) -> usize {
    let version = PROTOCOL_VERSION.as_bytes();
    out[..version.len()].copy_from_slice(version);
    out[version.len()..VERSION_FIELD_LEN].fill(0);
    out[VERSION_FIELD_LEN..LEGACY_GREETING_LEN].copy_from_slice(&max_papk.to_le_bytes());
    let fmv_len = framework_map_version.len().min(FMV_MAX);
    out[LEGACY_GREETING_LEN] = fmv_len as u8;
    out[LEGACY_GREETING_LEN + 1..LEGACY_GREETING_LEN + 1 + fmv_len]
        .copy_from_slice(&framework_map_version[..fmv_len]);
    LEGACY_GREETING_LEN + 1 + fmv_len
}

/// A parsed greeting, borrowing from the payload it was parsed from.
#[derive(Debug, PartialEq, Eq)]
pub struct Greeting<'a> {
    /// The full version string, NUL padding stripped.
    pub version: &'a str,
    /// Largest PAPK the device's flash slot accepts, in bytes.
    pub max_papk: u32,
    /// Empty on a legacy (`picodroid/2.0`) greeting, which predates the field.
    pub framework_map_version: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GreetingError {
    /// Shorter than even a legacy greeting; carries the actual length.
    TooShort(usize),
    /// A non-legacy greeting without the framework-map-version field.
    MissingFmv,
    /// The length byte promises more framework-map-version bytes than follow.
    TruncatedFmv,
    FmvNotUtf8,
}

impl<'a> Greeting<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<Self, GreetingError> {
        if payload.len() < LEGACY_GREETING_LEN {
            return Err(GreetingError::TooShort(payload.len()));
        }
        // A version field that isn't UTF-8 parses as "?" rather than failing:
        // the host's "unrecognized firmware greeting" policy wants to show
        // *something*, and a garbage version is its problem to refuse.
        let version = core::str::from_utf8(&payload[..VERSION_FIELD_LEN])
            .unwrap_or("?")
            .trim_end_matches('\0');
        let max_papk = u32::from_le_bytes(
            payload[VERSION_FIELD_LEN..LEGACY_GREETING_LEN]
                .try_into()
                .unwrap(),
        );

        if version == LEGACY_VERSION {
            // A 2.0 greeting ends here; every field below postdates it.
            return Ok(Greeting {
                version,
                max_papk,
                framework_map_version: "",
            });
        }

        if payload.len() < LEGACY_GREETING_LEN + 1 {
            return Err(GreetingError::MissingFmv);
        }
        let fmv_len = payload[LEGACY_GREETING_LEN] as usize;
        let fmv_end = LEGACY_GREETING_LEN + 1 + fmv_len;
        if payload.len() < fmv_end {
            return Err(GreetingError::TruncatedFmv);
        }
        let framework_map_version =
            core::str::from_utf8(&payload[LEGACY_GREETING_LEN + 1..fmv_end])
                .map_err(|_| GreetingError::FmvNotUtf8)?;

        Ok(Greeting {
            version,
            max_papk,
            framework_map_version,
        })
    }

    /// Firmware that predates the framework-map-version field. What to do
    /// about one is host policy.
    pub fn is_legacy(&self) -> bool {
        self.version == LEGACY_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden bytes for the current version. Every field gets a distinct
    /// value: a swapped pair of same-valued fields is invisible to a golden
    /// test (a lesson the sysmon layout taught three times).
    #[test]
    fn greeting_golden_bytes() {
        let mut buf = [0u8; GREETING_MAX];
        let n = encode(0xA1B2_C3D4, b"7.3.1-g", &mut buf);

        assert_eq!(&buf[..14], b"picodroid/2.1\0");
        assert_eq!(&buf[14..18], &[0xD4, 0xC3, 0xB2, 0xA1]);
        assert_eq!(buf[18], 7);
        assert_eq!(&buf[19..26], b"7.3.1-g");
        assert_eq!(n, 26);
    }

    /// The first [`LEGACY_GREETING_LEN`] bytes are the frozen contract with
    /// `picodroid/2.0` hosts, which read exactly that many and no more.
    #[test]
    fn legacy_prefix_is_frozen_at_18_bytes() {
        assert_eq!(LEGACY_GREETING_LEN, 18);
        let mut buf = [0u8; GREETING_MAX];
        encode(0x0010_0000, b"", &mut buf);
        assert_eq!(&buf[..VERSION_FIELD_LEN], b"picodroid/2.1\0");
        assert_eq!(&buf[VERSION_FIELD_LEN..18], &0x0010_0000u32.to_le_bytes());
    }

    #[test]
    fn encode_parse_round_trip() {
        let mut buf = [0u8; GREETING_MAX];
        let n = encode(1020 * 1024, b"5.2.8", &mut buf);

        let g = Greeting::parse(&buf[..n]).unwrap();
        assert_eq!(g.version, PROTOCOL_VERSION);
        assert_eq!(g.max_papk, 1020 * 1024);
        assert_eq!(g.framework_map_version, "5.2.8");
        assert!(!g.is_legacy());
    }

    #[test]
    fn an_oversized_fmv_is_truncated_not_overflowed() {
        let fmv = [b'x'; 200];
        let mut buf = [0u8; GREETING_MAX];
        let n = encode(1, &fmv, &mut buf);
        assert_eq!(n, GREETING_MAX);
        assert_eq!(buf[LEGACY_GREETING_LEN] as usize, FMV_MAX);
    }

    /// A real 2.0 firmware sends 18 bytes and no framework-map-version; that
    /// must parse (as legacy), not error, or the host loses the ability to
    /// tell the user *why* it refuses the device.
    #[test]
    fn a_legacy_18_byte_greeting_parses_without_an_fmv() {
        let mut raw = [0u8; 18];
        raw[..13].copy_from_slice(b"picodroid/2.0");
        raw[14..18].copy_from_slice(&0x0002_0000u32.to_le_bytes());

        let g = Greeting::parse(&raw).unwrap();
        assert!(g.is_legacy());
        assert_eq!(g.max_papk, 0x0002_0000);
        assert_eq!(g.framework_map_version, "");
    }

    #[test]
    fn parse_errors_name_what_is_missing() {
        assert_eq!(Greeting::parse(&[0u8; 4]), Err(GreetingError::TooShort(4)));

        let mut raw = [0u8; 18];
        raw[..14].copy_from_slice(b"picodroid/2.1\0");
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::MissingFmv));

        let mut raw = [0u8; 21];
        raw[..14].copy_from_slice(b"picodroid/2.1\0");
        raw[18] = 9; // promises 9 fmv bytes; only 2 follow
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::TruncatedFmv));

        let mut raw = [0u8; 20];
        raw[..14].copy_from_slice(b"picodroid/2.1\0");
        raw[18] = 1;
        raw[19] = 0xFF;
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::FmvNotUtf8));
    }
}
