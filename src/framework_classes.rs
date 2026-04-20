//! Embedded picodroid framework class bytecode.
//!
//! Compiled and embedded by `build.rs` from `sdk/java/`. Framework classes
//! (`picodroid.*`) are part of the platform — like Android's boot classpath,
//! not the APK — so they are always present in firmware Flash.
//!
//! Defines: `pub static FRAMEWORK_CLASSES: &[&[u8]] = &[include_bytes!("..."), ...];`
//!
//! Kept in its own module (rather than `app.rs`) so it remains compiled under
//! `cfg(test)`. The dispatch-site regression test in
//! [`crate::dispatch_sites`] parses these bytes directly.

include!(concat!(env!("OUT_DIR"), "/framework_classes.rs"));
