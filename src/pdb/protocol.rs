pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";

pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;
pub const CMD_SYSMON: u8 = 0x02;

pub const STATUS_OK: u8 = 0x00;
pub const STATUS_READY: u8 = 0x01; // device has erased flash and is ready to receive data
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;
/// Device refused the install: PAPK's framework-map-version is incompatible
/// with this firmware (asymmetric shrunk/unshrunk, or PAPK > firmware).
/// Returned in install Phase A *before* any flash erase, so the existing
/// PAPK on the device is unaffected.
pub const STATUS_INCOMPAT: u8 = 0xFC;

/// Number of PAPK bytes the host inlines immediately after the install
/// header (Phase A) so the device can run a pre-erase compat check
/// against the PAPK manifest without blocking. Sized comfortably above
/// the 24-byte file header + 16-byte section header + a typical manifest.
pub const INSTALL_PEEK_BYTES: usize = 512;

/// CRC32 over [cmd byte][4-byte len LE][payload] — protects all variable fields.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}
