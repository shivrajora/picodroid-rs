// SPDX-License-Identifier: GPL-3.0-only
//! CYW43439 WiFi driver — Rust FFI wrapper around the C driver.
//!
//! The bulk of the driver is compiled from C (third_party/cyw43-driver) via build.rs.
//! This module provides safe Rust functions for initialisation, polling, and
//! joining a network.
//!
//! The surface here is use-driven, not a complete mirror of `cyw43.h`. It
//! once carried the full `CYW43_AUTH_*`, `CYW43_ITF_*` and `CYW43_LINK_*`
//! vocabularies with no caller, so they were removed; leave, scan and the
//! STA status came back with `WifiManager` (`link.rs`). Re-add a constant
//! when the call that needs it arrives, rather than restoring the set
//! wholesale.

/// The link driver built on these bindings (`Cyw43Link: NetLink`).
pub mod link;

/// CYW43 authentication modes (matches CYW43_AUTH_* in cyw43_ll.h).
pub mod auth {
    pub const OPEN: u32 = 0;
    /// WPA (v1) with TKIP: what an old AP a scan reports as WPA needs.
    pub const WPA_TKIP_PSK: u32 = 0x0020_0002;
    pub const WPA2_AES: u32 = 0x0040_0004;
    /// WPA3-SAE only (the vendored driver programs the `sae_password`
    /// iovar on this path).
    pub const WPA3_SAE_AES: u32 = 0x0100_0004;
    /// WPA3-SAE with WPA2-PSK fallback — the right default for mixed-mode
    /// APs.
    pub const WPA3_WPA2_AES: u32 = 0x0140_0004;
}

/// CYW43 interface IDs.
pub mod itf {
    pub const STA: i32 = 0;
}

/// CYW43_COUNTRY('X', 'X', 0) — worldwide regulatory domain.
pub const COUNTRY_WORLDWIDE: u32 = ('X' as u32) | (('X' as u32) << 8);

/// CYW43_CHANNEL_NONE: join on whatever channel the AP uses.  NOTE: 0 is NOT
/// "any channel" — it pins the join to invalid chanspec channel 0.
pub const CHANNEL_NONE: u32 = 0xffff_ffff;

// C FFI bindings — hand-written (no bindgen dependency).
//
// Two of these are `safe`: they take nothing and return one word of the
// driver's state, a zero-initialised C global that is readable at any time.
// Everything else takes a pointer or writes the state, and is reached only
// through [`Driver`].
unsafe extern "C" {
    /// Opaque CYW43 driver state (allocated as a global in the C driver).
    pub static mut cyw43_state: Cyw43State;

    /// Reset driver state (no hardware access; cannot fail).
    fn cyw43_init(self_: *mut Cyw43State);

    /// Driver-owned dispatch hook: NULL until `cyw43_ensure_up` has loaded the
    /// WiFi firmware, then points at the internal poll function.  This is a
    /// function-pointer *variable* in cyw43_ctrl.c, not a function — it must be
    /// read and called through, never imported as a function symbol.
    static mut cyw43_poll: Option<unsafe extern "C" fn()>;

    /// Power the chip, download firmware+CLM, and bring an interface up.
    /// Errors are internal (void return); observe success via get_mac/join.
    fn cyw43_wifi_set_up(self_: *mut Cyw43State, itf: i32, up: bool, country: u32);

    /// Driver lock (recursive mutex in cyw43_port.c).  The poll dispatch and
    /// any code touching driver state must run under it.
    fn cyw43_thread_enter();
    fn cyw43_thread_exit();

    /// Join a WiFi network (STA mode).
    /// Returns 0 on success, negative on error.
    fn cyw43_wifi_join(
        self_: *mut Cyw43State,
        ssid_len: usize,
        ssid: *const u8,
        key_len: usize,
        key: *const u8,
        auth_type: u32,
        bssid: *const u8,
        channel: u32,
    ) -> i32;

    /// Get the MAC address.
    fn cyw43_wifi_get_mac(self_: *mut Cyw43State, itf: i32, mac: *mut [u8; 6]) -> i32;

    /// Set the CYW43 poll task handle (defined in cyw43_port.c).
    fn cyw43_set_poll_task(task: *mut core::ffi::c_void);

    /// Disassociate the STA interface.
    fn cyw43_wifi_leave(self_: *mut Cyw43State, itf: i32) -> i32;

    // The helpers in NetworkInterface_CYW43.c behind `WifiManager` and the
    // join supervisor: the STA state folded with the port's interface-up
    // flag, whether a scan is running, a scan start that owns the options
    // struct, the driver's raw join-state word, and the async-event trace.
    fn picodroid_cyw43_sta_status() -> i32;
    safe fn picodroid_cyw43_scan_active() -> i32;
    fn picodroid_cyw43_scan_start(
        cb: Option<unsafe extern "C" fn(*mut core::ffi::c_void, *const ScanResult) -> i32>,
    ) -> i32;
    safe fn picodroid_cyw43_join_state() -> u32;
    fn picodroid_cyw43_trace_events(on: i32);
}

