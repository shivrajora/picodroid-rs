// SPDX-License-Identifier: GPL-3.0-only
//! WiFi provisioning state, shared by `picodroid.net.wifi.WifiManager`'s
//! natives, the family's link driver and the simulator's fake
//! (docs/designs/wifi-provisioning-2026-09.md).
//!
//! Family-neutral, like `net_edge.rs`. Four things live here:
//!
//! - The **credential store** at [`STORE_PATH`]: one network, written by
//!   the native side only (tmp + rename), read at boot by the link driver
//!   before any app runs. [`configured`] puts the build-time
//!   `PICODROID_WIFI_SSID` ahead of it, so a firmware built with
//!   credentials joins that network whatever is stored.
//! - The **request mailbox**: the JVM task never touches the driver (its
//!   state is unsynchronised across tasks), so a join, scan or leave is
//!   parked here and the link task is notified; its next `service` pass
//!   takes it. In the simulator the fake runs the request at once.
//! - The **scan table** and the **STA status**, written on the link task
//!   and read by the natives through a seqlock (one writer per value, so
//!   load/store atomics are enough — the RP2040 has no atomic RMW).
//! - [`WIFI_EVENTS`], the generation the event loop watches: a bump means
//!   a scan finished or the status changed, and `WifiManager.fireEvent()`
//!   runs on the main thread between frames.

use core::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};

use crate::hal::net_edge::LinkChanges;

/// Longest SSID, in bytes (802.11).
pub const SSID_MAX: usize = 32;
/// Longest passphrase, in bytes: a WPA2 passphrase is 8–63 characters, a
/// 64-hex-digit PSK fits too.
pub const PASS_MAX: usize = 64;
/// How many networks a scan keeps: the strongest, one per SSID.
pub const SCAN_MAX: usize = 12;
/// Where the saved network lives. Outside `/data`, which is the app
/// sandbox and is swept per package: the network belongs to the device.
pub const STORE_DIR: &str = "/system";
pub const STORE_PATH: &str = "/system/wifi";
const STORE_TMP: &str = "/system/wifi.tmp";

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
    const EMPTY: Credentials = Credentials {
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
    let crc = pico_jvm::native::crc32_update(0, &out[..n]);
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
    if pico_jvm::native::crc32_update(0, &bytes[..end]) != crc {
        return None;
    }
    Credentials::new(
        &bytes[HEADER..HEADER + ssid_len],
        &bytes[HEADER + ssid_len..end],
        security,
    )
}

// ── The store on the volume ─────────────────────────────────────────────────

/// The saved network, if the store holds a valid one.
#[cfg(not(test))]
pub fn load_stored() -> Option<Credentials> {
    use crate::hal::fs;
    if !fs::is_file(STORE_PATH) {
        return None;
    }
    let mut buf = alloc::vec::Vec::new();
    if fs::read_at(STORE_PATH, 0, &mut buf, ENCODED_MAX) < 0 {
        return None;
    }
    decode(&buf)
}

/// Replace the saved network: written to a temporary file and renamed
/// over the store, so a reset mid-write leaves the old record intact.
#[cfg(not(test))]
pub fn save_stored(c: &Credentials) -> bool {
    use crate::hal::fs;
    let mut out = [0u8; ENCODED_MAX];
    let n = encode(c, &mut out);
    if !fs::is_dir(STORE_DIR) && !fs::mkdir(STORE_DIR) {
        return false;
    }
    fs::truncate(STORE_TMP);
    if fs::write_at(STORE_TMP, 0, &out[..n]) < 0 {
        let _ = fs::delete(STORE_TMP);
        return false;
    }
    fs::rename(STORE_TMP, STORE_PATH)
}

/// Forget the saved network. True when nothing is stored afterwards.
#[cfg(not(test))]
pub fn forget_stored() -> bool {
    use crate::hal::fs;
    !fs::is_file(STORE_PATH) || fs::delete(STORE_PATH)
}

// ── The build-time network ──────────────────────────────────────────────────

