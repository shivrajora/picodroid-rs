// SPDX-License-Identifier: GPL-3.0-only
//! class-shrink: Java class/method name shrinker for picodroid.
//!
//! Maps are **release-versioned** and **append-only**: each picodroid release
//! cuts an immutable map file `sdk/shrink-maps/v<semver>.toml` committed to
//! the repo. Symbols added between releases are **not** shrunk until the
//! next release folds them in. This keeps cross-version compatibility
//! predictable (old PAPKs run on new firmware as long as the firmware's
//! map version ≥ the PAPK's map version).
//!
//! Class names are rewritten here (`shrink`); member names are allocated
//! here (`shrink::cut_release_members`, schema-2 `[[member]]` rows) and
//! rewritten by the Gradle-side ASM `ShrinkMembersTask`. Since the first
//! member map (v0.16.0) an older shrunk PAPK no longer runs on newer
//! firmware — see `compat::MEMBER_SHRINK_FLOOR`.

pub mod classfile;
pub mod descriptor;
pub mod keep;
pub mod mapping;
pub mod rename;
pub mod shrink;
pub mod version;
