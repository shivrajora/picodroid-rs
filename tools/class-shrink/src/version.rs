// SPDX-License-Identifier: GPL-3.0-only
//! Map-version resolution.
//!
//! The **active map version** is the highest committed map file in
//! `sdk/shrink-maps/v*.toml` whose semver is ≤ the picodroid package version
//! in the root `Cargo.toml`. If no committed map exists, the active version
//! is `UNRELEASED_VERSION`, which is a well-defined sentinel meaning "no
//! shrinking map is active yet for this build".

use std::fs;
use std::path::{Path, PathBuf};

/// Sentinel emitted when no committed map file matches the current
/// picodroid version. Semver-sortable; lower than any real release.
pub const UNRELEASED_VERSION: &str = "0.0.0";

/// Parse a `MAJOR.MINOR.PATCH` string into a comparable tuple.
/// Pre-release suffixes (e.g. `-rc1`) are stripped before comparison.
pub fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.split(['-', '+']).next().unwrap_or(s);
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Read the picodroid package version from the root `Cargo.toml`.
/// Naive parser: looks for the first `version = "..."` under `[package]`.
pub fn read_picodroid_version(cargo_toml: &Path) -> Result<String, String> {
    let text = fs::read_to_string(cargo_toml)
        .map_err(|e| format!("failed to read {}: {e}", cargo_toml.display()))?;
    let mut in_package = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start().trim_start_matches('=').trim();
            let v = rest.trim_matches(|c| c == '"' || c == '\'');
            if parse_semver(v).is_some() {
                return Ok(v.to_string());
            }
        }
    }
    Err(format!(
        "no [package] version found in {}",
        cargo_toml.display()
    ))
}

/// List all committed map files under `shrink_maps_dir`. File names must
/// match `v<semver>.toml`; other entries are ignored.
pub fn list_committed_maps(shrink_maps_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(shrink_maps_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_prefix('v').and_then(|s| s.strip_suffix(".toml")) else {
            continue;
        };
        if parse_semver(stem).is_some() {
            out.push((stem.to_string(), path));
        }
    }
    out.sort_by_key(|a| parse_semver(&a.0));
    out
}

/// Resolve the active map version for a given picodroid version string.
/// Returns `UNRELEASED_VERSION` if no committed map is ≤ `pkg_version`.
pub fn resolve_active_version(pkg_version: &str, shrink_maps_dir: &Path) -> String {
    let Some(pkg) = parse_semver(pkg_version) else {
        return UNRELEASED_VERSION.to_string();
    };
    list_committed_maps(shrink_maps_dir)
        .into_iter()
        .filter_map(|(v, _)| parse_semver(&v).map(|sv| (v, sv)))
        .rfind(|(_, sv)| *sv <= pkg)
        .map(|(v, _)| v)
        .unwrap_or_else(|| UNRELEASED_VERSION.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_semver_basic() {
        assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_semver("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("not-a-version"), None);
        assert_eq!(parse_semver("1.2"), None);
    }

    #[test]
    fn resolve_picks_highest_leq() {
        let td = tempdir("highest_leq");
        for v in &["v0.1.0.toml", "v0.2.0.toml", "v0.3.0.toml"] {
            fs::write(td.join(v), "").unwrap();
        }
        assert_eq!(resolve_active_version("0.2.5", &td), "0.2.0");
        assert_eq!(resolve_active_version("0.3.0", &td), "0.3.0");
        assert_eq!(resolve_active_version("0.0.9", &td), UNRELEASED_VERSION);
    }

    #[test]
    fn resolve_no_maps() {
        let td = tempdir("no_maps");
        assert_eq!(resolve_active_version("0.1.0", &td), UNRELEASED_VERSION);
    }

    fn tempdir(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("class-shrink-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
}
