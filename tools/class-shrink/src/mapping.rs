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

    /// Compose this shrink map with an `alias` map (e.g. `android/* →
    /// picodroid/*`) applied FIRST, producing a single map that does both
    /// rewrites in one constant-pool pass.
    ///
    /// For each `alias_from → alias_to`, the composed entry points
    /// `alias_from` straight at `alias_to`'s final name: if this map shrinks
    /// `alias_to`, the entry becomes `alias_from → shrunk`, otherwise
    /// `alias_from → alias_to`. This map's own entries are kept, so both
    /// `android/view/View` and `picodroid/view/View` resolve to the same final
    /// name. Aliasing must precede shrinking — `android/*` names are not
    /// shrink-map keys, so aliasing after a shrink pass would leave them
    /// pointing at unshrunk names no loaded class matches.
    pub fn composed_with_aliases(&self, alias: &ShrinkMap) -> ShrinkMap {
        let mut out = self.clone();
        for (from, to) in alias.iter_classes() {
            let final_to = self.classes.get(to).map(String::as_str).unwrap_or(to);
            out.classes.insert(from.to_string(), final_to.to_string());
        }
        out
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
    fn compose_aliases_through_shrink() {
        // shrink: picodroid/view/View -> a/A  (picodroid/widget/TextView unshrunk)
        let mut shrink = ShrinkMap::new();
        shrink
            .classes
            .insert("picodroid/view/View".into(), "a/A".into());

        // alias: android/* -> picodroid/*
        let mut alias = ShrinkMap::new();
        alias
            .classes
            .insert("android/view/View".into(), "picodroid/view/View".into());
        alias.classes.insert(
            "android/widget/TextView".into(),
            "picodroid/widget/TextView".into(),
        );

        let composed = shrink.composed_with_aliases(&alias);

        // android name routes through the shrink to the final short name...
        assert_eq!(composed.classes.get("android/view/View").unwrap(), "a/A");
        // ...the picodroid name still maps to the same final name...
        assert_eq!(composed.classes.get("picodroid/view/View").unwrap(), "a/A");
        // ...and an aliased-but-unshrunk class lands on its picodroid name.
        assert_eq!(
            composed.classes.get("android/widget/TextView").unwrap(),
            "picodroid/widget/TextView"
        );
    }

    #[test]
    fn compose_alias_only_when_shrink_empty() {
        let shrink = ShrinkMap::new(); // shrinking off
        let mut alias = ShrinkMap::new();
        alias
            .classes
            .insert("android/util/Log".into(), "picodroid/util/Log".into());
        let composed = shrink.composed_with_aliases(&alias);
        assert_eq!(
            composed.classes.get("android/util/Log").unwrap(),
            "picodroid/util/Log"
        );
        assert_eq!(composed.classes.len(), 1);
    }

    /// Drift guard: every top-level public SDK class under sdk/java/picodroid
    /// must have an `android/<pkg>/<Class>` alias in sdk/compat-aliases.toml.
    /// Regenerate the toml (see its header) when this fails after adding an
    /// SDK class. Walks the source tree so it needs no build.
    #[test]
    fn compat_aliases_cover_every_sdk_class() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repo root")
            .to_path_buf();
        let sdk_src = repo_root.join("sdk/java/picodroid");
        let alias_file = repo_root.join("sdk/compat-aliases.toml");

        let alias = ShrinkMap::load(&alias_file).expect("load compat-aliases.toml");

        let mut missing = Vec::new();
        let mut stack = vec![sdk_src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).expect("read sdk dir") {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
                    let rel = path.strip_prefix(repo_root.join("sdk/java")).unwrap();
                    // picodroid/<pkg>/<Class>.java -> internal name minus ".java"
                    let internal = rel.with_extension("").to_string_lossy().replace('\\', "/");
                    let android = internal.replacen("picodroid/", "android/", 1);
                    if !alias.classes.contains_key(&android) {
                        missing.push(android);
                    }
                }
            }
        }
        missing.sort();
        assert!(
            missing.is_empty(),
            "sdk/compat-aliases.toml is missing aliases for: {missing:?}\n\
             Regenerate it (see the file header)."
        );
    }
}
