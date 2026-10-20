// SPDX-License-Identifier: GPL-3.0-only
//! The Wi-Fi credential store's record and the types it carries
//! (docs/designs/wifi-provisioning-2026-09.md): one network, written by the
//! native side only (tmp + rename), read at boot by the link driver before
//! any app runs. The store on the volume, the request mailbox and the scan
//! table are `picodroid_core::hal::wifi`.
//!
//! ```text
//! [PDWF][version u8][security u8][ssid len u8][pass len u8][ssid][pass][crc32 LE]
//! ```

/// Longest SSID, in bytes (802.11).
pub const SSID_MAX: usize = 32;
/// Longest passphrase, in bytes: a WPA2 passphrase is 8–63 characters, a
/// 64-hex-digit PSK fits too.
pub const PASS_MAX: usize = 64;

/// A network's security, as a scan reports it and as a join needs it.
/// The `u8` value is what the store and the Java side carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Security {
    #[default]
    Open = 0,
    /// Seen in scans; not joinable (the driver has no WEP path we bind).
    Wep = 1,
    Wpa = 2,
    Wpa2 = 3,
    Wpa3 = 4,
    /// A mixed-mode AP: WPA3-SAE with WPA2-PSK fallback.
    Wpa2Wpa3 = 5,
}

impl Security {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Security::Open,
            1 => Security::Wep,
            2 => Security::Wpa,
            3 => Security::Wpa2,
            4 => Security::Wpa3,
            5 => Security::Wpa2Wpa3,
            _ => return None,
        })
    }

    /// Whether a join needs a password.
    pub fn secured(self) -> bool {
        !matches!(self, Security::Open)
    }

    /// Android's `ScanResult.capabilities` string for this security.
    pub fn capabilities(self) -> &'static str {
        match self {
            Security::Open => "[ESS]",
            Security::Wep => "[WEP][ESS]",
            Security::Wpa => "[WPA-PSK-TKIP][ESS]",
            Security::Wpa2 => "[WPA2-PSK-CCMP][ESS]",
            Security::Wpa3 => "[WPA3-SAE-CCMP][ESS]",
            Security::Wpa2Wpa3 => "[WPA2-PSK-CCMP][WPA3-SAE-CCMP][ESS]",
        }
    }

    /// What a password implies when nothing better is known: a mixed-mode
    /// join covers WPA2 and WPA3 APs alike; no password means open.
    pub fn guess(has_password: bool) -> Self {
        if has_password {
            Security::Wpa2Wpa3
        } else {
            Security::Open
        }
    }
}

/// One network's credentials: what a join needs and what the store keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Credentials {
    ssid: [u8; SSID_MAX],
    ssid_len: u8,
    pass: [u8; PASS_MAX],
    pass_len: u8,
    pub security: Security,
}

impl Credentials {
    /// No network: what a `static` holds before a request arrives.
    pub const EMPTY: Credentials = Credentials {
        ssid: [0; SSID_MAX],
        ssid_len: 0,
        pass: [0; PASS_MAX],
        pass_len: 0,
        security: Security::Open,
    };

    /// `None` when the SSID is empty or either field is too long.
    pub fn new(ssid: &[u8], pass: &[u8], security: Security) -> Option<Self> {
        if ssid.is_empty() || ssid.len() > SSID_MAX || pass.len() > PASS_MAX {
            return None;
        }
        let mut c = Self::EMPTY;
        c.ssid[..ssid.len()].copy_from_slice(ssid);
        c.ssid_len = ssid.len() as u8;
        c.pass[..pass.len()].copy_from_slice(pass);
        c.pass_len = pass.len() as u8;
        c.security = security;
        Some(c)
    }

    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }

    pub fn pass(&self) -> &[u8] {
        &self.pass[..self.pass_len as usize]
    }
}

// ── The store's format ──────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"PDWF";
const VERSION: u8 = 1;
const HEADER: usize = 4 + 1 + 1 + 1 + 1;
/// The longest encoding: header, both fields at full length, the CRC.
pub const ENCODED_MAX: usize = HEADER + SSID_MAX + PASS_MAX + 4;

fn crc32(bytes: &[u8]) -> u32 {
    let mut h = pdb_protocol::Crc32::new();
    h.update(bytes);
    h.finalize()
}