/// `PICODROID_WIFI_SSID` at build time; empty when unset.
const BUILD_SSID: &str = match option_env!("PICODROID_WIFI_SSID") {
    Some(s) => s,
    None => "",
};
const BUILD_PASS: &str = match option_env!("PICODROID_WIFI_PASS") {
    Some(s) => s,
    None => "",
};
/// `PICODROID_WIFI_AUTH`: `open`, `wpa2`, `wpa3` (SAE only), `wpa2wpa3`
/// (SAE with WPA2-PSK fallback). Unset keeps the historical automatic
/// choice: open without a password, WPA2 with one.
const BUILD_AUTH: &str = match option_env!("PICODROID_WIFI_AUTH") {
    Some(s) => s,
    None => "",
};

/// The network compiled into this firmware, if any.
pub fn build_credentials() -> Option<Credentials> {
    if BUILD_SSID.is_empty() {
        return None;
    }
    let automatic = if BUILD_PASS.is_empty() {
        Security::Open
    } else {
        Security::Wpa2
    };
    let security = match BUILD_AUTH {
        "" => automatic,
        "open" => Security::Open,
        "wpa2" => Security::Wpa2,
        "wpa3" => Security::Wpa3,
        "wpa2wpa3" | "wpa3wpa2" => Security::Wpa2Wpa3,
        other => {
            crate::pd_warn!(
                "wifi: unknown PICODROID_WIFI_AUTH \"{}\" — using automatic auth",
                other
            );
            automatic
        }
    };
    Credentials::new(BUILD_SSID.as_bytes(), BUILD_PASS.as_bytes(), security)
}

/// Where the configured network came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Source {
    None = 0,
    Build = 1,
    Stored = 2,
}

/// The network this device joins at boot: the build-time one first, else
/// the stored one. Reads the volume, so not for every frame.
#[cfg(not(test))]
pub fn configured() -> (Source, Option<Credentials>) {
    if let Some(c) = build_credentials() {
        return (Source::Build, Some(c));
    }
    match load_stored() {
        Some(c) => (Source::Stored, Some(c)),
        None => (Source::None, None),
    }
}

// ── STA status ──────────────────────────────────────────────────────────────

/// What the station is doing, as the link driver last reported it. The
/// `u8` value is what `WifiManager.nativeStatus()` returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Status {
    /// Not associated and not trying.
    Down = 0,
    /// A join was requested; no verdict yet.
    Joining = 1,
    /// Associated and keyed. The address is `ConnectivityManager`'s business.
    Joined = 2,
    /// The join failed for a reason the driver did not classify.
    Fail = 3,
    /// No AP with that SSID answered.
    NoNet = 4,
    /// The AP rejected the password.
    BadAuth = 5,
}

impl Status {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Status::Joining,
            2 => Status::Joined,
            3 => Status::Fail,
            4 => Status::NoNet,
            5 => Status::BadAuth,
            _ => Status::Down,
        }
    }
}

static STATUS: AtomicU8 = AtomicU8::new(Status::Down as u8);

pub fn status() -> Status {
    Status::from_u8(STATUS.load(Ordering::Acquire))
}

/// Record the station's state; a change bumps [`WIFI_EVENTS`].
pub fn set_status(s: Status) {
    if STATUS.load(Ordering::Relaxed) != s as u8 {
        STATUS.store(s as u8, Ordering::Release);
        WIFI_EVENTS.note();
    }
}

/// A scan finished or the status changed since the event loop last
/// looked (`lifecycle::net_events::dispatch_wifi_events`).
pub static WIFI_EVENTS: LinkChanges = LinkChanges::new();

// ── The network the station is on (or was last asked to join) ───────────────

struct Current {
    ssid: [u8; SSID_MAX],
    len: u8,
}

static mut CURRENT: Current = Current {
    ssid: [0; SSID_MAX],
    len: 0,
};
/// Seqlock over `CURRENT`: odd while the (single) writer is inside.
static CURRENT_SEQ: AtomicU32 = AtomicU32::new(0);

