//! Picodroid PAPK ↔ firmware compatibility check.
//!
//! The same rule runs in two places:
//!
//! - **Device** (`pico-jvm`'s `Papk::verify_compat`) — at PAPK load time, so
//!   a baked-in or hot-swapped PAPK that doesn't match the firmware's
//!   `framework-map-version` is rejected before any class is loaded.
//! - **Host** (`tools/pdb`'s `install` pre-flight) — refuses to push a bad
//!   PAPK over USB, so the device never reboots into an incompatible image.
//!
//! Keeping the rule in one `no_std` crate prevents the two paths from
//! drifting as the shrink-map design evolves (e.g. when method-level
//! shrinking lands and adds new compat conditions).
//!
//! ## Compatibility rules
//!
//! | Firmware    | PAPK        | Result                          |
//! |-------------|-------------|---------------------------------|
//! | `0.0.0`     | `0.0.0`     | OK — both unshrunk              |
//! | `0.0.0`     | non-zero    | `Mismatch` — asymmetric         |
//! | non-zero    | `0.0.0`     | `Mismatch` — asymmetric         |
//! | `v` (≥1)    | `v'` ≤ `v`  | OK — append-only invariant      |
//! | `v` (≥1)    | `v'` > `v`  | `Mismatch` — PAPK from future   |
//! | anything    | `None`      | OK iff firmware = `0.0.0`, else `Missing` |
//!
//! See `docs/shrinker.md` for the broader design.

#![no_std]

/// Reasons a PAPK ↔ firmware compatibility check can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatError {
    /// One of the version strings is not parseable as `MAJOR.MINOR.PATCH`.
    BadVersion,
    /// PAPK and firmware versions are incompatible — either asymmetric
    /// (one side is the `0.0.0` sentinel and the other isn't) or PAPK
    /// is newer than firmware.
    Mismatch,
    /// PAPK predates the `framework-map-version` manifest key (legacy)
    /// and the firmware is past the `0.0.0` sentinel — can't tell whether
    /// it's compatible, so reject.
    Missing,
}

/// Apply the compatibility rule. `papk_version = None` means the PAPK has
/// no `framework-map-version` manifest key (legacy pre-M1 PAPK). Returns
/// `Ok(())` on accept; the precise [`CompatError`] on reject.
pub fn check(papk_version: Option<&str>, firmware_version: &str) -> Result<(), CompatError> {
    let fw = parse_semver(firmware_version).ok_or(CompatError::BadVersion)?;
    let papk_str = match papk_version {
        Some(v) => v,
        None => {
            // Legacy PAPK without the manifest key is only compatible with
            // firmware at the `0.0.0` sentinel.
            return if fw == (0, 0, 0) {
                Ok(())
            } else {
                Err(CompatError::Missing)
            };
        }
    };
    let pv = parse_semver(papk_str).ok_or(CompatError::BadVersion)?;
    let papk_zero = pv == (0, 0, 0);
    let fw_zero = fw == (0, 0, 0);
    if papk_zero && fw_zero {
        return Ok(());
    }
    if papk_zero != fw_zero {
        // One side shrunk, the other not — asymmetric, reject.
        return Err(CompatError::Mismatch);
    }
    if pv <= fw {
        Ok(())
    } else {
        Err(CompatError::Mismatch)
    }
}

/// Parse a `MAJOR.MINOR.PATCH` semver string into a comparable tuple.
/// Pre-release and build suffixes (`-rc1`, `+build.123`) are stripped
/// before parsing so `"0.1.0-rc1"` and `"0.1.0"` compare equal.
/// Returns `None` on malformed input. `no_std`/`no_alloc` friendly.
pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let core = match s.find(['-', '+']) {
        Some(i) => &s[..i],
        None => s,
    };
    let mut it = core.split('.');
    let major: u32 = it.next()?.parse().ok()?;
    let minor: u32 = it.next()?.parse().ok()?;
    let patch: u32 = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_semver ─────────────────────────────────────────────────────

    #[test]
    fn parse_semver_basic() {
        assert_eq!(parse_semver("0.0.0"), Some((0, 0, 0)));
        assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
    }

    #[test]
    fn parse_semver_strips_pre_release_and_build() {
        assert_eq!(parse_semver("0.1.0-rc1"), Some((0, 1, 0)));
        assert_eq!(parse_semver("1.2.3+build.5"), Some((1, 2, 3)));
    }

    #[test]
    fn parse_semver_rejects_malformed() {
        assert_eq!(parse_semver(""), None);
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3.4"), None);
        assert_eq!(parse_semver("a.b.c"), None);
    }

    // ── check: both 0.0.0 ────────────────────────────────────────────────

    #[test]
    fn both_zero_accepted() {
        assert_eq!(check(Some("0.0.0"), "0.0.0"), Ok(()));
    }

    // ── check: append-only forward compat ────────────────────────────────

    #[test]
    fn equal_non_zero_accepted() {
        assert_eq!(check(Some("0.1.0"), "0.1.0"), Ok(()));
    }

    #[test]
    fn older_papk_against_newer_firmware_accepted() {
        assert_eq!(check(Some("0.1.0"), "0.2.0"), Ok(()));
        assert_eq!(check(Some("1.0.0"), "1.5.7"), Ok(()));
    }

    #[test]
    fn newer_papk_against_older_firmware_rejected() {
        assert_eq!(check(Some("0.2.0"), "0.1.0"), Err(CompatError::Mismatch));
        assert_eq!(check(Some("1.5.0"), "1.0.0"), Err(CompatError::Mismatch));
    }

    // ── check: asymmetric (the bug we're guarding against) ──────────────

    #[test]
    fn unshrunk_papk_against_shrunk_firmware_rejected() {
        // PAPK built without --shrink (refs original framework names) loaded
        // on firmware built with --shrink (only has shrunk names). Linkage
        // would fail; reject early.
        assert_eq!(check(Some("0.0.0"), "0.1.0"), Err(CompatError::Mismatch));
    }

    #[test]
    fn shrunk_papk_against_unshrunk_firmware_rejected() {
        // Symmetric guard.
        assert_eq!(check(Some("0.1.0"), "0.0.0"), Err(CompatError::Mismatch));
    }

    // ── check: legacy PAPK (no manifest key) ────────────────────────────

    #[test]
    fn unversioned_papk_against_sentinel_firmware_accepted() {
        // Pre-M1 PAPKs lack the manifest key; only compatible with
        // pre-shrink firmware.
        assert_eq!(check(None, "0.0.0"), Ok(()));
    }

    #[test]
    fn unversioned_papk_against_released_firmware_rejected() {
        assert_eq!(check(None, "0.1.0"), Err(CompatError::Missing));
    }

    // ── check: malformed input ──────────────────────────────────────────

    #[test]
    fn bad_papk_version_returns_bad_version() {
        assert_eq!(
            check(Some("not-a-semver"), "0.0.0"),
            Err(CompatError::BadVersion)
        );
    }

    #[test]
    fn bad_firmware_version_returns_bad_version() {
        assert_eq!(
            check(Some("0.0.0"), "not-a-semver"),
            Err(CompatError::BadVersion)
        );
    }
}
