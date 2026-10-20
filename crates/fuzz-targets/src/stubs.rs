// SPDX-License-Identifier: GPL-3.0-only
//! Stand-ins for the firmware behind the installer and the debug bridge:
//! a first-fit package directory over an in-memory region, which is also
//! the bridge's `PdbDevice`. Shared by the `pdb_stream` and `rescan`
//! targets; `install` keeps its own single-slot directory.

use pd_install::{run_sectors, PackageDirectory, PapkFlash, Plan, PlanError};
use pd_pdb::{PdbDevice, UninstallRefusal};
use pdb_protocol::input::InputEvent;

/// A run the directory knows: package, first sector, sectors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub package: String,
    pub first: u32,
    pub sectors: u32,
}

/// The framework-map version every stub firmware reports; the seed PAPKs
/// are built for it.
pub const FRAMEWORK_MAP_VERSION: &str = "0.40.0";

/// A directory that places each install after the last run and never
/// compacts, on a `MULTI`-app or single-app firmware.
pub struct FirstFit<const MULTI: bool> {
    pub installed: Vec<Run>,
    pending: Option<Run>,
    pub injected: Vec<InputEvent>,
    pub removed: Vec<String>,
    pub sectors: u32,
}

impl<const MULTI: bool> FirstFit<MULTI> {
    pub fn new(sectors: u32) -> Self {
        Self {
            installed: Vec::new(),
            pending: None,
            injected: Vec::new(),
            removed: Vec::new(),
            sectors,
        }
    }

    /// First sector after the last run.
    fn next_free(&self) -> u32 {
        self.installed
            .iter()
            .map(|r| r.first + r.sectors)
            .max()
            .unwrap_or(0)
    }
}

impl<const MULTI: bool> PackageDirectory for FirstFit<MULTI> {
    const CAN_COMPACT: bool = false;

    fn rescan(&mut self, _flash: &impl PapkFlash) {
        if let Some(run) = self.pending.take() {
            self.installed.push(run);
        }
    }
    fn mark_verified(&mut self, _flash: &mut impl PapkFlash) {}
    fn plan_install(
        &mut self,
        package: Option<&str>,
        papk_len: usize,
        max_apps: usize,
    ) -> Result<Plan, PlanError> {
        let need = run_sectors(papk_len);
        let first = self.next_free();
        if papk_len == 0 || first + need > self.sectors {
            return Err(PlanError::TooLarge);
        }
        if MULTI && self.installed.len() >= max_apps {
            return Err(PlanError::TooLarge);
        }
        self.pending = Some(Run {
            package: package.unwrap_or("").to_string(),
            first,
            sectors: need,
        });
        Ok(Plan {
            first_sector: first,
            erase_before: (first, need),
            evict_after: None,
            flags: 0,
            seq: self.installed.len() as u32 + 1,
            compact_first: false,
        })
    }
    fn compact(&mut self, _flash: &mut impl PapkFlash) {}
    fn free_space(&self) -> (u32, u32) {
        let free = self.sectors - self.next_free();
        (free, free)
    }
    fn installed_count(&self) -> u32 {
        self.installed.len() as u32
    }
}

impl<const MULTI: bool> PdbDevice for FirstFit<MULTI> {
    const MULTI_APP: bool = MULTI;
    const FRAMEWORK_MAP_VERSION: &'static str = FRAMEWORK_MAP_VERSION;

    fn board(&self) -> &str {
        "fuzz"
    }
    fn serial(&self, out: &mut [u8; pdb_protocol::greeting::SERIAL_MAX]) -> usize {
        out[..8].copy_from_slice(b"FUZZ0000");
        8
    }
    fn inject(&mut self, event: InputEvent) -> (u8, &'static [u8]) {
        self.injected.push(event);
        match event {
            // A single-app board here has no touch panel: the other arm.
            InputEvent::Tap { .. } | InputEvent::Swipe { .. } if !MULTI => {
                (pdb_protocol::STATUS_ERR, b"no touch panel")
            }
            _ => (pdb_protocol::STATUS_OK, b""),
        }
    }
    fn write_list(&self, out: &mut dyn core::fmt::Write) {
        for r in &self.installed {
            let _ = writeln!(
                out,
                "{}\t{}\t1\t1.0\t{}\t-\t{}",
                r.first,
                r.package,
                r.sectors * 4096,
                r.package
            );
        }
        let (largest, total) = self.free_space();
        let _ = writeln!(
            out,
            "free\t{}\t{}\t{}\t8",
            largest * 4096,
            total * 4096,
            self.installed.len()
        );
    }
    fn uninstall_target(&self, package: &str) -> Result<(u32, u32), UninstallRefusal> {
        if package == "picodroid.launcher" {
            return Err(UninstallRefusal::System);
        }
        self.installed
            .iter()
            .find(|r| r.package == package)
            .map(|r| (r.first, r.sectors))
            .ok_or(UninstallRefusal::NotInstalled)
    }
    fn package_removed(&mut self, package: &str) {
        self.installed.retain(|r| r.package != package);
        self.removed.push(package.to_string());
    }
}
