// SPDX-License-Identifier: GPL-3.0-only
//! What the bridge needs from the firmware beyond the installer's
//! [`PackageDirectory`]: the facts only that build knows, and the two
//! things a frame can do to the device (inject input, remove a package).

use pd_install::PackageDirectory;
use pdb_protocol::input::InputEvent;
use pdb_protocol::sysmon::MemDiag;

/// Why `CMD_UNINSTALL` names no run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UninstallRefusal {
    /// No installed app has that package name.
    NotInstalled,
    /// The name is a system app's, which lives in the firmware.
    System,
}

/// The firmware behind the bridge.
pub trait PdbDevice: PackageDirectory {
    /// Whether `CMD_LIST` and `CMD_UNINSTALL` are served. A single-app
    /// firmware says no and answers "unknown cmd", and the list and
    /// uninstall arms fold away in its image.
    const MULTI_APP: bool;

    /// The framework-map version the installer gates on, and the PING
    /// greeting reports.
    const FRAMEWORK_MAP_VERSION: &'static str;

    /// The board this firmware was built for, as the greeting reports it.
    fn board(&self) -> &str;

    /// The device serial into `out`; the number of bytes written.
    fn serial(&self, out: &mut [u8; pdb_protocol::greeting::SERIAL_MAX]) -> usize;

    /// `CMD_INPUT`, decoded: inject the event, block until it is delivered,
    /// and answer with a status and a message.
    fn inject(&mut self, event: InputEvent) -> (u8, &'static [u8]);

    /// The allocator snapshot appended to a sysmon reply; `None` where the
    /// build has no monitor.
    fn mem_diag(&self) -> Option<MemDiag> {
        None
    }

    /// `CMD_LIST`'s text rows (multi-app only).
    fn write_list(&self, out: &mut dyn core::fmt::Write);

    /// `CMD_UNINSTALL`: the `(first_sector, sectors)` run `package`
    /// occupies, or why not (multi-app only).
    fn uninstall_target(&self, package: &str) -> Result<(u32, u32), UninstallRefusal>;

    /// After a run is erased and before the reset: the package's data goes
    /// with it.
    fn package_removed(&mut self, package: &str);
}
