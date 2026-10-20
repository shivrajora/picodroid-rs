// SPDX-License-Identifier: GPL-3.0-only
//! The device half of PDBP — the picodroid debug bridge — bound to this
//! firmware.
//!
//! The protocol loop is the `pd-pdb` crate (resync, dispatch, CRC, the
//! answers), generic over the byte pipe, the installer's flash and parking
//! seams, the task-statistics source and a [`pd_pdb::PdbDevice`]. This
//! module is that device: the board name, the serial, the framework-map
//! version, input injection ([`input`]), the package list and uninstall
//! targets ([`packages`]), all on [`crate::install::CoreDirectory`], which
//! is already the installer's view of [`crate::packages`]. The public items
//! a family or the simulator names keep their paths here.

pub mod input;
#[cfg(has_multi_app)]
pub mod packages;
pub mod usb_cdc;

pub use pd_pdb::{send_response, PdbTransport, SysmonSample, SysmonSource, TaskSample, MAX_TASKS};

use pd_pdb::{PdbDevice, UninstallRefusal};
use pdb_protocol::input::InputEvent;

use crate::install::{CoreCoordinator, CoreDirectory, PapkFlash};

impl PdbDevice for CoreDirectory {
    const MULTI_APP: bool = cfg!(has_multi_app);
    const FRAMEWORK_MAP_VERSION: &'static str = crate::framework_map::FRAMEWORK_MAP_VERSION;

    fn board(&self) -> &str {
        crate::board_cfg::build_info::BOARD
    }

    fn serial(&self, out: &mut [u8; pdb_protocol::greeting::SERIAL_MAX]) -> usize {
        let serial = crate::identity::serial();
        let bytes = serial.as_bytes();
        out[..bytes.len()].copy_from_slice(bytes);
        bytes.len()
    }

    fn inject(&mut self, event: InputEvent) -> (u8, &'static [u8]) {
        input::inject(event)
    }

    /// Device-only: the snapshot is published by the on-device monitor
    /// (`mem_diag::published_snapshot` is gated the same way); the
    /// simulator's bridge serves the plain sample.
    fn mem_diag(&self) -> Option<pdb_protocol::sysmon::MemDiag> {
        #[cfg(all(feature = "mem-diag", not(feature = "sim")))]
        {
            let (live, floor, alloc_total, largest_free) = crate::mem_diag::published_snapshot();
            Some(pdb_protocol::sysmon::MemDiag {
                live,
                floor,
                alloc_total,
                largest_free,
            })
        }
        #[cfg(not(all(feature = "mem-diag", not(feature = "sim"))))]
        None
    }

    fn write_list(&self, out: &mut dyn core::fmt::Write) {
        #[cfg(has_multi_app)]
        packages::render_list(out);
        #[cfg(not(has_multi_app))]
        let _ = out;
    }

    fn uninstall_target(&self, package: &str) -> Result<(u32, u32), UninstallRefusal> {
        #[cfg(has_multi_app)]
        {
            use crate::packages::Kind;
            match crate::packages::find(package) {
                None => Err(UninstallRefusal::NotInstalled),
                Some(e) if e.kind == Kind::System => Err(UninstallRefusal::System),
                Some(e) => Ok((e.first_sector as u32, e.sectors as u32)),
            }
        }
        #[cfg(not(has_multi_app))]
        {
            let _ = package;
            Err(UninstallRefusal::NotInstalled)
        }
    }

    /// The package's data goes with its run (D10).
    fn package_removed(&mut self, package: &str) {
        #[cfg(has_multi_app)]
        crate::storage::wipe_package(package);
        #[cfg(not(has_multi_app))]
        let _ = package;
    }
}

