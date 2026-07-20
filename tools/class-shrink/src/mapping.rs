// SPDX-License-Identifier: GPL-3.0-only
//! Release-versioned shrink map: original → shrunk class name pairs.
//!
//! The v1 map is class-only. Methods and fields are deferred to a later
//! release; existing entries will stay untouched when that happens
//! (append-only invariant).
//!
//! On-disk format (`sdk/shrink-maps/v<semver>.toml`): hand-authored-looking
//! minimal TOML — one table per class. A zero-dep writer/reader is enough
//! for the shapes we use, so we avoid pulling in a full toml crate.
//!
//! ```toml
//! # v0.1.0
//! schema = 1
//!
//! [[class]]
//! from = "picodroid/pio/Gpio"
//! to   = "a/A"
//!
//! [[class]]
//! from = "picodroid/pio/PeripheralManager"
//! to   = "a/B"
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Current on-disk schema version for map files. Bump when the file format
/// changes incompatibly.
pub const SCHEMA_VERSION: u32 = 1;

/// A loaded shrink map.
#[derive(Clone, Debug, Default)]
pub struct ShrinkMap {
    /// Original internal class name → shrunk internal class name.
    /// `BTreeMap` for deterministic iteration order.
    pub classes: BTreeMap<String, String>,
}

impl ShrinkMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sorted iter over (original, shrunk) class pairs.
    pub fn iter_classes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.classes.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut out = String::new();
        out.push_str(&format!("# Shrink map (schema v{SCHEMA_VERSION})\n"));
        out.push_str(&format!("schema = {SCHEMA_VERSION}\n\n"));
        for (from, to) in self.iter_classes() {
            out.push_str("[[class]]\n");
            out.push_str(&format!("from = {}\n", toml_string(from)));
            out.push_str(&format!("to   = {}\n\n", toml_string(to)));
        }
        fs::write(path, out)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        parse(&text)
    }

    /// Whether the map shrinks any classes.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
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
        let mut by_target: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (from, to) in self.iter_classes() {
            by_target.entry(to).or_default().push(from);
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
        let dups = self.duplicate_targets();
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
///   - `schema = N`
///   - `[[class]]` table headers
///   - `key = "value"` inside class tables (keys: `from`, `to`)
fn parse(text: &str) -> io::Result<ShrinkMap> {
    let mut map = ShrinkMap::new();
    let mut in_class = false;
    let mut cur_from: Option<String> = None;
    let mut cur_to: Option<String> = None;

    let flush = |map: &mut ShrinkMap,
                 in_class: &mut bool,
                 from: &mut Option<String>,
                 to: &mut Option<String>|
     -> io::Result<()> {
        if *in_class {
            let f = from.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "[[class]] missing 'from'")
            })?;
            let t = to.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "[[class]] missing 'to'")
            })?;
            map.classes.insert(f, t);
            *in_class = false;
        }
        Ok(())
    };

    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[class]]" {
            flush(&mut map, &mut in_class, &mut cur_from, &mut cur_to)?;
            in_class = true;
            continue;
        }
        if let Some((k, v)) = parse_kv(line) {
            if k == "schema" {
                let n: u32 = v.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("line {}: bad schema number", lineno + 1),
                    )
                })?;
                if n != SCHEMA_VERSION {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("unsupported map schema {n} (want {SCHEMA_VERSION})"),
                    ));
                }
                continue;
            }
            if in_class {
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
    flush(&mut map, &mut in_class, &mut cur_from, &mut cur_to)?;
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
}
