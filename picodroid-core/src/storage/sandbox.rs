// SPDX-License-Identifier: GPL-3.0-only
//! The storage sandbox: every path an app names is confined to its own
//! directory (docs/designs/multi-app-2026-09.md D10, §8 P1).
//!
//! An app sees a root of its own. Its `/x/y` resolves to
//! `/data/<package>/x/y`, where `<package>` is the manifest name of the app
//! `run_app` is executing (`packages::running()`); a run that carries no
//! package name — a PAPK from before the manifest gained one, single-app
//! boards only — keeps the volume's root, exactly as every app did before
//! the sandbox. Empty and `.` segments drop out and `..` is refused: the
//! natives are the only seam Java can reach storage through, so a refused
//! path is one the app can never name. Both ends of a rename are mapped, so
//! no file can leave the directory.
//!
//! The mapped path is built in a caller-supplied [`BUF`]-byte buffer:
//! `/data/` + a package name of at most [`PACKAGE_MAX`] bytes + the app's
//! path, which caps an app path at [`APP_PATH_MAX`] bytes whatever its
//! package name's length, so the limit an app sees is the same everywhere.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::packages;

/// Where every package's directory lives.
pub const DATA_ROOT: &str = "/data";
/// Longest package name the directory keeps — `packages::Name`'s capacity.
pub const PACKAGE_MAX: usize = 64;
/// Size of the buffer a mapped path is built in.
pub const BUF: usize = 256;
/// `/data/`, a full-length package name, and the `/` before the path.
const PREFIX_MAX: usize = DATA_ROOT.len() + 1 + PACKAGE_MAX + 1;
/// Longest app-visible path: what always fits behind the longest prefix.
pub const APP_PATH_MAX: usize = BUF - PREFIX_MAX;

/// Why a path was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// A `..` segment: the path tries to climb out of the directory.
    Climb,
    /// Longer than [`APP_PATH_MAX`] bytes.
    TooLong,
}

fn push(buf: &mut [u8; BUF], len: &mut usize, bytes: &[u8]) -> Result<(), Rejected> {
    let end = *len + bytes.len();
    if end > BUF {
        return Err(Rejected::TooLong);
    }
    buf[*len..end].copy_from_slice(bytes);
    *len = end;
    Ok(())
}

/// The volume path for the app-visible `path`, built in `buf`: under the
/// running package's directory, or `path` itself when no package runs.
pub fn resolve<'b>(
    package: Option<&str>,
    path: &str,
    buf: &'b mut [u8; BUF],
) -> Result<&'b str, Rejected> {
    let mut len = 0usize;
    match package {
        None => push(buf, &mut len, path.as_bytes())?,
        Some(package) => {
            if path.len() > APP_PATH_MAX {
                return Err(Rejected::TooLong);
            }
            push(buf, &mut len, DATA_ROOT.as_bytes())?;
            push(buf, &mut len, b"/")?;
            push(buf, &mut len, package.as_bytes())?;
            for segment in path.split('/') {
                match segment {
                    "" | "." => continue,
                    ".." => return Err(Rejected::Climb),
                    _ => {
                        push(buf, &mut len, b"/")?;
                        push(buf, &mut len, segment.as_bytes())?;
                    }
                }
            }
        }
    }
    // SAFETY: every byte pushed came from a `&str` — whole strings, or
    // segments split at `/`, an ASCII byte no multi-byte sequence contains —
    // so the buffer holds valid UTF-8 up to `len`.
    Ok(unsafe { core::str::from_utf8_unchecked(&buf[..len]) })
}

/// Create `/data/<package>` ahead of an app's first creating operation of a
/// run. `packages::run_generation()` moves when `run_app` records the next
/// package and the flag follows it, so each run pays two `mkdir`s once; two
/// Java threads racing here both `mkdir`, which is idempotent.
pub fn ensure_package_dir() {
    static ENSURED_FOR: AtomicU32 = AtomicU32::new(u32::MAX);
    let generation = packages::run_generation();
    if ENSURED_FOR.load(Ordering::Acquire) == generation {
        return;
    }
    if let Some(package) = packages::running() {
        let mut buf = [0u8; BUF];
        if let Ok(root) = resolve(Some(package), "", &mut buf) {
            let _ = crate::hal::fs::mkdir(DATA_ROOT);
            let _ = crate::hal::fs::mkdir(root);
        }
    }
    ENSURED_FOR.store(generation, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::{String, ToString};

    fn map(package: Option<&str>, path: &str) -> Result<String, Rejected> {
        let mut buf = [0u8; BUF];
        resolve(package, path, &mut buf).map(|s| s.to_string())
    }

    #[test]
    fn paths_land_under_the_package_directory() {
        assert_eq!(
            map(Some("com.a"), "/prefs/x").unwrap(),
            "/data/com.a/prefs/x"
        );
        assert_eq!(
            map(Some("com.a"), "prefs/x").unwrap(),
            "/data/com.a/prefs/x"
        );
        assert_eq!(map(Some("com.a"), "/").unwrap(), "/data/com.a");
        assert_eq!(map(Some("com.a"), "").unwrap(), "/data/com.a");
        assert_eq!(map(Some("com.a"), "//a//b/").unwrap(), "/data/com.a/a/b");
        assert_eq!(map(Some("com.a"), "/./a/./b").unwrap(), "/data/com.a/a/b");
        // Naming another package's directory lands inside this one's.
        assert_eq!(
            map(Some("com.a"), "/data/com.b/x").unwrap(),
            "/data/com.a/data/com.b/x"
        );
    }

    #[test]
    fn climbing_out_is_refused() {
        assert_eq!(map(Some("com.a"), "../x"), Err(Rejected::Climb));
        assert_eq!(map(Some("com.a"), "/a/../../x"), Err(Rejected::Climb));
        assert_eq!(map(Some("com.a"), "/.."), Err(Rejected::Climb));
        // `...` is an ordinary name.
        assert_eq!(map(Some("com.a"), "/...").unwrap(), "/data/com.a/...");
    }

    #[test]
    fn the_path_cap_is_the_same_for_every_package_name() {
        let ok = format!("/{}", "a".repeat(APP_PATH_MAX - 1));
        assert!(map(Some("a"), &ok).is_ok());
        let long = format!("/{}", "a".repeat(APP_PATH_MAX));
        assert_eq!(map(Some("a"), &long), Err(Rejected::TooLong));
        // The longest package name plus the longest path still fits, with
        // or without the leading slash the mapping supplies.
        let package = "p".repeat(PACKAGE_MAX);
        assert!(map(Some(&package), &ok).is_ok());
        let bare = "b".repeat(APP_PATH_MAX);
        assert!(map(Some(&package), &bare).is_ok());
    }

    #[test]
    fn no_package_means_the_volume_root_untouched() {
        assert_eq!(map(None, "/prefs/../x").unwrap(), "/prefs/../x");
        assert_eq!(map(None, "").unwrap(), "");
    }
}
