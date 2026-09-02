// SPDX-License-Identifier: GPL-3.0-only
//! Deterministic short-name allocator for class names.
//!
//! Given a set of classes to shrink (stable, sorted), allocate short internal
//! names `A`, `B`, …, `Z`, `AA`, … under one of two synthetic top-level
//! packages, so descriptor length stays minimal:
//!
//! - `a/` — framework classes (`picodroid/**`, `javax/**`).
//! - `b/` — `java/**` classes, which pico-jvm serves natively.
//!
//! Neither is translated at run time: the Rust runtime matches both through
//! constants generated from the active map (`build_support/names.rs`).
//!
//! Each namespace has its own counter, so the two suffix sequences are
//! allocated independently and never collide. Java reserved keywords are
//! skipped so generated names don't collide with language keywords (not a
//! JVM requirement, but avoids surprising tool output).
//!
//! Determinism: callers must sort input by original internal name (byte-wise)
//! before calling. Given identical input this allocator produces identical
//! output across runs.

/// Produce the next short suffix and advance `raw` to the following unused
/// raw index. `raw` is the single shared counter across a whole allocation
/// run: threading it through by mutable reference (rather than having each
/// call independently skip ahead from its own `n`) is what keeps consecutive
/// calls from re-landing on the same output when a reserved keyword is
/// skipped. A prior version took `n: usize` by value and had each call
/// re-derive its own skip-ahead from that call's `n`; two adjacent calls
/// straddling a skipped keyword (e.g. raw index 118 = "DO") both resolved to
/// the following index's name ("DP"), producing a silent short-name
/// collision between two unrelated classes.
pub fn short_suffix(raw: &mut usize) -> String {
    loop {
        let n = *raw;
        *raw += 1;
        let s = base26(n);
        if !is_java_reserved(&s) {
            return s;
        }
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

/// Invert `base26`: recover the raw index that produced `s`. Used to derive
/// the next free raw index from a base map's existing suffixes rather than
/// assuming it equals the entry count — that count-based shortcut silently
/// undercounts the moment any past allocation crossed a skipped reserved
/// keyword (see `short_suffix`), since a skip consumes a raw index without
/// producing a map entry.
pub fn base26_inverse(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let mut n: usize = 0;
    for b in s.bytes() {
        let digit = (b - b'A' + 1) as usize;
        n = n.checked_mul(26)?.checked_add(digit)?;
    }
    Some(n - 1)
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

/// Alphabet of a member target's first character (and of every 1-char
/// target): `a`–`z` then `A`–`Z`.
const MEMBER_FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
/// Alphabet of a member target's second character: letters then digits.
const MEMBER_SECOND: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Member (method/field) short names: raw index `n` → `a`…`z`, `A`…`Z`
/// (52 one-char names), then two-char names `<first><second>` over
/// [`MEMBER_FIRST`] × [`MEMBER_SECOND`]. All-lowercase pairs are skipped —
/// apps really do declare members called `id`, `io`, `eq`, `of`, and the
/// kotlin-shim has `to` — as are Java keywords. Like [`short_suffix`],
/// `raw` is one shared counter threaded through the whole cut.
pub fn member_suffix(raw: &mut usize) -> String {
    loop {
        let n = *raw;
        *raw += 1;
        let s = member_name(n);
        let all_lower = s.len() == 2 && s.bytes().all(|b| b.is_ascii_lowercase());
        if !all_lower && !is_java_reserved(&s) {
            return s;
        }
    }
}

fn member_name(n: usize) -> String {
    if n < MEMBER_FIRST.len() {
        return (MEMBER_FIRST[n] as char).to_string();
    }
    let n = n - MEMBER_FIRST.len();
    let first = MEMBER_FIRST[(n / MEMBER_SECOND.len()) % MEMBER_FIRST.len()];
    let second = MEMBER_SECOND[n % MEMBER_SECOND.len()];
    // Beyond two characters the sequence wraps; the corpus is ~1000 names
    // against 52 + 52·62 targets, so this is unreachable in practice.
    assert!(
        n < MEMBER_FIRST.len() * MEMBER_SECOND.len(),
        "member allocator exhausted"
    );
    String::from_utf8(vec![first, second]).unwrap()
}

/// Invert [`member_name`]: the raw index that produced `s`, or `None` if
/// `s` is not a member target shape. Used to resume the counter from a base
/// map's existing targets.
pub fn member_inverse(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    match b.len() {
        1 => MEMBER_FIRST.iter().position(|&c| c == b[0]),
        2 => {
            let first = MEMBER_FIRST.iter().position(|&c| c == b[0])?;
            let second = MEMBER_SECOND.iter().position(|&c| c == b[1])?;
            Some(MEMBER_FIRST.len() + first * MEMBER_SECOND.len() + second)
        }
        _ => None,
    }
}

/// Which synthetic package a shrunk name is allocated from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// `a/` — framework classes, un-shrunk by picodroid-core at dispatch.
    Framework,
    /// `b/` — `java/**` classes, un-shrunk by pico-jvm at the class-file
    /// boundary.
    Java,
}

impl Namespace {
    /// Every namespace, in allocation order.
    pub const ALL: [Namespace; 2] = [Namespace::Framework, Namespace::Java];

    /// The synthetic package prefix, including the trailing `/`.
    pub fn prefix(self) -> &'static str {
        match self {
            Namespace::Framework => "a/",
            Namespace::Java => "b/",
        }
    }
}

/// The namespace an original internal class name is allocated from.
/// `java/**` is the set pico-jvm interprets by name; everything else —
/// including `javax/**`, which the JVM does not know — is framework.
pub fn namespace_for(original: &str) -> Namespace {
    if original.starts_with("java/") {
        Namespace::Java
    } else {
        Namespace::Framework
    }
}

/// Compose a full shrunk internal name from a namespace and a suffix.
pub fn shrunk_name(ns: Namespace, suffix: &str) -> String {
    format!("{}{suffix}", ns.prefix())
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
    fn base26_inverse_round_trips() {
        for n in 0..2000usize {
            let s = base26(n);
            assert_eq!(
                base26_inverse(&s),
                Some(n),
                "round-trip failed for {n} → {s}"
            );
        }
        assert_eq!(base26_inverse(""), None);
        assert_eq!(base26_inverse("a"), None);
        assert_eq!(base26_inverse("A1"), None);
    }

    #[test]
    fn uppercase_never_hits_java_keywords() {
        // Keywords are all lowercase, but the reserved check is
        // case-insensitive to be extra safe against tooling quirks. Drive a
        // single shared counter (as cut_release does) across a range that
        // spans every 2-letter reserved keyword ("DO" at raw index 118,
        // "IF" at raw index 239) and confirm no output is ever reserved.
        let mut raw = 0usize;
        for _ in 0..300 {
            let s = short_suffix(&mut raw);
            assert!(!is_java_reserved(&s), "{s} collides with reserved");
        }
    }

    #[test]
    fn shared_counter_never_repeats_across_keyword_skips() {
        // Regression test for the "DO"/"DP" collision: a single shared `raw`
        // counter threaded across many calls must produce distinct outputs,
        // even where the underlying base26 sequence has to skip a reserved
        // keyword (previously this desynced the caller's per-call index from
        // the allocator's internal skip-ahead, so two different calls landed
        // on the same output string).
        let mut raw = 0usize;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..300 {
            let s = short_suffix(&mut raw);
            assert!(seen.insert(s.clone()), "duplicate suffix produced: {s}");
        }
    }

    #[test]
    fn member_targets_are_short_unique_and_never_lowercase_pairs() {
        let mut raw = 0usize;
        let mut seen = std::collections::HashSet::new();
        let mut first = Vec::new();
        // 52 + 52·62 − 26·26 lowercase pairs − keywords ≈ 2,600 usable targets.
        for _ in 0..2500 {
            let s = member_suffix(&mut raw);
            assert!(s.len() <= 2, "{s}");
            assert!(
                !(s.len() == 2 && s.bytes().all(|b| b.is_ascii_lowercase())),
                "{s} is an all-lowercase pair"
            );
            assert!(!is_java_reserved(&s));
            assert!(seen.insert(s.clone()), "duplicate member target {s}");
            if first.len() < 54 {
                first.push(s);
            }
        }
        assert_eq!(first[0], "a");
        assert_eq!(first[25], "z");
        assert_eq!(first[26], "A");
        assert_eq!(first[51], "Z");
        assert_eq!(first[52], "aA", "first pair skips a..az (all lowercase)");
        assert_eq!(first[53], "aB");
    }

    #[test]
    fn member_inverse_round_trips() {
        for n in 0..(52 + 52 * 62) {
            let s = member_name(n);
            assert_eq!(member_inverse(&s), Some(n), "{n} → {s}");
        }
        assert_eq!(member_inverse(""), None);
        assert_eq!(member_inverse("abc"), None);
        assert_eq!(member_inverse("1a"), None);
    }

    #[test]
    fn shrunk_full_name_contains_synthetic_package() {
        assert_eq!(shrunk_name(Namespace::Framework, "A"), "a/A");
        assert_eq!(shrunk_name(Namespace::Framework, "AB"), "a/AB");
        assert_eq!(shrunk_name(Namespace::Java, "A"), "b/A");
    }

    #[test]
    fn java_names_allocate_from_their_own_namespace() {
        assert_eq!(namespace_for("java/lang/Object"), Namespace::Java);
        assert_eq!(namespace_for("java/util/Map$Entry"), Namespace::Java);
        assert_eq!(namespace_for("picodroid/view/View"), Namespace::Framework);
        // javax/** is not interpreted by the JVM and is already committed
        // under a/ (javax/inject/Provider = a/EG); it must stay there.
        assert_eq!(namespace_for("javax/inject/Provider"), Namespace::Framework);
        assert_eq!(namespace_for("javaish/X"), Namespace::Framework);
    }
}
