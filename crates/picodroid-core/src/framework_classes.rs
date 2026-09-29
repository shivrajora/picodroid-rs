// SPDX-License-Identifier: GPL-3.0-only
//! Embedded picodroid framework classes.
//!
//! Compiled and embedded by `build.rs` from `sdk/java/`. Framework classes
//! (`picodroid.*`) are part of the platform — like Android's boot classpath,
//! not the APK — so they are always present in firmware flash. They ship as
//! one class section (`FRAMEWORK_CLSS`, see `crates/class-link`): every
//! class with the link table built for it at firmware-build time, plus the
//! sorted class index — byte for byte the layout of a PAPK's CLASSES
//! section, loaded through the same reader ([`framework_section`]), so the
//! framework gets exactly what an app gets
//! (docs/designs/class-link-2026-09.md).
//!
//! `build.rs` defines `FRAMEWORK_CLSS`, `FRAMEWORK_CLASS_COUNT`,
//! `FRAMEWORK_EXCLUDED_CLASSES` (the board's opt-outs) and
//! `FRAMEWORK_CLASSES_LINE_NUMBERS` — true when it embedded the
//! `:sdk:stripClassesLines` tree, i.e. the stripped classes that still carry
//! `LineNumberTable` / `SourceFile`, which it does exactly when the
//! `line-numbers` feature is on (docs/designs/flash-string-budget-2026-08.md §4).
//!
//! Kept in its own module (rather than `app.rs`) so it remains compiled under
//! `cfg(test)`: the dispatch-site and API-contract tests read these classes.

/// The section is read in place as `u16` words and 8-byte index entries;
/// `include_bytes!` data alone is only byte-aligned.
#[repr(C, align(8))]
pub struct AlignedBytes<const N: usize>(pub [u8; N]);

include!(concat!(env!("OUT_DIR"), "/framework_classes.rs"));

/// The framework corpus as a class section. `parse` checks the header and
/// bounds only; `build.rs` validated every table when it built the section.
pub fn framework_section() -> class_link::ClassSection<'static> {
    class_link::ClassSection::parse(&FRAMEWORK_CLSS.0)
        .expect("the framework class section was validated at build time")
}

/// Every framework class's bytes, in section order — for tests that scan
/// the corpus; the runtime loads the section itself.
pub fn class_bytes() -> impl Iterator<Item = &'static [u8]> {
    let s = framework_section();
    (0..s.len()).map(move |i| s.class(i).expect("validated at build time").class)
}

/// Every framework class as a registered `ClassFile`, in section order.
pub fn class_files() -> alloc::vec::Vec<pico_jvm::class_file::ClassFile> {
    let s = framework_section();
    (0..s.len())
        .map(|i| {
            pico_jvm::class_file::ClassFile::linked(s.class(i).expect("validated at build time"))
                .expect("validated at build time")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        class_bytes, framework_section, FRAMEWORK_CLASSES_LINE_NUMBERS, FRAMEWORK_CLASS_COUNT,
    };

    /// The debug attributes ship exactly when this build can read them.
    /// pico-jvm reads `LineNumberTable` and `SourceFile` only under the
    /// `line-numbers` feature and never reads `StackMapTable`; `build.rs`
    /// picks the SDK tree with `CARGO_FEATURE_LINE_NUMBERS`. This pins that
    /// choice to the cfg the reader uses, and to the bytes actually embedded
    /// — so the gate can never drift to a proxy (`DEBUG`, `PROFILE`,
    /// `debug_assertions`) that gets the firmware's
    /// `--config profile.dev.debug-assertions=false` wrong.
    #[test]
    fn debug_attributes_follow_line_numbers_feature() {
        assert!(
            FRAMEWORK_CLASS_COUNT > 0,
            "FRAMEWORK_CLSS is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
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
            class_bytes()
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

    /// The embedded section validates on the host exactly as the packer's
    /// output does: every table derives from its class bytes, the index
    /// names every class once. What `build.rs` checked, checked again from
    /// the bytes rustc actually embedded.
    #[test]
    fn framework_section_validates_and_indexes_every_class() {
        let s = framework_section();
        assert_eq!(s.len(), FRAMEWORK_CLASS_COUNT);
        s.validate().expect("framework class section");
        for i in 0..s.len() {
            let c = s.class(i).unwrap();
            let name = c.name().unwrap();
            assert_eq!(
                s.find_class(name),
                Some(i),
                "{}",
                String::from_utf8_lossy(name)
            );
        }
    }
}