/// `cyw43_ev_scan_result_t` (cyw43_ll.h), field for field: the driver
/// hands one per sighting to the scan callback.
#[repr(C)]
pub struct ScanResult {
    _0: [u32; 5],
    pub bssid: [u8; 6],
    _1: [u16; 2],
    pub ssid_len: u8,
    pub ssid: [u8; 32],
    _2: [u32; 5],
    pub channel: u16,
    _3: [u8; 4],
    pub rssi: i16,
    _4: [u8; 12],
    /// `CYW43_AUTH_FLAG_*` bits: WEP 0x1, WPA 0x0020_0000, WPA2
    /// 0x0040_0000, WPA3 0x0100_0000.
    pub auth_mode: u32,
}

/// Opaque CYW43 driver state — sized to match the C struct.
/// We never access fields directly; all operations go through FFI.
#[repr(C)]
pub struct Cyw43State {
    _opaque: [u8; 4096], // Conservative upper bound; actual size is smaller
}

/// The C driver, driven from the one task that owns it.
///
/// The vendored driver keeps one global state. Its entry points that other
/// tasks also reach — the IP task's transmit, the poll dispatch the
/// host-wake interrupt schedules — take the driver's own lock
/// (`CYW43_THREAD_ENTER`). What it leaves to its caller is the rest: the
/// reset (`cyw43_init`) runs unlocked and must come first and alone, the
/// poll hook is read outside the lock, and the STA-status and trace helpers
/// touch state fields unlocked, which is sound only on the task whose poll
/// writes them.
///
/// A `Driver` is that caller. There is one ([`Driver::new`] is `unsafe`),
/// it stays on the task that made it (a raw-pointer marker makes it neither
/// `Send` nor `Sync`), and its creator calls [`Driver::init`] once before
/// any other method — so every other method runs after the reset, on the
/// owning task, and needs no `unsafe` at the call.
pub struct Driver {
    _owner_task: core::marker::PhantomData<*mut ()>,
}

impl Driver {
    /// # Safety
    ///
    /// - No other `Driver` exists, now or later: the C state is one global.
    /// - [`init`](Self::init) is called exactly once, before any other
    ///   method.
    /// - Nothing else calls into the driver except through its locked entry
    ///   points (transmit, the scheduled poll dispatch).
    pub const unsafe fn new() -> Self {
        Driver {
            _owner_task: core::marker::PhantomData,
        }
    }

    /// Reset the driver state (no hardware access yet).
    pub fn init(&mut self) {
        // SAFETY: `new`'s contract makes this the first call into the
        // driver, made once, with no other task inside it; the state is the
        // C global, valid for the whole program.
        unsafe { cyw43_init(&raw mut cyw43_state) }
    }

    /// Register the FreeRTOS task handle used for CYW43 event notification.
    ///
    /// # Safety
    /// `task_handle` must be a valid FreeRTOS task handle that outlives all
    /// driver activity.
    pub unsafe fn set_poll_task(&self, task_handle: *mut core::ffi::c_void) {
        // SAFETY: the handle's validity is the caller's contract; the port
        // only stores it, for the notify calls that follow.
        unsafe { cyw43_set_poll_task(task_handle) }
    }

    /// Power up the chip, download WiFi firmware + CLM, and bring the
    /// interface up.  Void in C; a failure surfaces as get_mac/join failing.
    pub fn wifi_set_up(&self, interface: i32, up: bool, country: u32) {
        // SAFETY: after `init`, on the owning task (`new`'s contract); the
        // driver takes its lock for the bring-up.
        unsafe { cyw43_wifi_set_up(&raw mut cyw43_state, interface, up, country) }
    }

    /// Poll the driver — call from the CYW43 task loop.  A no-op until the
    /// driver has installed its poll hook (after firmware load).  Takes the
    /// driver lock: the IP task's TX path contends on the same driver state.
    pub fn poll(&self) {
        // SAFETY: the hook is NULL until the driver's bring-up stores
        // `cyw43_poll_func` in it (cyw43_ensure_up, under the lock: first
        // from `wifi_set_up` on this task; an ensure_up from the IP task's
        // transmit can only store the same function). A word-sized load
        // gives a valid `Option<fn>` either way, and the call itself runs
        // under the lock.
        unsafe {
            if let Some(f) = cyw43_poll {
                cyw43_thread_enter();
                f();
                cyw43_thread_exit();
            }
        }
    }

