// SPDX-License-Identifier: GPL-3.0-only
//! CYW43439 WiFi driver — Rust FFI wrapper around the C driver.
//!
//! The bulk of the driver is compiled from C (third_party/cyw43-driver) via build.rs.
//! This module provides safe Rust functions for initialisation, polling, and
//! joining a network.
//!
//! The surface here is use-driven, not a complete mirror of `cyw43.h`. It
//! once wrapped disconnect / link-status / RSSI and carried the full
//! `CYW43_AUTH_*`, `CYW43_ITF_*` and `CYW43_LINK_*` vocabularies; none of it
//! ever acquired a caller, so it was removed. Re-add a constant when the
//! call that needs it arrives, rather than restoring the set wholesale.

/// The link driver built on these bindings (`Cyw43Link: NetLink`).
pub mod link;

/// CYW43 authentication modes (matches CYW43_AUTH_* in cyw43_ll.h).
pub mod auth {
    pub const OPEN: u32 = 0;
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
extern "C" {
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
}

/// Opaque CYW43 driver state — sized to match the C struct.
/// We never access fields directly; all operations go through FFI.
#[repr(C)]
pub struct Cyw43State {
    _opaque: [u8; 4096], // Conservative upper bound; actual size is smaller
}

/// Initialise the CYW43439 driver state (no hardware access yet).
///
/// # Safety
/// Must be called exactly once, before the FreeRTOS scheduler starts or
/// from within a FreeRTOS task, and before any other function in this module.
pub unsafe fn init() {
    cyw43_init(&raw mut cyw43_state);
}

/// Power up the chip, download WiFi firmware + CLM, and bring the STA
/// interface up.  Void in C; a failure surfaces as get_mac/join failing.
///
/// # Safety
/// [`init`] must have been called; call only from the CYW43 task.
pub unsafe fn wifi_set_up(interface: i32, up: bool, country: u32) {
    cyw43_wifi_set_up(&raw mut cyw43_state, interface, up, country);
}

/// Poll the driver — call from the CYW43 task loop.  A no-op until the
/// driver has installed its poll hook (after firmware load).
///
/// # Safety
/// [`init`] must have been called; call only from the CYW43 task.  Takes the
/// driver lock: the IP task's TX path contends on the same driver state.
pub unsafe fn poll() {
    if let Some(f) = cyw43_poll {
        cyw43_thread_enter();
        f();
        cyw43_thread_exit();
    }
}

/// Register the FreeRTOS task handle used for CYW43 event notification.
///
/// # Safety
/// `task_handle` must be a valid FreeRTOS task handle that outlives all
/// driver activity.
pub unsafe fn set_poll_task(task_handle: *mut core::ffi::c_void) {
    cyw43_set_poll_task(task_handle);
}

/// Join a WiFi network.
///
/// `auth` is a `CYW43_AUTH_*` value from [`auth`]; pass `None` for the
/// historical automatic choice (OPEN without a password, WPA2-AES with
/// one).
///
/// # Safety
/// [`init`] must have succeeded; call only from the CYW43 task (the driver
/// state is unsynchronized).
pub unsafe fn wifi_join(ssid: &[u8], password: &[u8], auth: Option<u32>) -> Result<(), i32> {
    let auth = auth.unwrap_or(if password.is_empty() {
        auth::OPEN
    } else {
        auth::WPA2_AES
    });

    let ret = cyw43_wifi_join(
        &raw mut cyw43_state,
        ssid.len(),
        ssid.as_ptr(),
        password.len(),
        password.as_ptr(),
        auth,
        core::ptr::null(), // bssid (any)
        CHANNEL_NONE,      // channel: use the AP's channel
    );
    if ret != 0 {
        return Err(ret);
    }
    Ok(())
}

/// Get the MAC address for the STA interface.
///
/// # Safety
/// [`init`] must have succeeded; call only from the CYW43 task (the driver
/// state is unsynchronized).
pub unsafe fn get_mac() -> Result<[u8; 6], i32> {
    let mut mac = [0u8; 6];
    let ret = cyw43_wifi_get_mac(&raw mut cyw43_state, itf::STA, &mut mac);
    if ret != 0 {
        return Err(ret);
    }
    Ok(mac)
}