/// Run the debug bridge. Never returns.
///
/// The family supplies the four things this firmware cannot know: how bytes
/// reach the host, how to park the JVM core for a flash write, where task
/// statistics come from, and what the PAPK slot is.
pub fn run_pdb_task(
    transport: impl PdbTransport,
    coordinator: impl CoreCoordinator,
    sysmon_source: impl SysmonSource,
    flash: impl PapkFlash,
) -> ! {
    pd_pdb::run_pdb_task(transport, coordinator, sysmon_source, flash, CoreDirectory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pd_pdb::vec_pipe::VecPipe;
    use pd_pdb::{serve_one, BridgeState, Served};
    use pdb_protocol::{crc32_frame, greeting, CMD_PING, FRAME_MAGIC, STATUS_OK};

    struct MockFlash;
    /// What the mock maps: two erased sectors.
    static MOCK_REGION: [u8; 2 * 4096] = [0xFF; 2 * 4096];
    // SAFETY: `mapped_base` is a static of `region_len` bytes, readable for
    // the life of the program and never written; the writes are no-ops.
    unsafe impl PapkFlash for MockFlash {
        fn region_len(&self) -> usize {
            MOCK_REGION.len()
        }
        fn max_installed_apps(&self) -> usize {
            1
        }
        fn mapped_base(&self) -> *const u8 {
            MOCK_REGION.as_ptr()
        }
        fn select_run(&mut self, _first_sector: u32) {}
        unsafe fn erase_run(&mut self, _first_sector: u32, _sectors: u32) {}
        unsafe fn write_page(&mut self, _page_index: u32, _page: &[u8; 256]) -> bool {
            true
        }
        unsafe fn write_meta_header(&mut self, _len: u32, _flags: u32, _seq: u32) {}
        unsafe fn write_meta_commit(&mut self) {}
        unsafe fn write_meta_verified(&mut self) {}
        unsafe fn commit_metadata(&mut self, _len: u32, _flags: u32, _seq: u32) {}
        unsafe fn copy_page(&mut self, _src: u32, _dst: u32, _page: u32) {}
        fn trigger_reset(&mut self) -> ! {
            unreachable!("no reset in a ping test")
        }
    }

    struct NoSysmon;
    impl SysmonSource for NoSysmon {
        fn sample(&mut self, _out: &mut SysmonSample) -> bool {
            false
        }
    }

    /// The greeting bytes themselves are pinned in `pdb_protocol::greeting`;
    /// what this build must still prove is the wiring — that a PING through
    /// the shared loop answers with *this* firmware's framework-map-version,
    /// the flash slot's real capacity and the published serial, in a frame
    /// the host-side parser accepts.
    #[test]
    fn greeting_layout_is_stable() {
        let _g = crate::packages::test_support::lock();
        crate::identity::init();
        let mut rx = FRAME_MAGIC.to_vec();
        rx.push(CMD_PING);
        rx.extend_from_slice(&0u32.to_le_bytes());
        rx.extend_from_slice(&crc32_frame(CMD_PING, 0, &[]).to_le_bytes());
        let mut pipe = VecPipe::new(rx);
        let mut flash = MockFlash;
        let served = serve_one(
            &mut pipe,
            &mut crate::install::mem_region::NoCoordinator,
            &mut NoSysmon,
            &mut flash,
            &mut CoreDirectory,
            &mut BridgeState::new(),
        );
        assert_eq!(
            served,
            Served::Answered {
                cmd: CMD_PING,
                reset: false
            }
        );

        assert_eq!(&pipe.tx[..4], FRAME_MAGIC);
        assert_eq!(pipe.tx[4], STATUS_OK);
        let len = u32::from_le_bytes(pipe.tx[5..9].try_into().unwrap()) as usize;
        let payload = &pipe.tx[9..9 + len];

        let g = greeting::Greeting::parse(payload).unwrap();
        assert_eq!(g.version, greeting::PROTOCOL_VERSION);
        assert_eq!(g.max_papk as usize, MockFlash.max_data_size());
        assert_eq!(
            g.framework_map_version,
            crate::framework_map::FRAMEWORK_MAP_VERSION
        );
        assert_eq!(g.board, crate::board_cfg::build_info::BOARD);
        assert_eq!(g.serial, crate::identity::serial().as_str());
        assert_eq!(g.serial, "TEST0123456789AB");
        assert!(!g.is_legacy());
        assert_eq!(g.apps.is_some(), cfg!(has_multi_app));
    }
}
