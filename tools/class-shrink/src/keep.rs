// SPDX-License-Identifier: GPL-3.0-only
//! Keep list — names that must NEVER be shrunk.
//!
//! v1 only tracks class-name keeps (method/field keeps come with method/field
//! shrinking in a later release). Patterns:
//!
//!   - `"kotlin/**"`    — glob matching `kotlin/Unit`, `kotlin/jvm/internal/Intrinsics`, etc.
//!   - `"picodroid/annotation/KeepName"` — literal internal name.
//!
//! `java/**` is not kept: those names are shrunk under the `b/` namespace and
//! reverse-translated by pico-jvm at the class-file boundary (see
//! `rename::Namespace`).
//!
//! Format: simple TOML; one source of truth committed at `sdk/keep.toml`.
//!
//! ```toml
//! [[class]]
//! name = "picodroid/os/Activity"
//!
//! [[glob]]
//! pattern = "kotlin/**"
//! ```

use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct KeepList {
    pub exact: Vec<String>,
    pub globs: Vec<String>,
    /// Member names never mapped (`[[member]]`).
    pub members: Vec<String>,
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

    /// Returns true if `member_name` is listed under `[[member]]`.
    pub fn is_member_kept(&self, member_name: &str) -> bool {
        self.members.iter().any(|k| k == member_name)
    }
}

/// Supports `**` (any path including slashes; `**/` also matches no
/// directory at all) and `*` (no slashes), in any combination:
/// `java/**` matches `java/lang/Object` and `java/util/List`,
/// `kotlin/**/Intrinsics*` matches `kotlin/jvm/internal/Intrinsics$Ref` and
/// `kotlin/Intrinsics`. One backtracking matcher: the old split-on-`**`
/// scheme treated a `*` inside a pattern that also had `**` as a literal
/// star, so the first mixed pattern silently kept nothing.
fn glob_match(pattern: &str, name: &str) -> bool {
    fn rec(p: &[u8], s: &[u8]) -> bool {
        match p {
            [] => s.is_empty(),
            // `**/`: no directory, or any run of directories.
            [b'*', b'*', b'/', rest @ ..] => {
                rec(rest, s) || (1..=s.len()).any(|i| s[i - 1] == b'/' && rec(rest, &s[i..]))
            }
            // `**`: anything, slashes included.
            [b'*', b'*', rest @ ..] => (0..=s.len()).any(|i| rec(rest, &s[i..])),
            // `*`: anything but a slash.
            [b'*', rest @ ..] => (0..=s.len())
                .take_while(|&i| i == 0 || s[i - 1] != b'/')
                .any(|i| rec(rest, &s[i..])),
            [c, rest @ ..] => s.first() == Some(c) && rec(rest, &s[1..]),
        }
    }
    rec(pattern.as_bytes(), name.as_bytes())
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
        if line == "[[member]]" {
            section = Some("member");
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"');
            match (section, k) {
                (Some("class"), "name") => keep.exact.push(v.to_string()),
                (Some("glob"), "pattern") => keep.globs.push(v.to_string()),
                (Some("member"), "name") => keep.members.push(v.to_string()),
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
    fn glob_double_star_spans_packages() {
        let mut k = KeepList::default();
        k.globs.push("kotlin/**".into());
        assert!(k.is_kept("kotlin/Unit"));
        assert!(k.is_kept("kotlin/jvm/internal/Intrinsics$Ref"));
        assert!(!k.is_kept("picodroid/pio/Gpio"));
        assert!(!k.is_kept("java/lang/Object"));
    }

    #[test]
    fn star_and_double_star_combine() {
        let mut k = KeepList::default();
        k.globs.push("kotlin/**/Intrinsics*".into());
        assert!(k.is_kept("kotlin/jvm/internal/Intrinsics"));
        assert!(k.is_kept("kotlin/jvm/internal/Intrinsics$Ref"));
        assert!(
            k.is_kept("kotlin/Intrinsics"),
            "`**/` matches no directory too"
        );
        assert!(
            !k.is_kept("kotlin/jvm/internal/IntrinsicsKt/Inner"),
            "`*` stops at a slash"
        );
        assert!(!k.is_kept("kotlinx/Intrinsics"));
        let mut k = KeepList::default();
        k.globs.push("*/Main".into());
        assert!(k.is_kept("app/Main"));
        assert!(!k.is_kept("app/sub/Main"));
        let mut k = KeepList::default();
        k.globs.push("**Kt".into());
        assert!(k.is_kept("a/b/UtilsKt"));
        assert!(!k.is_kept("a/b/Utils"));
    }

    #[test]
    fn parses_sample() {
        let text = r#"
# Sample keep list
[[class]]
name = "foo/Bar"

[[glob]]
pattern = "kotlin/**"

[[member]]
name = "main"
"#;
        let k = parse(text).unwrap();
        assert_eq!(k.exact, vec!["foo/Bar".to_string()]);
        assert_eq!(k.globs, vec!["kotlin/**".to_string()]);
        assert_eq!(k.members, vec!["main".to_string()]);
        assert!(k.is_member_kept("main"));
        assert!(!k.is_member_kept("onCreate"));
    }
}
