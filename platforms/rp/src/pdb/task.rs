// SPDX-License-Identifier: GPL-3.0-only
use super::cdc_transport::{CdcTransport, PdbCoreCoordinator};
use super::protocol::{
    crc32_frame, CMD_INPUT, CMD_INSTALL, CMD_PING, CMD_SYSMON, FRAME_MAGIC, STATUS_CRC_FAIL,
    STATUS_ERR, STATUS_OK,
};

// ── Low-level helpers ─────────────────────────────────────────────────────────

/// Block until the 4-byte "PDBP" magic is found in the byte stream.
/// Re-syncs on any mismatch (discards unrecognised bytes).
fn wait_for_magic() -> bool {
    let mut matched = 0usize;
    // Give up after 64 KB of garbage without a frame start
    for _ in 0..65536usize {
        let b = crate::hal::pdb_usb::queue_read_byte();
        if b == FRAME_MAGIC[matched] {
            matched += 1;
            if matched == FRAME_MAGIC.len() {
                return true;
            }
        } else {
            matched = if b == FRAME_MAGIC[0] { 1 } else { 0 };
        }
    }
    false
}

// ── Command handlers ──────────────────────────────────────────────────────────

fn handle_ping(len: u32) {
    // CMD_PING is a standard framed command with empty payload.
    // The host sends [PDBP][0x00][len=0][crc32]; consume the CRC.
    let wire_crc = crate::hal::pdb_usb::queue_read_u32_le();
    let expected_crc = crc32_frame(CMD_PING, len, &[]);
    if wire_crc != expected_crc {
        CdcTransport::send_pdbp_response(STATUS_CRC_FAIL, b"");
        return;
    }
    // Greeting payload (additive layout, version-string sentinel signals new fields):
    //   [14] "picodroid/2.1\0"
    //   [4]  max_papk_bytes (u32 LE)
    //   [1]  framework_map_version_len
    //   [N]  framework-map-version bytes
    //
    // Older "picodroid/2.0" hosts saw only the first 18 bytes; this is
    // strictly additive, so newer hosts read the version field while
    // older hosts ignore the trailing bytes. New hosts also detect "2.0"
    // and refuse to install, prompting an SWD reflash.
    let fw_ver = crate::app::FRAMEWORK_MAP_VERSION.as_bytes();
    let mut buf = [0u8; 14 + 4 + 1 + 64];
    buf[..14].copy_from_slice(b"picodroid/2.1\0");
    buf[14..18].copy_from_slice(&(crate::hal::flash::PAPK_MAX_DATA_SIZE as u32).to_le_bytes());
    let ver_len = fw_ver.len().min(64) as u8;
    buf[18] = ver_len;
    buf[19..19 + ver_len as usize].copy_from_slice(&fw_ver[..ver_len as usize]);
    let total = 19 + ver_len as usize;
    CdcTransport::send_pdbp_response(STATUS_OK, &buf[..total]);
}

// ── pdb_task body ─────────────────────────────────────────────────────────────

pub fn run_pdb_task() -> ! {
    crate::hal::pdb_usb::init();

    loop {
        if !wait_for_magic() {
            continue;
        }
        let cmd = crate::hal::pdb_usb::queue_read_byte();
        let len = crate::hal::pdb_usb::queue_read_u32_le();
        match cmd {
            CMD_PING => handle_ping(len),
            CMD_INSTALL => {
                let mut transport = CdcTransport;
                let mut coordinator = PdbCoreCoordinator;
                crate::packagemanager::install::run_install(&mut transport, &mut coordinator, len);
            }
            CMD_SYSMON => super::sysmon::handle_sysmon(len),
            CMD_INPUT => super::input::handle_input(len),
            _ => CdcTransport::send_pdbp_response(STATUS_ERR, b"unknown cmd"),
        }
    }
}
