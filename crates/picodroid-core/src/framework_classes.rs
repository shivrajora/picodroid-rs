// SPDX-License-Identifier: GPL-3.0-only
//! Embedded picodroid framework class bytecode.
//!
//! Compiled and embedded by `build.rs` from `sdk/java/`. Framework classes
//! (`picodroid.*`) are part of the platform — like Android's boot classpath,
//! not the APK — so they are always present in firmware Flash.
//!
//! Defines: `pub static FRAMEWORK_CLASSES: &[&[u8]] = &[include_bytes!("..."), ...];`
//! plus `FRAMEWORK_EXCLUDED_CLASSES` (the board's opt-outs) and
//! `FRAMEWORK_CLASSES_LINE_NUMBERS` — true when `build.rs` embedded the
//! `:sdk:stripClassesLines` tree, i.e. the stripped classes that still carry
//! `LineNumberTable` / `SourceFile`, which it does exactly when the
//! `line-numbers` feature is on (docs/designs/flash-string-budget-2026-08.md §4).
//!
//! Kept in its own module (rather than `app.rs`) so it remains compiled under
//! `cfg(test)`. The dispatch-site regression test in
//! [`crate::dispatch_sites`] parses these bytes directly.

include!(concat!(env!("OUT_DIR"), "/framework_classes.rs"));

#[cfg(test)]
mod tests {
    use super::{FRAMEWORK_CLASSES, FRAMEWORK_CLASSES_LINE_NUMBERS};

    /// The debug attributes ship exactly when this build can read them.
    /// pico-jvm parses `LineNumberTable` and `SourceFile` only under the
    /// `line-numbers` feature (`jvm/src/class_file/parse.rs`) and never reads
    /// `StackMapTable`; `build.rs` picks the SDK tree with
    /// `CARGO_FEATURE_LINE_NUMBERS`. This pins that choice to the cfg the
    /// parser uses, and to the bytes actually embedded — so the gate can never
    /// drift to a proxy (`DEBUG`, `PROFILE`, `debug_assertions`) that gets the
    /// firmware's `--config profile.dev.debug-assertions=false` wrong.
    #[test]
    fn debug_attributes_follow_line_numbers_feature() {
        assert!(
            !FRAMEWORK_CLASSES.is_empty(),
            "FRAMEWORK_CLASSES is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
        );
        assert_eq!(
            FRAMEWORK_CLASSES_LINE_NUMBERS,
            cfg!(feature = "line-numbers"),
            "build.rs embedded the {} SDK tree into a build with the line-numbers feature {}",
            if FRAMEWORK_CLASSES_LINE_NUMBERS {
                "with-lines"
            } else {
                "fully stripped"
            },
            if cfg!(feature = "line-numbers") {
                "on"
            } else {
                "off"
            },
        );
        // An attribute name lives in the constant pool as a CONSTANT_Utf8_info
        // entry: tag 1, u16 length, then the bytes — so the length-prefixed
        // form is an exact marker, immune to identifiers that merely contain
        // the word.
        let count = |marker: &[u8]| {
            FRAMEWORK_CLASSES
                .iter()
                .filter(|c| c.windows(marker.len()).any(|w| w == marker))
                .count()
        };
        let lnt = count(b"\x01\x00\x0fLineNumberTable");
        let source = count(b"\x01\x00\x0aSourceFile");
        let frames = count(b"\x01\x00\x0dStackMapTable");
        assert_eq!(
            frames, 0,
            "{frames} embedded SDK classes still carry a StackMapTable, which no build reads"
        );
        if FRAMEWORK_CLASSES_LINE_NUMBERS {
            assert!(
                lnt > 0 && source > 0,
                "line-numbers build, yet only {lnt} classes carry a LineNumberTable and {source} a SourceFile"
            );
        } else {
            assert_eq!(
                lnt + source,
                0,
                "{lnt} classes carry a LineNumberTable and {source} a SourceFile in a build that cannot read them"
            );
        }
    }
}
