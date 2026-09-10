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
//! [14] version string, NUL-padded            ("picodroid/2.2\0")
//! [4]  max PAPK size in bytes, u32 LE
//! [1]  framework-map-version length
//! [N]  framework-map-version bytes
//! [10] apps tail, multi-app firmware only:   [max u8][installed u8]
//!                                            [largest free bytes u32 LE]
//!                                            [total free bytes u32 LE]
//! ```
//!
//! The apps tail is what `picodroid/2.2` added; a single-app board sends
//! none, and a host tells the two apart by the payload length.
//!
//! The device encodes with [`encode`]; the host decodes with
//! [`Greeting::parse`]. What a version *means* — which ones are refused, and
//! with what message — is host policy and stays in `tools/pdb`; this module
//! owns only the bytes.

/// What current firmware answers to `CMD_PING`. Bumping this is a protocol
/// change, not an edit: the golden test pins the greeting prefix per version.
pub const PROTOCOL_VERSION: &str = "picodroid/2.2";

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

/// Bytes of the apps tail a multi-app firmware appends.
pub const APPS_TAIL_LEN: usize = 10;

/// Largest greeting [`encode_with_apps`] can produce — size wire buffers
/// with this.
pub const GREETING_MAX: usize = LEGACY_GREETING_LEN + 1 + FMV_MAX + APPS_TAIL_LEN;

/// What a multi-app firmware says about its package directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppsInfo {
    /// Directory capacity.
    pub max: u8,
    pub installed: u8,
    /// Largest contiguous free run of the app region, in bytes.
    pub largest_free: u32,
    /// All free sectors of the app region, in bytes.
    pub total_free: u32,
}

/// [`encode`] plus the apps tail.
pub fn encode_with_apps(
    max_papk: u32,
    framework_map_version: &[u8],
    apps: &AppsInfo,
    out: &mut [u8; GREETING_MAX],
) -> usize {
    let n = encode(max_papk, framework_map_version, out);
    out[n] = apps.max;
    out[n + 1] = apps.installed;
    out[n + 2..n + 6].copy_from_slice(&apps.largest_free.to_le_bytes());
    out[n + 6..n + 10].copy_from_slice(&apps.total_free.to_le_bytes());
    n + APPS_TAIL_LEN
}

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
    /// The package directory, on multi-app firmware; `None` on a single-app
    /// board and on firmware older than `picodroid/2.2`.
    pub apps: Option<AppsInfo>,
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
                apps: None,
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

        // The apps tail is additive: present iff the bytes are there.
        let apps = payload
            .get(fmv_end..fmv_end + APPS_TAIL_LEN)
            .map(|t| AppsInfo {
                max: t[0],
                installed: t[1],
                largest_free: u32::from_le_bytes(t[2..6].try_into().unwrap()),
                total_free: u32::from_le_bytes(t[6..10].try_into().unwrap()),
            });

        Ok(Greeting {
            version,
            max_papk,
            framework_map_version,
            apps,
        })
    }

    /// The minor protocol number (`2` for `picodroid/2.2`), if the version
    /// string has the expected shape.
    pub fn minor(&self) -> Option<u32> {
        self.version
            .strip_prefix(VERSION_PREFIX)?
            .split('.')
            .nth(1)?
            .parse()
            .ok()
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

        assert_eq!(&buf[..14], b"picodroid/2.2\0");
        assert_eq!(&buf[14..18], &[0xD4, 0xC3, 0xB2, 0xA1]);
        assert_eq!(buf[18], 7);
        assert_eq!(&buf[19..26], b"7.3.1-g");
        assert_eq!(n, 26);
    }

    /// The apps tail, byte for byte, and a parse that sees it — while a
    /// greeting without one still parses to `apps: None`.
    #[test]
    fn apps_tail_golden_bytes_and_round_trip() {
        let apps = AppsInfo {
            max: 8,
            installed: 3,
            largest_free: 0x0004_B000,
            total_free: 0x0007_C000,
        };
        let mut buf = [0u8; GREETING_MAX];
        let n = encode_with_apps(1, b"0.20.0", &apps, &mut buf);
        assert_eq!(n, 18 + 1 + 6 + APPS_TAIL_LEN);
        assert_eq!(buf[25], 8);
        assert_eq!(buf[26], 3);
        assert_eq!(&buf[27..31], &[0x00, 0xB0, 0x04, 0x00]);
        assert_eq!(&buf[31..35], &[0x00, 0xC0, 0x07, 0x00]);
        let g = Greeting::parse(&buf[..n]).unwrap();
        assert_eq!(g.apps, Some(apps));
        assert_eq!(g.minor(), Some(2));

        let m = encode(1, b"0.20.0", &mut buf);
        let g = Greeting::parse(&buf[..m]).unwrap();
        assert_eq!(g.apps, None);
        assert_eq!(g.framework_map_version, "0.20.0");
    }

    /// The first [`LEGACY_GREETING_LEN`] bytes are the frozen contract with
    /// `picodroid/2.0` hosts, which read exactly that many and no more.
    #[test]
    fn legacy_prefix_is_frozen_at_18_bytes() {
        assert_eq!(LEGACY_GREETING_LEN, 18);
        let mut buf = [0u8; GREETING_MAX];
        encode(0x0010_0000, b"", &mut buf);
        assert_eq!(&buf[..VERSION_FIELD_LEN], b"picodroid/2.2\0");
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
        assert_eq!(n, GREETING_MAX - APPS_TAIL_LEN);
        assert_eq!(buf[LEGACY_GREETING_LEN] as usize, FMV_MAX);
        // And with the tail, exactly the buffer.
        let apps = AppsInfo {
            max: 1,
            installed: 0,
            largest_free: 0,
            total_free: 0,
        };
        assert_eq!(encode_with_apps(1, &fmv, &apps, &mut buf), GREETING_MAX);
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
        raw[..14].copy_from_slice(b"picodroid/2.2\0");
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::MissingFmv));

        let mut raw = [0u8; 21];
        raw[..14].copy_from_slice(b"picodroid/2.2\0");
        raw[18] = 9; // promises 9 fmv bytes; only 2 follow
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::TruncatedFmv));

        let mut raw = [0u8; 20];
        raw[..14].copy_from_slice(b"picodroid/2.2\0");
        raw[18] = 1;
        raw[19] = 0xFF;
        assert_eq!(Greeting::parse(&raw), Err(GreetingError::FmvNotUtf8));
    }
}
