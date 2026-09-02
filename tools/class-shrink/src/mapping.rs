// SPDX-License-Identifier: GPL-3.0-only
//! Release-versioned shrink map: original → shrunk name pairs for classes
//! and, since schema 2, for members (methods and fields).
//!
//! Members share one namespace: the map is keyed by bare member name, not
//! by owner, so `setText` renames the same way in every class that declares
//! or calls it — that is what keeps an app's override in lockstep with the
//! framework method it overrides. JVM method and field namespaces are
//! disjoint, so one target per original name serves both kinds.
//!
//! Both sections are append-only across releases. `member-floor` records
//! the first release that carried member entries: a PAPK shrunk before it
//! still spells every member in full and cannot run on firmware at or past
//! it (`compat::MEMBER_SHRINK_FLOOR`).
//!
//! On-disk format (`sdk/shrink-maps/v<semver>.toml`): hand-authored-looking
//! minimal TOML — one table per entry. A zero-dep writer/reader is enough
//! for the shapes we use, so we avoid pulling in a full toml crate.
//!
//! ```toml
//! # v0.16.0
//! schema = 2
//! member-floor = "0.16.0"
//!
//! [[class]]
//! from = "picodroid/pio/Gpio"
//! to   = "a/A"
//!
//! [[member]]
//! from = "nativeCreate"
//! to   = "aB"
//! ```
//!
//! Schema-1 files (classes only) still load; their member section is empty.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Current on-disk schema version for map files. Bump when the file format
/// changes incompatibly. Every schema up to this one is still readable.
pub const SCHEMA_VERSION: u32 = 2;

/// A loaded shrink map.
#[derive(Clone, Debug, Default)]
pub struct ShrinkMap {
    /// Original internal class name → shrunk internal class name.
    /// `BTreeMap` for deterministic iteration order.
    pub classes: BTreeMap<String, String>,
    /// Original member (method or field) name → shrunk name. Owner-agnostic.
    pub members: BTreeMap<String, String>,
    /// First release whose map carried member entries, carried forward
    /// verbatim by every later cut. `None` while no member has been mapped.
    pub member_floor: Option<String>,
}

impl ShrinkMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sorted iter over (original, shrunk) class pairs.
    pub fn iter_classes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.classes.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Sorted iter over (original, shrunk) member pairs.
    pub fn iter_members(&self) -> impl Iterator<Item = (&str, &str)> {
        self.members.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Whether the map renames any member.
    pub fn has_members(&self) -> bool {
        !self.members.is_empty()
    }

    /// The shrunk spelling of a member name, or `None` if it is unmapped.
    pub fn member_target(&self, original: &str) -> Option<&str> {
        self.members.get(original).map(String::as_str)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut out = String::new();
        out.push_str(&format!("# Shrink map (schema v{SCHEMA_VERSION})\n"));
        out.push_str(&format!("schema = {SCHEMA_VERSION}\n"));
        if let Some(floor) = &self.member_floor {
            out.push_str(&format!("member-floor = {}\n", toml_string(floor)));
        }
        out.push('\n');
        for (from, to) in self.iter_classes() {
            out.push_str("[[class]]\n");
            out.push_str(&format!("from = {}\n", toml_string(from)));
            out.push_str(&format!("to   = {}\n\n", toml_string(to)));
        }
        for (from, to) in self.iter_members() {
            out.push_str("[[member]]\n");
            out.push_str(&format!("from = {}\n", toml_string(from)));
            out.push_str(&format!("to   = {}\n\n", toml_string(to)));
        }
        fs::write(path, out)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        parse(&text)
    }

    /// Whether the map shrinks anything at all.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.members.is_empty()
    }

    /// Find shrunk names that more than one original class maps to.
    ///
    /// The map is keyed by original name, so duplicate *originals* are
    /// impossible by construction — but a bug in the short-name allocator
    /// (e.g. a desynced raw-index counter skipping a reserved keyword) can
    /// assign the same *shrunk* name to two unrelated classes. That silently
    /// corrupts any shrunk build using either class, so the shrink map must
    /// be an injective (1:1) mapping. Returns each colliding shrunk name with
    /// the sorted list of originals that claim it; empty means the map is
    /// injective.
    pub fn duplicate_targets(&self) -> Vec<(String, Vec<String>)> {
        Self::duplicate_targets_in(&self.classes)
    }

    /// [`duplicate_targets`](Self::duplicate_targets) for the member table.
    /// Members are their own namespace: a member target never collides with
    /// a class target (class targets carry a `/`).
    pub fn duplicate_member_targets(&self) -> Vec<(String, Vec<String>)> {
        Self::duplicate_targets_in(&self.members)
    }

    fn duplicate_targets_in(table: &BTreeMap<String, String>) -> Vec<(String, Vec<String>)> {
        let mut by_target: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (from, to) in table {
            by_target
                .entry(to.as_str())
                .or_default()
                .push(from.as_str());
        }
        by_target
            .into_iter()
            .filter(|(_, froms)| froms.len() > 1)
            .map(|(to, froms)| {
                (
                    to.to_string(),
                    froms.iter().map(|s| s.to_string()).collect(),
                )
            })
            .collect()
    }

    /// Assert the map is an injective original → shrunk mapping. Returns a
    /// human-readable error listing every collision if not.
    pub fn verify_injective(&self) -> Result<(), String> {
        let mut dups = self.duplicate_targets();
        dups.extend(self.duplicate_member_targets());
        if dups.is_empty() {
            return Ok(());
        }
        let mut msg = String::from("shrink map has duplicate shrunk names (must be 1:1):");
        for (to, froms) in &dups {
            msg.push_str(&format!("\n  {to} <- {}", froms.join(", ")));
        }
        Err(msg)
    }
}

