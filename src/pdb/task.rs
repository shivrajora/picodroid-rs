use super::cdc_transport::{CdcTransport, PdbCoreCoordinator};
use super::protocol::{
    crc32_frame, CMD_INSTALL, CMD_PING, CMD_SYSMON, FRAME_MAGIC, STATUS_CRC_FAIL, STATUS_ERR,
    STATUS_OK,
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
    // Payload: "picodroid/2.0\0" (14 bytes) + max_papk_bytes (4 bytes LE)
    let mut ping_resp = [0u8; 18];
    ping_resp[..14].copy_from_slice(b"picodroid/2.0\0");
    ping_resp[14..18]
        .copy_from_slice(&(crate::hal::flash::PAPK_MAX_DATA_SIZE as u32).to_le_bytes());
    CdcTransport::send_pdbp_response(STATUS_OK, &ping_resp);
}

// ── pdb_task body ─────────────────────────────────────────────────────────────

pub fn run_pdb_task() -> ! {
    crate::hal::pdb_usb::init();
    #[cfg(feature = "chip-rp2350-hal")]
    crate::pdb::pending::init_park_signal();

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
            _ => CdcTransport::send_pdbp_response(STATUS_ERR, b"unknown cmd"),
        }
    }
}