/// Record the SSID a join was issued for. Link-driver side.
pub fn set_current_ssid(ssid: &[u8]) {
    let n = ssid.len().min(SSID_MAX);
    let seq = CURRENT_SEQ.load(Ordering::Relaxed);
    CURRENT_SEQ.store(seq.wrapping_add(1), Ordering::Release);
    unsafe {
        let cur = &mut *core::ptr::addr_of_mut!(CURRENT);
        cur.ssid[..n].copy_from_slice(&ssid[..n]);
        cur.len = n as u8;
    }
    CURRENT_SEQ.store(seq.wrapping_add(2), Ordering::Release);
}

/// The SSID of the network the station is on or was last asked to join,
/// copied into `out`; its length (0 when none).
pub fn current_ssid(out: &mut [u8; SSID_MAX]) -> usize {
    loop {
        let s1 = CURRENT_SEQ.load(Ordering::Acquire);
        if s1 & 1 == 1 {
            continue;
        }
        let len = unsafe {
            let cur = &*core::ptr::addr_of!(CURRENT);
            let n = cur.len as usize;
            out[..n].copy_from_slice(&cur.ssid[..n]);
            n
        };
        if CURRENT_SEQ.load(Ordering::Acquire) == s1 {
            return len;
        }
    }
}

// ── The scan table ──────────────────────────────────────────────────────────

/// One network a scan found: the strongest sighting of its SSID.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScanEntry {
    pub ssid: [u8; SSID_MAX],
    pub ssid_len: u8,
    pub bssid: [u8; 6],
    pub rssi: i16,
    pub channel: u8,
    pub security: Security,
}

impl ScanEntry {
    const EMPTY: ScanEntry = ScanEntry {
        ssid: [0; SSID_MAX],
        ssid_len: 0,
        bssid: [0; 6],
        rssi: -127,
        channel: 0,
        security: Security::Open,
    };

    pub fn new(
        ssid: &[u8],
        bssid: [u8; 6],
        rssi: i16,
        channel: u8,
        security: Security,
    ) -> Option<Self> {
        if ssid.is_empty() || ssid.len() > SSID_MAX {
            return None;
        }
        let mut e = Self::EMPTY;
        e.ssid[..ssid.len()].copy_from_slice(ssid);
        e.ssid_len = ssid.len() as u8;
        e.bssid = bssid;
        e.rssi = rssi;
        e.channel = channel;
        e.security = security;
        Some(e)
    }

    pub fn ssid(&self) -> &[u8] {
        &self.ssid[..self.ssid_len as usize]
    }
}

/// The table, sorted strongest first once a scan ends.
static mut SCAN: [ScanEntry; SCAN_MAX] = [ScanEntry::EMPTY; SCAN_MAX];
static SCAN_LEN: AtomicUsize = AtomicUsize::new(0);
/// Seqlock over `SCAN`/`SCAN_LEN`: odd while the writer is inside.
static SCAN_SEQ: AtomicU32 = AtomicU32::new(0);
/// How many scans have completed since boot.
static SCAN_GEN: AtomicU32 = AtomicU32::new(0);
static SCANNING: AtomicU8 = AtomicU8::new(0);

fn scan_write_begin() -> u32 {
    let seq = SCAN_SEQ.load(Ordering::Relaxed);
    SCAN_SEQ.store(seq.wrapping_add(1), Ordering::Release);
    seq
}

fn scan_write_end(seq: u32) {
    SCAN_SEQ.store(seq.wrapping_add(2), Ordering::Release);
}

/// A scan is starting: empty the table. Link-driver side.
pub fn scan_begin() {
    let seq = scan_write_begin();
    SCAN_LEN.store(0, Ordering::Relaxed);
    scan_write_end(seq);
    SCANNING.store(1, Ordering::Release);
}

/// One sighting. The table keeps one entry per SSID, the strongest; a
/// full table drops the weakest for a stronger newcomer. Link-driver side.
pub fn scan_add(entry: ScanEntry) {
    let seq = scan_write_begin();
    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SCAN);
        let len = SCAN_LEN.load(Ordering::Relaxed);
        let slot = table[..len].iter().position(|e| e.ssid() == entry.ssid());
        match slot {
            Some(i) => {
                if entry.rssi > table[i].rssi {
                    table[i] = entry;
                }
            }
            None if len < SCAN_MAX => {
                table[len] = entry;
                SCAN_LEN.store(len + 1, Ordering::Relaxed);
            }
            None => {
                let (weakest, w) = table
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.rssi)
                    .map(|(i, e)| (i, e.rssi))
                    .unwrap_or((0, i16::MIN));
                if entry.rssi > w {
                    table[weakest] = entry;
                }
            }
        }
    }
    scan_write_end(seq);
}

