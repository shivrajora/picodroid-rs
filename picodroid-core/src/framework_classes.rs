// SPDX-License-Identifier: GPL-3.0-only
//! Embedded picodroid framework class bytecode.
//!
//! Compiled and embedded by `build.rs` from `sdk/java/`. Framework classes
//! (`picodroid.*`) are part of the platform — like Android's boot classpath,
//! not the APK — so they are always present in firmware Flash.
//!
//! Defines: `pub static FRAMEWORK_CLASSES: &[&[u8]] = &[include_bytes!("..."), ...];`
//! plus `FRAMEWORK_EXCLUDED_CLASSES` (the board's opt-outs) and
//! `FRAMEWORK_CLASSES_DEBUG_STRIPPED` — true when `build.rs` embedded the
//! `:sdk:stripClasses` tree, i.e. the classes without `LineNumberTable` /
//! `SourceFile` / `StackMapTable`, which it does for every build with
//! `debug_assertions` off (docs/designs/flash-string-budget-2026-08.md §4).
//!
//! Kept in its own module (rather than `app.rs`) so it remains compiled under
//! `cfg(test)`. The dispatch-site regression test in
//! [`crate::dispatch_sites`] parses these bytes directly.

include!(concat!(env!("OUT_DIR"), "/framework_classes.rs"));

#[cfg(test)]
mod tests {
    use super::{FRAMEWORK_CLASSES, FRAMEWORK_CLASSES_DEBUG_STRIPPED};

    /// The debug attributes ship exactly when this build could read them.
    /// pico-jvm parses `LineNumberTable` only under `debug_assertions`
    /// (`jvm/src/class_file/parse.rs`) and never reads `SourceFile` or
    /// `StackMapTable`; `build.rs` picks the stripped or raw SDK tree with
    /// `CARGO_CFG_DEBUG_ASSERTIONS`. This pins that choice to the cfg the
    /// parser uses, and to the bytes actually embedded — so the gate can never
    /// drift to a proxy (`DEBUG`, `PROFILE`) that gets the firmware's
    /// `--config profile.dev.debug-assertions=false` wrong.
    #[test]
    fn debug_attributes_follow_debug_assertions() {
        assert!(
            !FRAMEWORK_CLASSES.is_empty(),
            "FRAMEWORK_CLASSES is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
        );
        assert_eq!(
            FRAMEWORK_CLASSES_DEBUG_STRIPPED,
            !cfg!(debug_assertions),
            "build.rs embedded the {} SDK tree into a build with debug_assertions {}",
            if FRAMEWORK_CLASSES_DEBUG_STRIPPED {
                "stripped"
            } else {
                "raw"
            },
            if cfg!(debug_assertions) { "on" } else { "off" },
        );
        // An attribute name lives in the constant pool as a CONSTANT_Utf8_info
        // entry: tag 1, u16 length, then the bytes — so the length-prefixed
        // form is an exact marker, immune to identifiers that merely contain
        // the word.
        let markers: [&[u8]; 3] = [
            b"\x01\x00\x0fLineNumberTable",
            b"\x01\x00\x0aSourceFile",
            b"\x01\x00\x0dStackMapTable",
        ];
        let carries = |bytes: &[u8]| {
            markers
                .iter()
                .any(|m| bytes.windows(m.len()).any(|w| w == *m))
        };
        let with_attrs = FRAMEWORK_CLASSES.iter().filter(|c| carries(c)).count();
        if FRAMEWORK_CLASSES_DEBUG_STRIPPED {
            assert_eq!(
                with_attrs, 0,
                "{with_attrs} embedded SDK classes still carry a debug attribute"
            );
        } else {
            assert!(
                with_attrs > 0,
                "no embedded SDK class carries a debug attribute, yet the raw javac tree was embedded"
            );
        }
    }
}
