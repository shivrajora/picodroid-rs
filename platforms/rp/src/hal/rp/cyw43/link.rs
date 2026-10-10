// SPDX-License-Identifier: GPL-3.0-only
//! `Cyw43Link` — the CYW43439 WiFi module as a picodroid network link
//! (`picodroid_core::hal::NetLink`), driven by core's `run_link_task`
//! (docs/designs/network-seam-2026-09.md D6). This is the reference link
//! driver: a new chip copies its shape — a `NetLink` here, a
//! `NetworkInterface_<X>.c` next to `port/net/NetworkInterface_CYW43.c`.
//!
//! Provisioning (docs/designs/wifi-provisioning-2026-09.md): the network
//! joined at boot comes from `hal::wifi::configured()` — the build-time
//! `PICODROID_WIFI_SSID` first, else the one Settings saved. After that,
//! every `service` pass drains the `hal::wifi` mailbox (a join, scan or
//! leave the Java `WifiManager` asked for), feeds scan sightings into the
//! shared table and mirrors the driver's STA state, so the driver is only
//! ever touched from this task.
//!
//! Keeping the network joined (docs/networking-followups-2026-08.md
//! NET-12): the chip's driver issues one join and reports what became of
//! it; it retries nothing on its own, so a join that ends in NONET, a
//! deauth, a link lost during the handshake, or one whose events never
//! arrive left the board off the network until the next reboot. The
//! `JoinSupervisor` watches the mirrored status and rejoins with a
//! doubling backoff — the pure policy lives in core, the driver calls
//! here execute it.
//!
//! Gated behind the `network_cyw43` cfg; only compiled for the Pico 2 W.

use freertos_rust::*;
use picodroid_core::hal::types::LinkKind;
use picodroid_core::hal::wifi::{self, Credentials, Request, ScanEntry, Security, Source, Status};
use picodroid_core::hal::wifi_join::{JoinSupervisor, Reason};
use picodroid_core::hal::NetLink;

/// CYW43 driver log shim: receives the already-formatted message from
/// the C-side mini formatter (picodroid_cyw43_log_fmt in cyw43_port.c).
///
/// # Safety
/// `fmt` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn picodroid_cyw43_log_str(fmt: *const core::ffi::c_char) {
    if fmt.is_null() {
        return;
    }
    if let Ok(s) = core::ffi::CStr::from_ptr(fmt).to_str() {
        defmt::info!("cyw43: {=str}", s.trim_end());
    }
}

/// A scan that has not completed after this long is closed as it stands.
const SCAN_TIMEOUT_MS: u32 = 10_000;
/// After a leave, how long to give the driver's DEAUTH event before the
/// next join is issued over it.
const LEAVE_SETTLE_MS: u32 = 500;
/// A retry that falls due while the chip is mid-way through a join of
/// its own (the firmware keeps trying the SSID after a NONET verdict)
/// waits this long for it to finish before issuing over it.
const SELF_JOIN_GRACE_MS: u32 = 5_000;
/// The driver's join-state progress bits: AUTH, LINK, KEYED.
const JOIN_PROGRESS_BITS: u32 = 0x0e00;

fn now_ms() -> u32 {
    (picodroid_core::hal::system_clock::elapsed_realtime_nanos() / 1_000_000) as u32
}

/// The `CYW43_AUTH_*` value a join with this security needs; `None` for
/// WEP, which the driver has no path for.
fn auth_for(security: Security) -> Option<u32> {
    Some(match security {
        Security::Open => super::auth::OPEN,
        Security::Wep => return None,
        Security::Wpa => super::auth::WPA_TKIP_PSK,
        Security::Wpa2 => super::auth::WPA2_AES,
        Security::Wpa3 => super::auth::WPA3_SAE_AES,
        Security::Wpa2Wpa3 => super::auth::WPA3_WPA2_AES,
    })
}

/// A scan sighting's `auth_mode` bits as a `Security`. The privacy bit
/// (`WEP_ENABLED`) is set for every secured AP, so it means WEP only when
/// no WPA generation is flagged.
fn security_from_auth(bits: u32) -> Security {
    const WEP: u32 = 0x0000_0001;
    const WPA: u32 = 0x0020_0000;
    const WPA2: u32 = 0x0040_0000;
    const WPA3: u32 = 0x0100_0000;
    if bits & WPA3 != 0 {
        if bits & WPA2 != 0 {
            Security::Wpa2Wpa3
        } else {
            Security::Wpa3
        }
    } else if bits & WPA2 != 0 {
        Security::Wpa2
    } else if bits & WPA != 0 {
        Security::Wpa
    } else if bits & WEP != 0 {
        Security::Wep
    } else {
        Security::Open
    }
}

