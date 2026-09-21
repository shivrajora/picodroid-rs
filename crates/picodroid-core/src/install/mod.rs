// SPDX-License-Identifier: GPL-3.0-only
//! PAPK install orchestration — the `pd-install` crate, bound to this
//! firmware's package directory and framework-map version.
//!
//! Everything a family touches ([`PapkFlash`], [`PapkRegionFlash`],
//! [`CoreCoordinator`], [`InstallTransport`]) is re-exported unchanged. The
//! four entry points are wrapped so they keep the signatures the debug bridge
//! and the simulator call: the crate's versions also take a
//! [`PackageDirectory`] and the framework-map version, and here those are
//! always [`crate::packages`] and the build's own.

pub use pd_install::*;

use crate::framework_map::FRAMEWORK_MAP_VERSION;

/// [`crate::packages`] as the installer's [`PackageDirectory`]. Zero-sized:
/// the directory is one process-wide static, reached through free functions.
pub struct CoreDirectory;

impl PackageDirectory for CoreDirectory {
    // A single-app firmware carries no compactor and never plans for one.
    const CAN_COMPACT: bool = cfg!(has_multi_app);

    fn rescan(&mut self, flash: &impl PapkFlash) {
        crate::packages::rescan_region(flash);
    }
    fn plan_install(
        &mut self,
        package: Option<&str>,
        papk_len: usize,
        max_apps: usize,
    ) -> Result<Plan, PlanError> {
        crate::packages::plan_install(package, papk_len, max_apps)
    }
    fn compact(&mut self, flash: &mut impl PapkFlash) {
        #[cfg(has_multi_app)]
        crate::packages::compact(flash);
        #[cfg(not(has_multi_app))]
        let _ = flash;
    }
    fn free_space(&self) -> (u32, u32) {
        crate::packages::free_space()
    }
    fn installed_count(&self) -> u32 {
        crate::packages::installed_count()
    }
}

/// Install, then reboot. See [`pd_install::run_install`].
pub fn run_install(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    papk_len: u32,
) {
    pd_install::run_install(
        transport,
        coordinator,
        flash,
        &mut CoreDirectory,
        FRAMEWORK_MAP_VERSION,
        papk_len,
    )
}

/// The install sequence, minus the reset. See [`pd_install::install`].
pub fn install(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    papk_len: u32,
) -> bool {
    pd_install::install(
        transport,
        coordinator,
        flash,
        &mut CoreDirectory,
        FRAMEWORK_MAP_VERSION,
        papk_len,
    )
}

/// Erase a run, then reboot. See [`pd_install::run_uninstall`].
pub fn run_uninstall(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    first_sector: u32,
    sectors: u32,
) {
    pd_install::run_uninstall(
        transport,
        coordinator,
        flash,
        &mut CoreDirectory,
        first_sector,
        sectors,
    )
}

/// Uninstall minus the reset. See [`pd_install::uninstall`].
pub fn uninstall(
    transport: &mut impl InstallTransport,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    first_sector: u32,
    sectors: u32,
) -> bool {
    pd_install::uninstall(
        transport,
        coordinator,
        flash,
        &mut CoreDirectory,
        first_sector,
        sectors,
    )
}