/// The scan finished: sort strongest first, publish, bump the generation
/// and [`WIFI_EVENTS`]. Link-driver side.
pub fn scan_end() {
    let seq = scan_write_begin();
    unsafe {
        let table = &mut *core::ptr::addr_of_mut!(SCAN);
        let len = SCAN_LEN.load(Ordering::Relaxed);
        table[..len].sort_unstable_by_key(|e| core::cmp::Reverse(e.rssi));
    }
    scan_write_end(seq);
    SCANNING.store(0, Ordering::Release);
    let g = SCAN_GEN.load(Ordering::Relaxed).wrapping_add(1);
    SCAN_GEN.store(g, Ordering::Release);
    WIFI_EVENTS.note();
}

pub fn is_scanning() -> bool {
    SCANNING.load(Ordering::Acquire) != 0
}

pub fn scan_generation() -> u32 {
    SCAN_GEN.load(Ordering::Acquire)
}

pub fn scan_count() -> usize {
    SCAN_LEN.load(Ordering::Acquire).min(SCAN_MAX)
}

/// Entry `i` of the last scan, or `None` past the end.
pub fn scan_entry(i: usize) -> Option<ScanEntry> {
    loop {
        let s1 = SCAN_SEQ.load(Ordering::Acquire);
        if s1 & 1 == 1 {
            continue;
        }
        let got = unsafe {
            let table = &*core::ptr::addr_of!(SCAN);
            let len = SCAN_LEN.load(Ordering::Relaxed).min(SCAN_MAX);
            if i < len {
                Some(table[i])
            } else {
                None
            }
        };
        if SCAN_SEQ.load(Ordering::Acquire) == s1 {
            return got;
        }
    }
}

/// The last scan's entry for `ssid`, if it saw one.
pub fn scan_lookup(ssid: &[u8]) -> Option<ScanEntry> {
    (0..scan_count())
        .filter_map(scan_entry)
        .find(|e| e.ssid() == ssid)
}

// ── The request mailbox ─────────────────────────────────────────────────────

/// What the Java side asks of the link driver.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Request {
    Join(Credentials),
    Scan,
    Leave,
}

const REQ_NONE: u8 = 0;
const REQ_JOIN: u8 = 1;
const REQ_SCAN: u8 = 2;
const REQ_LEAVE: u8 = 3;

static REQ_KIND: AtomicU8 = AtomicU8::new(REQ_NONE);
static mut REQ_CREDS: Credentials = Credentials::EMPTY;
/// The link task to notify after a submit; 0 until the driver registers.
static LINK_TASK: AtomicUsize = AtomicUsize::new(0);

/// The link driver's task, notified on every submit so a request does not
/// wait out the service timeout. Called once from the driver's `init`.
pub fn register_link_task(task: crate::rtos::RawTask) {
    LINK_TASK.store(task, Ordering::Release);
}

/// Park a request for the link task. False when one is already waiting:
/// the caller retries after the next event rather than queueing.
///
/// One producer (the JVM task) and one consumer, so the kind flag doubles
/// as the lock on the credential slot: the slot is written only while the
/// flag reads none, and the flag is stored after it (release).
pub fn submit(req: Request) -> bool {
    if REQ_KIND.load(Ordering::Acquire) != REQ_NONE {
        return false;
    }
    let kind = match req {
        Request::Join(c) => {
            unsafe {
                *core::ptr::addr_of_mut!(REQ_CREDS) = c;
            }
            REQ_JOIN
        }
        Request::Scan => REQ_SCAN,
        Request::Leave => REQ_LEAVE,
    };
    REQ_KIND.store(kind, Ordering::Release);
    kick();
    true
}