/// The supervisor's reason for a retry, for the log.
fn reason_str(reason: Reason) -> &'static str {
    match reason {
        Reason::NoNet => "no such network",
        Reason::Fail => "join failed",
        Reason::BadAuth => "bad password",
        Reason::Down => "link lost",
        Reason::Stuck => "no verdict",
    }
}

/// One sighting from the driver, inside `cyw43_poll` on this task. Hidden
/// networks (an empty SSID) are skipped: nothing to show or to type.
unsafe extern "C" fn scan_result_cb(
    _env: *mut core::ffi::c_void,
    result: *const super::ScanResult,
) -> i32 {
    if result.is_null() {
        return 0;
    }
    let r = unsafe { &*result };
    let len = (r.ssid_len as usize).min(r.ssid.len());
    if len == 0 {
        return 0;
    }
    if let Some(entry) = ScanEntry::new(
        &r.ssid[..len],
        r.bssid,
        r.rssi,
        r.channel as u8,
        security_from_auth(r.auth_mode),
    ) {
        wifi::scan_add(entry);
    }
    0
}

/// The CYW43439 over the family's PIO gSPI transport (`pio_spi.rs`).
pub struct Cyw43Link {
    /// A scan is running: `scan_end` once the driver says it is done, or
    /// at `scan_started_ms + SCAN_TIMEOUT_MS`.
    scan_started_ms: Option<u32>,
    /// A join waiting for a leave to settle: the credentials, when it may
    /// go, and the log's word for who asked.
    pending_join: Option<(Credentials, u32, &'static str)>,
    /// What the driver's STA state was last mirrored as.
    last_status: Option<Status>,
    /// The network to keep joined: the configured one from boot, or the
    /// app's last join; cleared by a leave.
    wanted: Option<Credentials>,
    /// Rejoins `wanted` when a join ends without it.
    supervisor: JoinSupervisor,
    /// When a due retry first found the chip joining by itself.
    self_join_seen_ms: Option<u32>,
}

impl Cyw43Link {
    pub const fn new() -> Self {
        Cyw43Link {
            scan_started_ms: None,
            pending_join: None,
            last_status: None,
            wanted: None,
            supervisor: JoinSupervisor::new(),
            self_join_seen_ms: None,
        }
    }

    /// Issue a join for `c` now. The status goes to `Joining` on success
    /// and to `Fail` when the driver refuses the request outright (the
    /// supervisor then retries it, unless the refusal is final).
    fn join(&mut self, c: &Credentials, why: &str) {
        let ssid = core::str::from_utf8(c.ssid()).unwrap_or("?");
        wifi::set_current_ssid(c.ssid());
        let Some(auth) = auth_for(c.security) else {
            defmt::warn!("wifi: join \"{=str}\" refused: WEP is not supported", ssid);
            self.wanted = None;
            self.supervisor.cleared();
            wifi::set_status(Status::Fail);
            return;
        };
        self.supervisor.issued(now_ms());
        match unsafe { super::wifi_join(c.ssid(), c.pass(), Some(auth)) } {
            Ok(()) => {
                defmt::info!("wifi: join \"{=str}\" requested ({=str})", ssid, why);
                self.last_status = Some(Status::Joining);
                wifi::set_status(Status::Joining);
            }
            Err(e) => {
                defmt::warn!("wifi: join \"{=str}\" failed: {=i32}", ssid, e);
                self.last_status = Some(Status::Fail);
                wifi::set_status(Status::Fail);
            }
        }
    }

    /// Leave now and join `c` once the driver's DEAUTH has had its moment.
    fn leave_then_join(&mut self, c: Credentials, why: &'static str) {
        match unsafe { super::wifi_leave() } {
            Ok(()) => defmt::info!("wifi: leaving for a new join"),
            Err(e) => defmt::warn!("wifi: leave failed: {=i32}", e),
        }
        wifi::set_status(Status::Joining);
        self.last_status = Some(Status::Joining);
        self.pending_join = Some((c, now_ms().wrapping_add(LEAVE_SETTLE_MS), why));
    }

    /// A join asked for by the app: leave first when associated (or
    /// still trying), join straight away when down. Starts the
    /// supervisor's ladder over.
    fn request_join(&mut self, c: Credentials) {
        self.wanted = Some(c);
        self.supervisor.requested(now_ms());
        if unsafe { super::sta_status() } > 0 {
            self.leave_then_join(c, "app");
        } else {
            self.join(&c, "app");
        }
    }

