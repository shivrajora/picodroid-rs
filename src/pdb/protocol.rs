pub const FRAME_MAGIC: &[u8; 4] = b"PDBP";

pub const CMD_PING: u8 = 0x00;
pub const CMD_INSTALL: u8 = 0x01;

pub const STATUS_OK: u8 = 0x00;
pub const STATUS_ERR: u8 = 0xFF;
pub const STATUS_TOO_LARGE: u8 = 0xFE;
pub const STATUS_CRC_FAIL: u8 = 0xFD;

/// CRC32 over [cmd byte][4-byte len LE][payload] — protects all variable fields.
pub fn crc32_frame(cmd: u8, len: u32, payload: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(&[cmd]);
    h.update(&len.to_le_bytes());
    h.update(payload);
    h.finalize()
}
