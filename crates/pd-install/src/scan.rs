// SPDX-License-Identifier: GPL-3.0-only
//! The boot-time walk over an app region: which sectors hold a run, and
//! what kind. Pure over a byte slice, so the firmware's `rescan` is a thin
//! wrapper that feeds its directory, and the host can fuzz the walk over
//! torn, duplicated and garbled regions (`docs/fuzzing.md`, target
//! `rescan`).
//!
//! Sector by sector: a sector whose header parses starts a run of
//! `run_sectors(len)` sectors. Without its commit page it is [`Found::Stale`]
//! (an install or relocation cut short: not an app, not free space either
//! until the firmware's cleanup erases it). Committed, its image is checked
//! in full — or only by its receipt when the verified page says an earlier
//! scan already did — and must name a package; one that does is
//! [`Found::App`], one whose class names collide with the firmware's
//! ([`scan_region`]'s `classes_ok`) is [`Found::Collides`]. A header over
//! anything else is [`Found::Garbage`], and the walk steps one sector, not
//! the claimed span, so a real run that starts inside the claim is still
//! found.

use papk_format::flash_image::{
    is_committed, is_verified, parse_header, BootMeta, META_SIZE, VERIFIED_READ_LEN,
};
use papk_format::{validate_embedded, validate_structure, Papk};

use crate::run_sectors;

/// What one step of the walk found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found<'r> {
    /// A committed run whose image passed the check (deep, or by receipt)
    /// and names a package.
    App {
        first_sector: u32,
        sectors: u32,
        meta: BootMeta,
        image: &'r [u8],
        /// The verified page was present, so only the receipt was checked.
        verified: bool,
    },
    /// A header with no commit page. Occupies its span.
    Stale { first_sector: u32, sectors: u32 },
    /// Committed and valid, but a class hashes like a framework class.
    /// Occupies its span.
    Collides {
        first_sector: u32,
        sectors: u32,
        image: &'r [u8],
    },
    /// A header over bytes that are not a PAPK with a package name: the
    /// walk steps one sector.
    Garbage { first_sector: u32 },
}

impl Found<'_> {
    /// The sectors this finding occupies.
    pub fn span(&self) -> (u32, u32) {
        match *self {
            Found::App {
                first_sector,
                sectors,
                ..
            }
            | Found::Stale {
                first_sector,
                sectors,
            }
            | Found::Collides {
                first_sector,
                sectors,
                ..
            } => (first_sector, sectors),
            Found::Garbage { first_sector } => (first_sector, 1),
        }
    }
}

/// The walk, as an iterator.
pub struct RegionScan<'r, F> {
    region: &'r [u8],
    sectors: usize,
    next: usize,
    classes_ok: F,
}

/// Walk `region` sector by sector. `classes_ok(image)` is the firmware's
/// class-hash disjointness check: the walk cannot know the framework set.
pub fn scan_region<F: FnMut(&[u8]) -> bool>(region: &[u8], classes_ok: F) -> RegionScan<'_, F> {
    RegionScan {
        region,
        sectors: region.len() / META_SIZE,
        next: 0,
        classes_ok,
    }
}

impl<'r, F: FnMut(&[u8]) -> bool> Iterator for RegionScan<'r, F> {
    type Item = Found<'r>;

