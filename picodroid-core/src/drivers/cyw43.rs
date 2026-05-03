// SPDX-License-Identifier: GPL-3.0-only
//! CYW43439 WiFi driver — Rust FFI wrapper around the C driver.
//!
//! The bulk of the driver is compiled from C (vendor/cyw43-driver) via build.rs.
//! This module provides safe Rust functions for initialisation, polling, and
//! WiFi operations (join, disconnect, link status, RSSI).

/// CYW43 authentication modes (matches CYW43_AUTH_* in cyw43.h).
#[allow(dead_code)]
pub mod auth {
    pub const OPEN: u32 = 0;
    pub const WPA_TKIP: u32 = 0x00200002;
    pub const WPA2_AES: u32 = 0x00400004;
    pub const WPA2_MIXED: u32 = 0x00400006;
}

/// CYW43 interface IDs.
#[allow(dead_code)]
pub mod itf {
    pub const STA: i32 = 0;
    pub const AP: i32 = 1;
}

/// Link status values.
#[allow(dead_code)]
pub mod link {
    pub const DOWN: i32 = 0;
    pub const JOIN: i32 = 1;
    pub const NOIP: i32 = 2;
    pub const UP: i32 = 3;
    pub const FAIL: i32 = -1;
    pub const NONET: i32 = -2;
    pub const BADAUTH: i32 = -3;
}

// C FFI bindings — hand-written (no bindgen dependency).
extern "C" {
    /// Opaque CYW43 driver state (allocated as a global in the C driver).
    pub static mut cyw43_state: Cyw43State;

    /// Initialise the CYW43 driver.  Must be called before any WiFi operations.
    /// `country` is a two-character ISO 3166-1 code (e.g., b"XX").
    fn cyw43_init(
        self_: *mut Cyw43State,
        send_cb: Option<unsafe extern "C" fn(*mut core::ffi::c_void, i32, usize, *const u8)>,
    ) -> i32;

    /// Poll the CYW43 driver — processes pending WiFi events and RX packets.
    /// Must be called regularly from the CYW43 task.
    fn cyw43_poll(self_: *mut Cyw43State);

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

    /// Leave the current WiFi network.
    fn cyw43_wifi_leave(self_: *mut Cyw43State, itf: i32) -> i32;

    /// Get WiFi link status for an interface.
    fn cyw43_tcpip_link_status(self_: *const Cyw43State, itf: i32) -> i32;

    /// Get the WiFi RSSI (signal strength in dBm).
    fn cyw43_wifi_get_rssi(self_: *mut Cyw43State, rssi: *mut i32) -> i32;

    /// Get the MAC address.
    fn cyw43_wifi_get_mac(self_: *mut Cyw43State, itf: i32, mac: *mut [u8; 6]) -> i32;

    /// Set the CYW43 poll task handle (defined in cyw43_port.c).
    fn cyw43_set_poll_task(task: *mut core::ffi::c_void);

    /// Send a raw Ethernet frame.
    fn cyw43_send_ethernet(
        self_: *mut Cyw43State,
        itf: i32,
        len: usize,
        buf: *const u8,
        is_pbuf: bool,
    ) -> i32;
}

/// Opaque CYW43 driver state — sized to match the C struct.
/// We never access fields directly; all operations go through FFI.
#[repr(C)]
pub struct Cyw43State {
    _opaque: [u8; 4096], // Conservative upper bound; actual size is smaller
}

/// Initialise the CYW43439 driver and hardware.
///
/// # Safety
/// Must be called exactly once, before the FreeRTOS scheduler starts or
/// from within a FreeRTOS task.
pub unsafe fn init() -> Result<(), i32> {
    let ret = cyw43_init(&raw mut cyw43_state, None);
    if ret != 0 {
        return Err(ret);
    }
    Ok(())
}

/// Poll the driver — call from the CYW43 task loop.
pub unsafe fn poll() {
    cyw43_poll(&raw mut cyw43_state);
}

/// Register the FreeRTOS task handle used for CYW43 event notification.
pub unsafe fn set_poll_task(task_handle: *mut core::ffi::c_void) {
    cyw43_set_poll_task(task_handle);
}

/// Join a WiFi network with WPA2 authentication.
pub unsafe fn wifi_join(ssid: &[u8], password: &[u8]) -> Result<(), i32> {
    let auth = if password.is_empty() {
        auth::OPEN
    } else {
        auth::WPA2_AES
    };

    let ret = cyw43_wifi_join(
        &raw mut cyw43_state,
        ssid.len(),
        ssid.as_ptr(),
        password.len(),
        password.as_ptr(),
        auth,
        core::ptr::null(), // bssid (any)
        0,                 // channel (any)
    );
    if ret != 0 {
        return Err(ret);
    }
    Ok(())
}

/// Disconnect from the current WiFi network.
pub unsafe fn wifi_leave() -> Result<(), i32> {
    let ret = cyw43_wifi_leave(&raw mut cyw43_state, itf::STA);
    if ret != 0 {
        return Err(ret);
    }
    Ok(())
}

/// Get the current link status for the STA interface.
pub unsafe fn link_status() -> i32 {
    cyw43_tcpip_link_status(&raw const cyw43_state, itf::STA)
}

/// Get the WiFi RSSI (signal strength in dBm).
pub unsafe fn get_rssi() -> Result<i32, i32> {
    let mut rssi: i32 = 0;
    let ret = cyw43_wifi_get_rssi(&raw mut cyw43_state, &mut rssi);
    if ret != 0 {
        return Err(ret);
    }
    Ok(rssi)
}

/// Get the MAC address for the STA interface.
pub unsafe fn get_mac() -> Result<[u8; 6], i32> {
    let mut mac = [0u8; 6];
    let ret = cyw43_wifi_get_mac(&raw mut cyw43_state, itf::STA, &mut mac);
    if ret != 0 {
        return Err(ret);
    }
    Ok(mac)
}
