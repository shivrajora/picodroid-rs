// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_LIST` and `CMD_UNINSTALL` — the package directory over the bridge
//! (docs/designs/multi-app-2026-09.md D8). Multi-app devices only; a
//! single-app device never reaches these (`serve_one` answers `unknown cmd`).
//!
//! The list is text, one tab-separated row per installed app, one per
//! system app, a `free` footer and a `running` line, so `tools/pdb` renders
//! it without a second wire format and a person can read the raw frame in
//! a serial capture. The rows are the device's ([`PdbDevice::write_list`]);
//! this module frames them.

use pd_install::{CoreCoordinator, PapkFlash};
use pdb_protocol::{
    crc32_frame, CMD_LIST, CMD_UNINSTALL, MAX_PACKAGE_NAME, STATUS_CRC_FAIL, STATUS_ERR,
    STATUS_NOT_FOUND, STATUS_OK,
};

use crate::framing::{send_response, Framed, TextBuf};
use crate::{PdbDevice, PdbTransport, UninstallRefusal};

/// Room for eight rows of a long package name and label, the system rows,
/// the footer and the running line.
pub const LIST_BUF: usize = 1280;

/// Handle `CMD_LIST`.
pub fn handle_list(transport: &mut impl PdbTransport, device: &impl PdbDevice, len: u32) {
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_LIST, len, &[]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    let mut text = TextBuf::<LIST_BUF>::new();
    device.write_list(&mut text);
    send_response(transport, STATUS_OK, text.as_bytes());
}

/// Handle `CMD_UNINSTALL`: payload is the package name. Parks the JVM and
/// erases the whole run — the same choreography as an install. `true` when
/// the run is gone and the caller must reset.
pub fn handle_uninstall<T: PdbTransport, D: PdbDevice>(
    transport: &mut T,
    coordinator: &mut impl CoreCoordinator,
    flash: &mut impl PapkFlash,
    device: &mut D,
    len: u32,
) -> bool {
    // The payload + trailing CRC. `serve_one` refused any `len` above
    // MAX_PACKAGE_NAME before calling here; the `min` keeps this function
    // safe on its own.
    let mut name = [0u8; MAX_PACKAGE_NAME];
    let n = (len as usize).min(MAX_PACKAGE_NAME);
    for b in name.iter_mut().take(n) {
        *b = transport.read_byte();
    }
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_UNINSTALL, len, &name[..n]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return false;
    }
    let Ok(package) = core::str::from_utf8(&name[..n]) else {
        send_response(transport, STATUS_ERR, b"package name is not UTF-8");
        return false;
    };
    let (first, sectors) = match device.uninstall_target(package) {
        Err(UninstallRefusal::NotInstalled) => {
            send_response(transport, STATUS_NOT_FOUND, b"not installed");
            return false;
        }
        Err(UninstallRefusal::System) => {
            send_response(transport, STATUS_ERR, b"system package");
            return false;
        }
        Ok(run) => run,
    };
    let mut framed = Framed::<T, D>::new(transport);
    if !pd_install::uninstall(&mut framed, coordinator, flash, device, first, sectors) {
        return false;
    }
    // The package's data goes with it (D10); then the reset the caller
    // makes.
    device.package_removed(package);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_device::Stub;
    use crate::vec_pipe::VecPipe;
    use alloc::string::String;
    use alloc::vec::Vec;
    use pd_install::mem_region::{MemRegion, NoCoordinator, Op};

    fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = payload.to_vec();
        v.extend_from_slice(&crc32_frame(cmd, payload.len() as u32, payload).to_le_bytes());
        v
    }

    fn payload_of(tx: &[u8]) -> (u8, String) {
        let len = u32::from_le_bytes(tx[5..9].try_into().unwrap()) as usize;
        (tx[4], String::from_utf8_lossy(&tx[9..9 + len]).into_owned())
    }

    #[test]
    fn list_frames_the_devices_rows() {
        let mut device = Stub::<true>::new(8);
        device.installed.push((String::from("com.a"), 0, 2));
        let mut pipe = VecPipe::new(frame(CMD_LIST, b""));
        handle_list(&mut pipe, &device, 0);
        let (status, text) = payload_of(&pipe.tx);
        assert_eq!(status, STATUS_OK);
        assert_eq!(text, "0\tcom.a\t1\t1.0\t8192\t-\tcom.a\n");
    }

    #[test]
    fn a_bad_crc_on_list_is_refused() {
        let mut pipe = VecPipe::new(alloc::vec![0, 0, 0, 0]);
        handle_list(&mut pipe, &Stub::<true>::new(8), 0);
        assert_eq!(pipe.tx[4], STATUS_CRC_FAIL);
    }

    #[test]
    fn uninstall_of_an_unknown_package_is_not_found() {
        let mut region = MemRegion::new(8, 8);
        let mut device = Stub::<true>::new(8);
        let name = b"com.nope";
        let mut pipe = VecPipe::new(frame(CMD_UNINSTALL, name));
        let reset = handle_uninstall(
            &mut pipe,
            &mut NoCoordinator,
            &mut region,
            &mut device,
            name.len() as u32,
        );
        assert!(!reset);
        let (status, text) = payload_of(&pipe.tx);
        assert_eq!(status, STATUS_NOT_FOUND);
        assert_eq!(text, "not installed");
        assert!(region.ops.is_empty());
    }

    #[test]
    fn uninstall_erases_the_run_reports_ok_and_asks_for_a_reset() {
        let mut region = MemRegion::new(8, 8);
        let mut device = Stub::<true>::new(8);
        device.installed.push((String::from("com.a"), 0, 2));
        let name = b"com.a";
        let mut pipe = VecPipe::new(frame(CMD_UNINSTALL, name));
        let reset = handle_uninstall(
            &mut pipe,
            &mut NoCoordinator,
            &mut region,
            &mut device,
            name.len() as u32,
        );
        assert!(reset, "no reset after uninstall");
        assert_eq!(pipe.tx[4], STATUS_OK);
        assert_eq!(region.ops, alloc::vec![Op::Erase(0, 2)]);
        assert!(device.installed.is_empty(), "the directory was rescanned");
    }
}