/// Format a string as a TOML basic-string literal with `"` escaping.
/// Framework class names only use `[a-zA-Z0-9_/$]`, so `"` is never
/// needed inside — we still escape for safety.
fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Lightweight parser tailored to the exact layout `save` produces.
/// Supports:
///   - blank / comment-only lines
///   - `schema = N` (1 or 2)
///   - `member-floor = "x.y.z"` (schema 2)
///   - `[[class]]` / `[[member]]` table headers
///   - `key = "value"` inside tables (keys: `from`, `to`)
fn parse(text: &str) -> io::Result<ShrinkMap> {
    #[derive(Clone, Copy, PartialEq)]
    enum Section {
        None,
        Class,
        Member,
    }
    let mut map = ShrinkMap::new();
    let mut section = Section::None;
    let mut cur_from: Option<String> = None;
    let mut cur_to: Option<String> = None;

    let flush = |map: &mut ShrinkMap,
                 section: &mut Section,
                 from: &mut Option<String>,
                 to: &mut Option<String>|
     -> io::Result<()> {
        if *section != Section::None {
            let kind = if *section == Section::Class {
                "[[class]]"
            } else {
                "[[member]]"
            };
            let f = from.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{kind} missing 'from'"))
            })?;
            let t = to.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{kind} missing 'to'"))
            })?;
            match *section {
                Section::Class => map.classes.insert(f, t),
                _ => map.members.insert(f, t),
            };
            *section = Section::None;
        }
        Ok(())
    };

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[class]]" || line == "[[member]]" {
            flush(&mut map, &mut section, &mut cur_from, &mut cur_to)?;
            section = if line == "[[class]]" {
                Section::Class
            } else {
                Section::Member
            };
            continue;
        }
        if let Some((k, v)) = parse_kv(line) {
            if section == Section::None && k == "schema" {
                let n: u32 = v.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("line {}: bad schema number", lineno + 1),
                    )
                })?;
                if n == 0 || n > SCHEMA_VERSION {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unsupported map schema {n} (want 1..={SCHEMA_VERSION})"),
                    ));
                }
                continue;
            }
            if section == Section::None && k == "member-floor" {
                let val = strip_quotes(&v).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("line {}: expected quoted string", lineno + 1),
                    )
                })?;
                map.member_floor = Some(val);
                continue;
            }
            if section != Section::None {
                // Expect `"..."` quoted string
                let val = strip_quotes(&v).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("line {}: expected quoted string", lineno + 1),
                    )
                })?;
                match k.as_str() {
                    "from" => cur_from = Some(val),
                    "to" => cur_to = Some(val),
                    _ => {}
                }
                continue;
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("line {}: unrecognized content: {raw:?}", lineno + 1),
        ));
    }
    flush(&mut map, &mut section, &mut cur_from, &mut cur_to)?;
    if map.has_members() && map.member_floor.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "map has [[member]] entries but no member-floor",
        ));
    }
    Ok(map)
}

