// SPDX-License-Identifier: GPL-3.0-only
//! What the installer needs from whoever tracks the installed runs.
//!
//! The installer streams, verifies and commits one run; *where* that run goes
//! is the package directory's decision, because only the directory knows what
//! is already installed. This trait is that boundary. It is a generic
//! parameter of [`install`](super::install) for the same reason
//! [`PapkFlash`] is — one caller, no link surface, mockable — and it is not a
//! porting seam: the firmware's directory implements it once, for every
//! family.

use papk_format::flash_image::META_SIZE;

use super::PapkFlash;

/// Sectors a run for an image of `len` bytes occupies: the meta sector plus
/// the image rounded up to whole sectors.
pub fn run_sectors(len: usize) -> u32 {
    1 + len.div_ceil(META_SIZE) as u32
}

/// Where an install goes, and what to erase around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub first_sector: u32,
    /// `(first_sector, sectors)` to erase before streaming: the target run,
    /// widened over an old copy it replaces in place.
    pub erase_before: (u32, u32),
    /// An old copy to erase after the new run commits (an upgrade beside it).
    pub evict_after: Option<(u32, u32)>,
    pub flags: u32,
    pub seq: u32,
    /// Free space suffices but no gap does: compact, then plan again.
    pub compact_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    TooLarge,
    NoPackageName,
    SystemPackage,
    NoRoom {
        need: u32,
        largest_free: u32,
        total_free: u32,
        installed: u32,
        max: u32,
    },
}

/// The directory of installed runs, as the installer drives it.
pub trait PackageDirectory {
    /// Whether [`compact`](Self::compact) can ever free a gap. A single-app
    /// directory never asks for compaction; saying so as a constant lets the
    /// installer's retry path fold away on the builds that must not pay
    /// flash for it.
    const CAN_COMPACT: bool;

    /// Rebuild the directory from the region — after a commit or an erase.
    fn rescan(&mut self, flash: &impl PapkFlash);

    /// Decide where a PAPK of `papk_len` bytes for `package` goes.
    /// `max_apps` is the board's directory capacity.
    fn plan_install(
        &mut self,
        package: Option<&str>,
        papk_len: usize,
        max_apps: usize,
    ) -> Result<Plan, PlanError>;

    /// Slide every run down over the gaps below it.
    fn compact(&mut self, flash: &mut impl PapkFlash);

    /// `(largest free gap, total free)`, in sectors.
    fn free_space(&self) -> (u32, u32);

    /// Installed (non-system) apps.
    fn installed_count(&self) -> u32;
}
