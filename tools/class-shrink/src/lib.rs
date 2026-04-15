//! class-shrink: Java class/method name shrinker for picodroid.
//!
//! Maps are **release-versioned** and **append-only**: each picodroid release
//! cuts an immutable map file `sdk/shrink-maps/v<semver>.toml` committed to
//! the repo. Symbols added between releases are **not** shrunk until the
//! next release folds them in. This keeps cross-version compatibility
//! predictable (old PAPKs run on new firmware as long as the firmware's
//! map version ≥ the PAPK's map version).
//!
//! M1 exposes only the version-resolution machinery. Actual bytecode
//! rewriting lands in M3.

pub mod classfile;
pub mod descriptor;
pub mod keep;
pub mod mapping;
pub mod rename;
pub mod shrink;
pub mod version;
