// SPDX-License-Identifier: GPL-3.0-only
//! `CMD_PING` — the greeting.

use pd_install::PapkFlash;
use pdb_protocol::{crc32_frame, greeting, CMD_PING, STATUS_CRC_FAIL, STATUS_OK};

use crate::{send_response, PdbDevice, PdbTransport};

pub fn handle(
    transport: &mut impl PdbTransport,
    flash: &impl PapkFlash,
    device: &impl PdbDevice,
    len: u32,
) {
    // A framed command with an empty payload: consume the trailing CRC so the
    // stream stays aligned whether or not it matches.
    let wire_crc = transport.read_u32_le();
    if wire_crc != crc32_frame(CMD_PING, len, &[]) {
        send_response(transport, STATUS_CRC_FAIL, b"");
        return;
    }
    let g = greeting_of(flash, device);
    send_response(transport, STATUS_OK, &g);
}

/// The greeting layout is `pdb_protocol::greeting`'s; the device supplies
/// the facts only it knows — region capacity, framework-map-version, the
/// board it was built for, its serial and, on a multi-app board, the state
/// of the package directory.
fn greeting_of<D: PdbDevice>(flash: &impl PapkFlash, device: &D) -> Greeting {
    let mut serial = [0u8; greeting::SERIAL_MAX];
    let n = device.serial(&mut serial);
    let mut g = Greeting {
        buf: [0u8; greeting::GREETING_MAX],
        len: 0,
    };
    g.len = if D::MULTI_APP {
        let (largest, free) = device.free_space();
        // A sector of the app region: the unit the directory counts free
        // space in.
        let sector = papk_format::flash_image::META_SIZE as u32;
        let apps = greeting::AppsInfo {
            max: flash.max_installed_apps() as u8,
            installed: device.installed_count() as u8,
            largest_free: largest * sector,
            total_free: free * sector,
        };
        greeting::encode_with_apps(
            flash.max_data_size() as u32,
            D::FRAMEWORK_MAP_VERSION.as_bytes(),
            device.board().as_bytes(),
            &serial[..n],
            &apps,
            &mut g.buf,
        )
    } else {
        greeting::encode(
            flash.max_data_size() as u32,
            D::FRAMEWORK_MAP_VERSION.as_bytes(),
            device.board().as_bytes(),
            &serial[..n],
            &mut g.buf,
        )
    };
    g
}

struct Greeting {
    buf: [u8; greeting::GREETING_MAX],
    len: usize,
}

impl core::ops::Deref for Greeting {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}