    fn request_scan(&mut self) {
        if self.scan_started_ms.is_some() {
            return; // one at a time; the table fills from the running scan
        }
        wifi::scan_begin();
        match unsafe { super::scan_start(scan_result_cb) } {
            Ok(()) => {
                defmt::info!("wifi: scan started");
                self.scan_started_ms = Some(now_ms());
            }
            Err(e) => {
                defmt::warn!("wifi: scan failed to start: {=i32}", e);
                wifi::scan_end();
            }
        }
    }

    fn request_leave(&mut self) {
        self.pending_join = None;
        self.wanted = None;
        self.supervisor.cleared();
        match unsafe { super::wifi_leave() } {
            Ok(()) => defmt::info!("wifi: left"),
            Err(e) => defmt::warn!("wifi: leave failed: {=i32}", e),
        }
        wifi::set_current_ssid(&[]);
        self.last_status = Some(Status::Down);
        wifi::set_status(Status::Down);
    }

    /// The driver's STA state, folded by `picodroid_cyw43_sta_status`,
    /// mirrored into `hal::wifi` when it moved.
    fn mirror_status(&mut self) {
        // Nothing to mirror between a leave and the join that follows: the
        // driver reports the old association going down, not the new
        // attempt.
        if self.pending_join.is_some() {
            return;
        }
        let status = match unsafe { super::sta_status() } {
            -3 => Status::BadAuth,
            -2 => Status::NoNet,
            -1 => Status::Fail,
            0 => Status::Down,
            1 => Status::Joining,
            _ => Status::Joined,
        };
        if self.last_status != Some(status) {
            self.last_status = Some(status);
            wifi::set_status(status);
            match status {
                Status::Joined => defmt::info!("wifi: associated"),
                Status::BadAuth => defmt::warn!("wifi: join failed: bad password"),
                Status::NoNet => defmt::warn!("wifi: join failed: no such network"),
                Status::Fail => defmt::warn!("wifi: join failed"),
                Status::Down => defmt::info!("wifi: down"),
                Status::Joining => {}
            }
        }
    }

    /// Show the supervisor the mirrored status and run the retry it asks
    /// for. Idle between a leave and its join, like `mirror_status`.
    fn supervise(&mut self) {
        if self.pending_join.is_some() {
            return;
        }
        let Some(c) = self.wanted else {
            return;
        };
        let status = self.last_status.unwrap_or(Status::Down);
        let now = now_ms();
        let Some(retry) = self.supervisor.observe(status, now) else {
            self.self_join_seen_ms = None;
            return;
        };
        // SAFETY: a field read of the driver state, on the link task after
        // `init`; `service` only runs once `init` succeeded.
        let join_state = unsafe { super::join_state() };
        // Progress bits over a failure verdict: the chip is joining by
        // itself (bench run 3b cycle 42 — the retry landed on top of it
        // and the two joins raced). Give it a moment; the port's collapse
        // takes the station up when it keys, which cancels the retry.
        if join_state & JOIN_PROGRESS_BITS != 0 {
            let since = *self.self_join_seen_ms.get_or_insert(now);
            if now.wrapping_sub(since) < SELF_JOIN_GRACE_MS {
                return;
            }
        }
        self.self_join_seen_ms = None;
        defmt::warn!(
            "wifi: rejoin \"{=str}\" ({=str}; attempt {=u32}, join state {=u32:#x})",
            core::str::from_utf8(c.ssid()).unwrap_or("?"),
            reason_str(retry.reason),
            retry.attempt,
            join_state
        );
        if retry.leave_first {
            self.leave_then_join(c, "retry");
        } else {
            self.join(&c, "retry");
        }
    }
}

impl Default for Cyw43Link {
    fn default() -> Self {
        Self::new()
    }
}

impl NetLink for Cyw43Link {
    const KIND: LinkKind = LinkKind::Wifi;
    const NAME: &'static str = "cyw43";
    /// The host-wake IRQ (NET-5) and TX-side notifications are the real
    /// wake sources; the timeout is only a safety net, so it can be long —
    /// it used to be the sole RX path at 100 ms. A `WifiManager` request
    /// notifies this task too (`hal::wifi::submit`), so it never waits
    /// this out. The join supervisor's deadlines are read on the same
    /// pass, so a retry lands within a second of falling due.
    const SERVICE_TIMEOUT_MS: Option<u32> = Some(1000);

