// SPDX-License-Identifier: GPL-3.0-only
//! Keep list — names that must NEVER be shrunk.
//!
//! v1 only tracks class-name keeps (method/field keeps come with method/field
//! shrinking in a later release). Patterns:
//!
//!   - `"java/**"`      — glob matching `java/lang/Object`, `java/util/HashMap`, etc.
//!   - `"picodroid/annotation/KeepName"` — literal internal name.
//!
//! Format: simple TOML; one source of truth committed at `sdk/keep.toml`.
//!
//! ```toml
//! [[class]]
//! name = "picodroid/os/Activity"
//!
//! [[glob]]
//! pattern = "java/**"
//! ```

use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct KeepList {
    pub exact: Vec<String>,
    pub globs: Vec<String>,
}

impl KeepList {
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        parse(&text)
    }

    /// Returns true if `class_internal_name` (e.g. `"picodroid/pio/Gpio"`)
    /// matches any keep entry.
    pub fn is_kept(&self, class_internal_name: &str) -> bool {
        if self.exact.iter().any(|k| k == class_internal_name) {
            return true;
        }
        for g in &self.globs {
            if glob_match(g, class_internal_name) {
                return true;
            }
        }
        false
    }
}

/// Supports `**` (any path including slashes) and `*` (no slashes).
/// `java/**` matches `java/lang/Object` and `java/util/List`.
fn glob_match(pattern: &str, name: &str) -> bool {
    // Convert to a regex-free matcher: split on `**`, then each segment is
    // a shell-style glob matched against successive prefixes of `name`.
    fn match_simple(pat: &str, s: &str) -> bool {
        // No `**` inside. `*` matches any sequence of non-`/` chars.
        let (mut pi, mut si) = (0usize, 0usize);
        let pb = pat.as_bytes();
        let sb = s.as_bytes();
        let mut star: Option<(usize, usize)> = None; // (pat after *, s pos)
        while si < sb.len() {
            if pi < pb.len() && pb[pi] == b'*' {
                star = Some((pi + 1, si));
                pi += 1;
            } else if pi < pb.len() && (pb[pi] == sb[si]) {
                pi += 1;
                si += 1;
            } else if let Some((np, ns)) = star {
                if sb[ns] == b'/' {
                    return false;
                }
                pi = np;
                si = ns + 1;
                star = Some((np, si));
            } else {
                return false;
            }
        }
        while pi < pb.len() && pb[pi] == b'*' {
            pi += 1;
        }
        pi == pb.len()
    }

    if pattern.contains("**") {
        // Split on "**"; "**" matches any chars including `/`.
        let parts: Vec<&str> = pattern.split("**").collect();
        let mut s = name;
        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                if !s.starts_with(part) {
                    return false;
                }
                s = &s[part.len()..];
            } else if i == parts.len() - 1 {
                // Final part: must match end of s, but allow any
                // intervening characters by searching.
                if part.is_empty() {
                    return true;
                }
                return s.ends_with(part);
            } else {
                let Some(idx) = s.find(part) else {
                    return false;
                };
                s = &s[idx + part.len()..];
            }
        }
        true
    } else {
        match_simple(pattern, name)
    }
}

/// Parse hand-authored TOML-ish keep list. Zero-dep parser tailored to the
/// shape we need.
fn parse(text: &str) -> io::Result<KeepList> {
    let mut keep = KeepList::default();
    let mut section: Option<&'static str> = None;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[class]]" {
            section = Some("class");
            continue;
        }
        if line == "[[glob]]" {
            section = Some("glob");
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"');
            match (section, k) {
                (Some("class"), "name") => keep.exact.push(v.to_string()),
                (Some("glob"), "pattern") => keep.globs.push(v.to_string()),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "line {}: unexpected key {k:?} in section {section:?}",
                            lineno + 1
                        ),
                    ));
                }
            }
            continue;
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("line {}: unrecognized line: {raw:?}", lineno + 1),
        ));
    }
    Ok(keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let mut k = KeepList::default();
        k.exact.push("foo/Bar".into());
        assert!(k.is_kept("foo/Bar"));
        assert!(!k.is_kept("foo/Baz"));
    }

    #[test]
    fn glob_java_double_star() {
        let mut k = KeepList::default();
        k.globs.push("java/**".into());
        assert!(k.is_kept("java/lang/Object"));
        assert!(k.is_kept("java/util/Map$Entry"));
        assert!(!k.is_kept("picodroid/pio/Gpio"));
    }

    #[test]
    fn parses_sample() {
        let text = r#"
# Sample keep list
[[class]]
name = "foo/Bar"

[[glob]]
pattern = "java/**"
"#;
        let k = parse(text).unwrap();
        assert_eq!(k.exact, vec!["foo/Bar".to_string()]);
        assert_eq!(k.globs, vec!["java/**".to_string()]);
    }
}