fn parse_kv(line: &str) -> Option<(String, String)> {
    let eq = line.find('=')?;
    let k = line[..eq].trim().to_string();
    let v = line[eq + 1..].trim().to_string();
    Some((k, v))
}

fn strip_quotes(s: &str) -> Option<String> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut m = ShrinkMap::new();
        m.classes.insert("picodroid/pio/Gpio".into(), "a/A".into());
        m.classes
            .insert("picodroid/os/SystemClock".into(), "a/B".into());
        let td = std::env::temp_dir().join(format!("cs-map-roundtrip-{}", std::process::id()));
        let _ = fs::remove_file(&td);
        m.save(&td).unwrap();
        let back = ShrinkMap::load(&td).unwrap();
        assert_eq!(back.classes, m.classes);
    }

    #[test]
    fn rejects_unknown_schema() {
        let text = "schema = 99\n";
        assert!(parse(text).is_err());
        assert!(parse("schema = 0\n").is_err());
    }

    #[test]
    fn schema_1_files_load_with_no_members() {
        let text = "schema = 1\n\n[[class]]\nfrom = \"a/B\"\nto   = \"a/A\"\n";
        let m = parse(text).unwrap();
        assert_eq!(m.classes.len(), 1);
        assert!(!m.has_members());
        assert_eq!(m.member_floor, None);
    }

    #[test]
    fn members_round_trip_with_floor() {
        let mut m = ShrinkMap::new();
        m.classes.insert("picodroid/pio/Gpio".into(), "a/A".into());
        m.members.insert("nativeCreate".into(), "a".into());
        m.members.insert("setText".into(), "aB".into());
        m.member_floor = Some("0.16.0".into());
        let td = std::env::temp_dir().join(format!("cs-map-members-{}", std::process::id()));
        let _ = fs::remove_file(&td);
        m.save(&td).unwrap();
        let back = ShrinkMap::load(&td).unwrap();
        assert_eq!(back.classes, m.classes);
        assert_eq!(back.members, m.members);
        assert_eq!(back.member_floor.as_deref(), Some("0.16.0"));
        assert_eq!(back.member_target("setText"), Some("aB"));
        assert_eq!(back.member_target("other"), None);
        assert!(!back.is_empty());
    }

    #[test]
    fn members_without_a_floor_are_rejected() {
        let text = "schema = 2\n\n[[member]]\nfrom = \"setText\"\nto   = \"aB\"\n";
        assert!(parse(text).is_err());
    }

    #[test]
    fn duplicate_member_targets_are_detected() {
        let mut m = ShrinkMap::new();
        m.members.insert("setText".into(), "aB".into());
        m.members.insert("getText".into(), "aB".into());
        m.member_floor = Some("0.16.0".into());
        assert_eq!(m.duplicate_member_targets().len(), 1);
        assert!(m.duplicate_targets().is_empty());
        assert!(m.verify_injective().is_err());
    }

    #[test]
    fn duplicate_targets_are_detected() {
        let mut m = ShrinkMap::new();
        m.classes
            .insert("picodroid/os/IBinder".into(), "a/DP".into());
        m.classes
            .insert("picodroid/text/InputType".into(), "a/DP".into());
        m.classes.insert("picodroid/pio/Gpio".into(), "a/A".into());
        let dups = m.duplicate_targets();
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0].0, "a/DP");
        assert_eq!(
            dups[0].1,
            vec!["picodroid/os/IBinder", "picodroid/text/InputType"]
        );
        assert!(m.verify_injective().is_err());
    }

    #[test]
    fn injective_map_passes() {
        let mut m = ShrinkMap::new();
        m.classes.insert("picodroid/pio/Gpio".into(), "a/A".into());
        m.classes
            .insert("picodroid/os/SystemClock".into(), "a/B".into());
        assert!(m.duplicate_targets().is_empty());
        assert!(m.verify_injective().is_ok());
    }

    /// Committed maps are append-only: every release map must contain its
    /// predecessor's entries verbatim (vN+1 ⊇ vN), or PAPKs shrunk with vN
    /// stop resolving on firmware that ships vN+1. `cut-release --base`
    /// enforces this at generation time; this test re-enforces it over the
    /// committed history so a hand-edit can't slip through review.
    #[test]
    fn committed_maps_are_append_only() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/shrink-maps");
        // Collect (semver-triple, path) so v0.9.0 sorts before v0.10.0 —
        // string order would not.
        let mut versions: Vec<(Vec<u32>, std::path::PathBuf)> = fs::read_dir(&dir)
            .expect("read sdk/shrink-maps")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
            .map(|p| {
                let stem = p.file_stem().unwrap().to_str().unwrap();
                let triple: Vec<u32> = stem
                    .strip_prefix('v')
                    .unwrap_or_else(|| panic!("map name not vX.Y.Z: {stem}"))
                    .split('.')
                    .map(|n| n.parse().unwrap_or_else(|_| panic!("bad version: {stem}")))
                    .collect();
                (triple, p)
            })
            .collect();
        versions.sort();
        assert!(
            versions.len() >= 2,
            "need at least two committed maps to check append-only"
        );
        for pair in versions.windows(2) {
            let (_, prev_path) = &pair[0];
            let (_, next_path) = &pair[1];
            let prev = ShrinkMap::load(prev_path).unwrap();
            let next = ShrinkMap::load(next_path).unwrap();
            let (prev_name, next_name) = (
                prev_path.file_name().unwrap().to_str().unwrap(),
                next_path.file_name().unwrap().to_str().unwrap(),
            );
            for (from, to) in prev.iter_classes() {
                match next.classes.get(from) {
                    Some(t) if t == to => {}
                    Some(t) => panic!(
                        "{next_name} remaps {from}: {to} ({prev_name}) -> {t}; \
                         maps are append-only"
                    ),
                    None => panic!(
                        "{next_name} drops {from} -> {to} present in {prev_name}; \
                         maps are append-only"
                    ),
                }
            }
            for (from, to) in prev.iter_members() {
                match next.members.get(from) {
                    Some(t) if t == to => {}
                    Some(t) => panic!(
                        "{next_name} remaps member {from}: {to} ({prev_name}) -> {t}; \
                         maps are append-only"
                    ),
                    None => panic!(
                        "{next_name} drops member {from} -> {to} present in {prev_name}; \
                         maps are append-only"
                    ),
                }
            }
            // A cut may raise the floor (it renamed names the previous map
            // spelled verbatim, e.g. v0.17.0's contract members) but never
            // lower it or drop it.
            if let Some(floor) = &prev.member_floor {
                let n = next
                    .member_floor
                    .as_ref()
                    .unwrap_or_else(|| panic!("{next_name} drops {prev_name}'s member-floor"));
                assert!(
                    crate::version::parse_semver(n) >= crate::version::parse_semver(floor),
                    "{next_name} lowers the member-floor from {floor} ({prev_name}) to {n}"
                );
            }
        }
    }

    /// Every committed release map must be a 1:1 mapping. This guards the
    /// whole `sdk/shrink-maps/` history against the allocator-collision class
    /// of bug in one place, for past and future maps alike.
    #[test]
    fn all_committed_maps_are_injective() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/shrink-maps");
        let mut checked = 0;
        for entry in fs::read_dir(&dir).expect("read sdk/shrink-maps") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let map =
                ShrinkMap::load(&path).unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
            map.verify_injective()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            checked += 1;
        }
        assert!(checked > 0, "no committed maps found in {}", dir.display());
    }

    /// Every committed entry lives in the namespace `rename::namespace_for`
    /// assigns it: `java/**` under `b/`, everything else under `a/`. pico-jvm
    /// reverse-translates only `b/` and picodroid-core only `a/`, so a name
    /// filed under the wrong prefix would load but never dispatch.
    #[test]
    fn committed_maps_respect_namespaces() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/shrink-maps");
        let mut checked = 0;
        for entry in fs::read_dir(&dir).expect("read sdk/shrink-maps") {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let map = ShrinkMap::load(&path).unwrap();
            for (from, to) in map.iter_classes() {
                let want = crate::rename::namespace_for(from).prefix();
                assert!(
                    to.starts_with(want),
                    "{}: {from} -> {to} must be allocated under {want}",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no committed map entries found in {}",
            dir.display()
        );
    }
}