/// The waiting request, if any, taken. Link-driver side.
pub fn take_request() -> Option<Request> {
    let req = match REQ_KIND.load(Ordering::Acquire) {
        REQ_NONE => return None,
        REQ_JOIN => Request::Join(unsafe { *core::ptr::addr_of!(REQ_CREDS) }),
        REQ_SCAN => Request::Scan,
        _ => Request::Leave,
    };
    REQ_KIND.store(REQ_NONE, Ordering::Release);
    Some(req)
}

/// Wake whoever serves the mailbox: the link task on a device; in the
/// simulator the fake, which runs the request now.
fn kick() {
    #[cfg(feature = "sim")]
    crate::hal::sim::wifi::service();
    #[cfg(not(feature = "sim"))]
    {
        let task = LINK_TASK.load(Ordering::Acquire);
        if task != 0 {
            crate::rtos::task_notify(task);
        }
    }
}

/// Whether this build has a WiFi link at all (`network_link_wifi`, from
/// the board's `network_type`). The natives answer with this on a board
/// whose framework still carries `WifiManager`.
pub const fn available() -> bool {
    cfg!(network_link_wifi)
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

    #[test]
    fn scan_table_keeps_strongest_per_ssid_sorted() {
        let e = |s: &str, rssi: i16| {
            ScanEntry::new(s.as_bytes(), [0; 6], rssi, 6, Security::Wpa2).unwrap()
        };
        scan_begin();
        assert!(is_scanning());
        scan_add(e("a", -80));
        scan_add(e("b", -50));
        scan_add(e("a", -40)); // a stronger sighting of `a` replaces it
        scan_add(e("a", -90)); // a weaker one does not
        scan_end();
        assert!(!is_scanning());
        assert_eq!(scan_count(), 2);
        assert_eq!(scan_entry(0).unwrap().ssid(), b"a");
        assert_eq!(scan_entry(0).unwrap().rssi, -40);
        assert_eq!(scan_entry(1).unwrap().ssid(), b"b");
        assert_eq!(scan_entry(2), None);
        assert_eq!(scan_lookup(b"b").unwrap().rssi, -50);
        assert_eq!(scan_lookup(b"zzz"), None);
    }

    #[test]
    fn full_scan_table_drops_the_weakest() {
        let e = |i: usize, rssi: i16| {
            ScanEntry::new(
                alloc::format!("n{i}").as_bytes(),
                [0; 6],
                rssi,
                1,
                Security::Open,
            )
            .unwrap()
        };
        scan_begin();
        for i in 0..SCAN_MAX {
            scan_add(e(i, -60 - i as i16)); // n0 strongest … n11 weakest (-71)
        }
        scan_add(e(99, -90)); // weaker than everything: dropped
        scan_add(e(42, -30)); // stronger: replaces n11
        scan_end();
        assert_eq!(scan_count(), SCAN_MAX);
        assert_eq!(scan_entry(0).unwrap().ssid(), b"n42");
        assert!(scan_lookup(b"n11").is_none());
        assert!(scan_lookup(b"n99").is_none());
    }

    #[test]
    fn mailbox_holds_one_request() {
        assert_eq!(take_request(), None);
        let c = creds("Home", "secret12", Security::Wpa2);
        assert!(submit(Request::Join(c)));
        assert!(!submit(Request::Scan), "a waiting request is not replaced");
        assert_eq!(take_request(), Some(Request::Join(c)));
        assert_eq!(take_request(), None);
        assert!(submit(Request::Scan));
        assert_eq!(take_request(), Some(Request::Scan));
    }

    #[test]
    fn status_change_bumps_events() {
        let before = WIFI_EVENTS.generation();
        set_status(Status::Joining);
        assert_ne!(WIFI_EVENTS.generation(), before);
        let again = WIFI_EVENTS.generation();
        set_status(Status::Joining);
        assert_eq!(WIFI_EVENTS.generation(), again, "a repeat is not an event");
        set_status(Status::Down);
    }

    #[test]
    fn current_ssid_round_trips() {
        set_current_ssid(b"Home Net");
        let mut out = [0u8; SSID_MAX];
        let n = current_ssid(&mut out);
        assert_eq!(&out[..n], b"Home Net");
        set_current_ssid(b"");
        assert_eq!(current_ssid(&mut out), 0);
    }
}