    /// # Safety
    /// All CYW43 FFI calls are unsafe. This is the sole caller of the
    /// driver's init/set_up functions, on the dedicated link task.
    fn init(&mut self) -> Result<(), i32> {
        // Reset driver state (no hardware access yet).
        unsafe {
            super::init();
        }

        // Register this task so the CYW43 ISR can wake us via task notification.
        // freertos-rust returns the handle as `*const c_void`; FreeRTOS itself treats
        // task handles as opaque `void*` so the const-to-mut cast is a no-op in C.
        let task = Task::current().unwrap();
        unsafe {
            super::set_poll_task(task.raw_handle() as *mut core::ffi::c_void);
        }
        // And so a `WifiManager` request wakes the service loop at once.
        wifi::register_link_task(picodroid_core::rtos::task_current());

        // Power the chip, download WiFi firmware + CLM, bring the STA interface up.
        defmt::info!("wifi: cyw43 set_up (STA)");
        unsafe {
            super::wifi_set_up(super::itf::STA, true, super::COUNTRY_WORLDWIDE);
        }
        // Log the chip's async events: a join that goes wrong then says how
        // (NET-12 stayed open for weeks on `net: down` alone). Half a dozen
        // lines per join and one per sighting during a scan; every build,
        // because the boots that need it are the ones nobody attached to
        // on purpose. (`cfg(debug_assertions)` is off in device firmware of
        // both profiles — build-lib.sh::build_firmware — so it cannot gate
        // this.)
        // SAFETY: `set_up` ran above on this task, the only one that
        // touches the driver.
        unsafe {
            super::trace_events(true);
        }

        // Arm the GP24 host-wake IRQ (NET-5): RX now wakes this task the moment
        // the chip asserts the wake line instead of waiting out the poll
        // timeout. The ISR masks the (level-high) interrupt when it fires and
        // CYW43_POST_POLL_HOOK re-arms it after each poll. This programs the
        // calling core's NVIC bank, which is why the link task lives on core 1.
        crate::hal::gpio::hostwake::init();
        Ok(())
    }

    /// OTP-derived; valid once `set_up` succeeded.
    fn mac(&mut self) -> [u8; 6] {
        unsafe { super::get_mac() }.expect("cyw43 get_mac failed")
    }

    /// Join the configured network: the build-time one, else the stored one.
    fn bring_up(&mut self) {
        match wifi::configured() {
            (source, Some(c)) => {
                let why = match source {
                    Source::Build => "build",
                    _ => "stored",
                };
                self.wanted = Some(c);
                self.supervisor.requested(now_ms());
                self.join(&c, why);
            }
            (_, None) => {
                defmt::warn!("wifi: no network configured (Settings > Wi-Fi, or PICODROID_WIFI_SSID) — not joining");
            }
        }
    }

    fn service(&mut self) {
        // Silent poll counter (gdb-read only, never logged): proves the loop
        // itself is running when diagnosing RX stalls (Bug B in
        // docs/designs/cyw43-pio-transport.md).
        INSTR_CYW43_POLLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        unsafe {
            super::poll();
        }

        // What the app asked for since the last pass.
        if let Some(req) = wifi::take_request() {
            match req {
                Request::Join(c) => self.request_join(c),
                Request::Scan => self.request_scan(),
                Request::Leave => self.request_leave(),
            }
        }

        // A join parked behind a leave.
        if let Some((c, not_before, why)) = self.pending_join {
            let now = now_ms();
            if unsafe { super::sta_status() } <= 0 || now.wrapping_sub(not_before) < u32::MAX / 2 {
                self.pending_join = None;
                self.join(&c, why);
            }
        }

        // A running scan: closed when the driver says so, or on timeout.
        if let Some(started) = self.scan_started_ms {
            let done = !unsafe { super::scan_active() };
            let timed_out = now_ms().wrapping_sub(started) >= SCAN_TIMEOUT_MS;
            if done || timed_out {
                self.scan_started_ms = None;
                wifi::scan_end();
                defmt::info!(
                    "wifi: scan {=str}: {=usize} networks",
                    if done { "done" } else { "timed out" },
                    wifi::scan_count()
                );
            }
        }

        self.mirror_status();
        self.supervise();
    }
}

/// Poll-loop iteration counter for the Bug B decision tree; `no_mangle` so a
/// gdb batch script can read it by name alongside the C-side counters.
#[no_mangle]
pub static INSTR_CYW43_POLLS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