    fn next(&mut self) -> Option<Found<'r>> {
        while self.next < self.sectors {
            let s = self.next;
            let header = &self.region[s * META_SIZE..][..VERIFIED_READ_LEN];
            // The largest image the sectors after this one can hold.
            let max_len = (self.sectors - s - 1) * META_SIZE;
            let Some(meta) = parse_header(header, max_len) else {
                self.next += 1;
                continue;
            };
            let span = run_sectors(meta.len as usize) as usize;
            if !is_committed(header) {
                self.next += span;
                return Some(Found::Stale {
                    first_sector: s as u32,
                    sectors: span as u32,
                });
            }
            // `parse_header` bounded `meta.len` by `max_len`, so the image
            // lies inside the region.
            let image = &self.region[(s + 1) * META_SIZE..][..meta.len as usize];
            // A run with its receipt was deep-checked by an earlier scan (or
            // by the build script that baked it): only its bounds are read.
            // One without is checked in full, here, before anything trusts it.
            let verified = is_verified(header);
            let valid = if verified {
                validate_embedded(image).is_ok()
            } else {
                validate_structure(image).is_ok()
            };
            if valid && !(self.classes_ok)(image) {
                self.next += span;
                return Some(Found::Collides {
                    first_sector: s as u32,
                    sectors: span as u32,
                    image,
                });
            }
            let named = valid
                && Papk::parse(image)
                    .ok()
                    .and_then(|p| p.package_name())
                    .is_some();
            if named {
                self.next += span;
                return Some(Found::App {
                    first_sector: s as u32,
                    sectors: span as u32,
                    meta,
                    image,
                    verified,
                });
            }
            self.next += 1;
            return Some(Found::Garbage {
                first_sector: s as u32,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use papk_format::flash_image::{build_commit_page, build_header_page, COMMIT_OFFSET};
    use papk_format::{EntryPoint, ManifestSpec, PapkBuilder};

    fn papk(package: &str) -> Vec<u8> {
        PapkBuilder::new(ManifestSpec {
            entry: EntryPoint::MainClass("t/Main"),
            package_name: package,
            version: "1.0",
            framework_map_version: "0.40.0",
            version_code: Some(1),
            label: None,
            icon: None,
            design_size: None,
            requires_features: None,
        })
        .build()
        .unwrap()
    }

    fn bake(region: &mut [u8], sector: usize, image: &[u8], commit: bool) {
        let at = sector * META_SIZE;
        region[at..at + 256].copy_from_slice(&build_header_page(image.len() as u32, 0, 1));
        if commit {
            region[at + COMMIT_OFFSET..at + COMMIT_OFFSET + 256]
                .copy_from_slice(&build_commit_page());
        }
        region[at + META_SIZE..at + META_SIZE + image.len()].copy_from_slice(image);
    }

    fn found(region: &[u8]) -> Vec<Found<'_>> {
        scan_region(region, |_| true).collect()
    }

    #[test]
    fn an_erased_region_holds_nothing() {
        let region = alloc::vec![0xFFu8; 4 * META_SIZE];
        assert!(found(&region).is_empty());
    }

    #[test]
    fn a_run_is_found_and_a_commit_less_one_is_stale() {
        let mut region = alloc::vec![0xFFu8; 6 * META_SIZE];
        let a = papk("com.a");
        bake(&mut region, 0, &a, true);
        bake(&mut region, 3, &papk("com.b"), false);
        let f = found(&region);
        assert_eq!(f.len(), 2);
        assert!(matches!(
            f[0],
            Found::App {
                first_sector: 0,
                sectors: 2,
                image,
                verified: false,
                ..
            } if image == &a[..]
        ));
        assert_eq!(
            f[1],
            Found::Stale {
                first_sector: 3,
                sectors: 2
            }
        );
    }

    /// A header whose length runs past the region is not a run; a run
    /// ending exactly at the last sector is.
    #[test]
    fn the_length_is_bounded_by_the_sectors_left() {
        let mut region = alloc::vec![0xFFu8; 3 * META_SIZE];
        let a = papk("com.a");
        assert!(a.len() <= META_SIZE);
        // Sector 1 + one image sector = the last sector exactly.
        bake(&mut region, 1, &a, true);
        assert!(matches!(
            found(&region)[..],
            [Found::App {
                first_sector: 1,
                sectors: 2,
                ..
            }]
        ));
        // From sector 2 the image would need sector 3, which does not exist.
        let mut region = alloc::vec![0xFFu8; 3 * META_SIZE];
        let at = 2 * META_SIZE;
        region[at..at + 256].copy_from_slice(&build_header_page(a.len() as u32, 0, 1));
        assert!(found(&region).is_empty());
    }

    /// A header over garbage steps one sector, so a real run that starts
    /// inside the claimed span is still found; a collision occupies its span.
    #[test]
    fn garbage_steps_one_sector_and_a_collision_its_span() {
        let mut region = alloc::vec![0xFFu8; 6 * META_SIZE];
        let a = papk("com.a");
        // A committed header at 0 claiming 3 sectors over nothing.
        let at = 0;
        region[at..at + 256].copy_from_slice(&build_header_page(2 * META_SIZE as u32, 0, 1));
        region[at + COMMIT_OFFSET..at + COMMIT_OFFSET + 256].copy_from_slice(&build_commit_page());
        bake(&mut region, 1, &a, true);
        let f = found(&region);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0], Found::Garbage { first_sector: 0 });
        assert!(matches!(
            f[1],
            Found::App {
                first_sector: 1,
                ..
            }
        ));

        let f: Vec<_> = scan_region(&region, |_| false).collect();
        assert!(matches!(
            f[1],
            Found::Collides {
                first_sector: 1,
                sectors: 2,
                ..
            }
        ));
        assert_eq!(f[1].span(), (1, 2));
    }
}