    /// Join a WiFi network.
    ///
    /// `auth` is a `CYW43_AUTH_*` value from [`auth`]; pass `None` for the
    /// historical automatic choice (OPEN without a password, WPA2-AES with
    /// one).
    pub fn wifi_join(&self, ssid: &[u8], password: &[u8], auth: Option<u32>) -> Result<(), i32> {
        let auth = auth.unwrap_or(if password.is_empty() {
            auth::OPEN
        } else {
            auth::WPA2_AES
        });

        // SAFETY: after `init`, on the owning task; the driver reads both
        // slices, for their lengths, during the call (it copies them into
        // the join request under its lock); a null BSSID means any.
        let ret = unsafe {
            cyw43_wifi_join(
                &raw mut cyw43_state,
                ssid.len(),
                ssid.as_ptr(),
                password.len(),
                password.as_ptr(),
                auth,
                core::ptr::null(), // bssid (any)
                CHANNEL_NONE,      // channel: use the AP's channel
            )
        };
        if ret != 0 {
            return Err(ret);
        }
        Ok(())
    }

    /// Get the MAC address for the STA interface.
    pub fn get_mac(&self) -> Result<[u8; 6], i32> {
        let mut mac = [0u8; 6];
        // SAFETY: the port copies six bytes (the OTP MAC from the state, or
        // a placeholder before set-up) into `mac`, a live local of exactly
        // that size.
        let ret = unsafe { cyw43_wifi_get_mac(&raw mut cyw43_state, itf::STA, &mut mac) };
        if ret != 0 {
            return Err(ret);
        }
        Ok(mac)
    }

    /// Disassociate from the current access point.
    pub fn wifi_leave(&self) -> Result<(), i32> {
        // SAFETY: after `init`, on the owning task; an ioctl under the
        // driver's lock, with no buffer.
        match unsafe { cyw43_wifi_leave(&raw mut cyw43_state, itf::STA) } {
            0 => Ok(()),
            e => Err(e),
        }
    }

    /// The STA's state as `NetworkInterface_CYW43.c` folds it: the driver's
    /// failure kinds (-3 BADAUTH, -2 NONET, -1 FAIL), 0 down, 1 associating,
    /// 2 associated.
    pub fn sta_status(&self) -> i32 {
        // SAFETY: reads plain fields of the state, and after a self-join
        // that keyed, collapses the join word — unlocked, so sound only on
        // the owning task, where the poll that writes them also runs.
        unsafe { picodroid_cyw43_sta_status() }
    }

    /// The driver's raw join-state word, for the log when a join is retried:
    /// bit 0 a join is active, 0x200 authenticated, 0x400 link up, 0x800
    /// keyed; a kind of 2/3/4 in the low nibble is FAIL/NONET/BADAUTH
    /// (`cyw43_ctrl.c`).
    pub fn join_state(&self) -> u32 {
        picodroid_cyw43_join_state()
    }

    /// Log every async event the chip sends (`cyw43: [ms] ASYNC(flags,NAME,
    /// status,reason,itf)`): the join's AUTH/LINK/SET_SSID/PSK_SUP sequence,
    /// a DEAUTH_IND from the AP, and each scan sighting while a scan runs.
    pub fn trace_events(&self, on: bool) {
        // SAFETY: sets or clears one bit of the state's trace flags,
        // unlocked — sound on the owning task, whose poll reads them.
        unsafe { picodroid_cyw43_trace_events(on as i32) }
    }

    /// Whether a scan started with [`scan_start`](Self::scan_start) is
    /// still running.
    pub fn scan_active(&self) -> bool {
        picodroid_cyw43_scan_active() != 0
    }

    /// Start an active scan; `cb` sees every sighting from inside
    /// [`poll`](Self::poll), on this task.
    pub fn scan_start(
        &self,
        cb: unsafe extern "C" fn(*mut core::ffi::c_void, *const ScanResult) -> i32,
    ) -> Result<(), i32> {
        // SAFETY: after `init`, on the owning task; the driver takes its
        // lock, and later calls `cb` with a null `env` and a result it owns
        // for the length of the call, which is the callback's contract.
        match unsafe { picodroid_cyw43_scan_start(Some(cb)) } {
            0 => Ok(()),
            e => Err(e),
        }
    }
}