/// Encode `c` into `out`; the number of bytes written.
pub fn encode(c: &Credentials, out: &mut [u8; ENCODED_MAX]) -> usize {
    out[..4].copy_from_slice(MAGIC);
    out[4] = VERSION;
    out[5] = c.security as u8;
    out[6] = c.ssid_len;
    out[7] = c.pass_len;
    let mut n = HEADER;
    out[n..n + c.ssid().len()].copy_from_slice(c.ssid());
    n += c.ssid().len();
    out[n..n + c.pass().len()].copy_from_slice(c.pass());
    n += c.pass().len();
    let crc = crc32(&out[..n]);
    out[n..n + 4].copy_from_slice(&crc.to_le_bytes());
    n + 4
}

/// Decode a store image; `None` for anything but a well-formed, CRC-clean
/// record of this version.
pub fn decode(bytes: &[u8]) -> Option<Credentials> {
    if bytes.len() < HEADER + 4 || &bytes[..4] != MAGIC || bytes[4] != VERSION {
        return None;
    }
    let security = Security::from_u8(bytes[5])?;
    let ssid_len = bytes[6] as usize;
    let pass_len = bytes[7] as usize;
    let end = HEADER + ssid_len + pass_len;
    if bytes.len() < end + 4 {
        return None;
    }
    let crc = u32::from_le_bytes([bytes[end], bytes[end + 1], bytes[end + 2], bytes[end + 3]]);
    if crc32(&bytes[..end]) != crc {
        return None;
    }
    Credentials::new(
        &bytes[HEADER..HEADER + ssid_len],
        &bytes[HEADER + ssid_len..end],
        security,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(ssid: &str, pass: &str, sec: Security) -> Credentials {
        Credentials::new(ssid.as_bytes(), pass.as_bytes(), sec).unwrap()
    }

    #[test]
    fn encode_decode_round_trips() {
        let c = creds("Home Net", "correct horse", Security::Wpa2Wpa3);
        let mut out = [0u8; ENCODED_MAX];
        let n = encode(&c, &mut out);
        assert_eq!(n, HEADER + 8 + 13 + 4);
        assert_eq!(decode(&out[..n]), Some(c));
        // A trailing slack (a read that returns the whole buffer) is fine.
        assert_eq!(decode(&out), Some(c));
    }

    /// The bytes on the volume, pinned: a record written by the firmware
    /// before the CRC moved from the JVM's `crc32_update` to
    /// `pdb_protocol::Crc32` (both IEEE, init `!0`, final complement) must
    /// still read.
    #[test]
    fn the_stored_encoding_is_pinned() {
        let c = creds("net", "pass1234", Security::Wpa2);
        let mut out = [0u8; ENCODED_MAX];
        let n = encode(&c, &mut out);
        let golden: [u8; 23] = [
            0x50, 0x44, 0x57, 0x46, 0x01, 0x03, 0x03, 0x08, 0x6e, 0x65, 0x74, 0x70, 0x61, 0x73,
            0x73, 0x31, 0x32, 0x33, 0x34, 0x29, 0xae, 0x80, 0xcc,
        ];
        assert_eq!(&out[..n], &golden);
        assert_eq!(decode(&golden), Some(c));
        let open: [u8; 16] = [
            0x50, 0x44, 0x57, 0x46, 0x01, 0x00, 0x04, 0x00, 0x6f, 0x70, 0x65, 0x6e, 0x82, 0x0f,
            0x70, 0x05,
        ];
        assert_eq!(decode(&open), Some(creds("open", "", Security::Open)));
    }

    #[test]
    fn open_network_has_no_password() {
        let c = creds("Cafe", "", Security::Open);
        let mut out = [0u8; ENCODED_MAX];
        let n = encode(&c, &mut out);
        let back = decode(&out[..n]).unwrap();
        assert_eq!(back.pass(), b"");
        assert!(!back.security.secured());
    }

    #[test]
    fn corruption_is_refused() {
        let c = creds("Home", "secret12", Security::Wpa2);
        let mut out = [0u8; ENCODED_MAX];
        let n = encode(&c, &mut out);
        let mut bad = out;
        bad[HEADER] ^= 0x20; // flip a bit of the SSID
        assert_eq!(decode(&bad[..n]), None);
        assert_eq!(decode(&out[..n - 1]), None); // truncated CRC
        assert_eq!(decode(b"PDWF"), None);
        let mut wrong_version = out;
        wrong_version[4] = 2;
        assert_eq!(decode(&wrong_version[..n]), None);
    }

    #[test]
    fn limits_are_enforced() {
        assert!(Credentials::new(b"", b"", Security::Open).is_none());
        assert!(Credentials::new(&[b'a'; 33], b"", Security::Open).is_none());
        assert!(Credentials::new(b"x", &[b'p'; 65], Security::Wpa2).is_none());
        assert!(Credentials::new(&[b'a'; 32], &[b'p'; 64], Security::Wpa2).is_some());
    }
}
