// SPDX-License-Identifier: GPL-3.0-only
//! Deterministic short-name allocator for class names.
//!
//! Given a set of classes to shrink (stable, sorted), allocate short internal
//! names `a/A`, `a/B`, …, `a/Z`, `a/AA`, …. All classes land in a synthetic
//! top-level package `a/` so descriptor length stays minimal. Java reserved
//! keywords are skipped so generated names don't collide with language
//! keywords (not a JVM requirement, but avoids surprising tool output).
//!
//! Determinism: callers must sort input by original internal name (byte-wise)
//! before calling. Given identical input this allocator produces identical
//! output across runs.

/// Produce the `n`-th short suffix (0 → "A", 1 → "B", …, 25 → "Z", 26 → "AA").
/// Using uppercase [A-Z] keeps Java-identifier validity and max density.
/// Skips names that collide with Java reserved keywords.
pub fn short_suffix(mut n: usize) -> String {
    loop {
        let s = base26(n);
        if !is_java_reserved(&s) {
            return s;
        }
        n += 1;
    }
}

/// Base-26 using [A-Z], with least-significant digit last. 0 → "A",
/// 26 → "AA" (not "BA" — bijective base-26 is messy but preserves distinct
/// names; we use conventional base-26 here which is fine because we're not
/// decoding, just allocating).
fn base26(mut n: usize) -> String {
    // Convert to bijective base-26 so every non-negative integer maps to a
    // unique non-empty string: 0→A, …, 25→Z, 26→AA, 27→AB, … 701→ZZ, 702→AAA.
    let mut chars = Vec::new();
    n += 1; // shift to 1-based so the math below is clean
    while n > 0 {
        n -= 1;
        chars.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    chars.reverse();
    chars.iter().collect()
}

/// Java reserved words that must not be used as identifiers
/// (JLS §3.9). Short alphabetic names that collide are skipped.
/// Uppercase-only generated names avoid most keywords, but "DO",
/// "IF", etc. are ruled out as they would still cause confusion.
fn is_java_reserved(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
            | "true"
            | "false"
            | "null"
    )
}

/// Compose a full shrunk internal name. The synthetic package `a/` prefix
/// collapses all shrinkable classes under a single directory.
pub fn shrunk_name(suffix: &str) -> String {
    format!("a/{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base26_is_bijective() {
        assert_eq!(base26(0), "A");
        assert_eq!(base26(1), "B");
        assert_eq!(base26(25), "Z");
        assert_eq!(base26(26), "AA");
        assert_eq!(base26(27), "AB");
        assert_eq!(base26(51), "AZ");
        assert_eq!(base26(52), "BA");
        assert_eq!(base26(701), "ZZ");
        assert_eq!(base26(702), "AAA");
    }

    #[test]
    fn uppercase_never_hits_java_keywords() {
        // Keywords are all lowercase, but the reserved check is
        // case-insensitive to be extra safe against tooling quirks.
        // Enumerate first 100 suffixes and confirm none are reserved.
        for n in 0..100 {
            let s = short_suffix(n);
            assert!(!is_java_reserved(&s), "{n} → {s} collides with reserved");
        }
    }

    #[test]
    fn shrunk_full_name_contains_synthetic_package() {
        assert_eq!(shrunk_name("A"), "a/A");
        assert_eq!(shrunk_name("AB"), "a/AB");
    }
}
