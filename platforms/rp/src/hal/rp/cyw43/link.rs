// SPDX-License-Identifier: GPL-3.0-only
//! `Cyw43Link` — the CYW43439 WiFi module as a picodroid network link
//! (`picodroid_core::hal::NetLink`), driven by core's `run_link_task`
//! (docs/designs/network-seam-2026-09.md D6). This is the reference link
//! driver: a new chip copies its shape — a `NetLink` here, a
//! `NetworkInterface_<X>.c` next to `port/net/NetworkInterface_CYW43.c`.
//!
//! Gated behind the `network_cyw43` cfg; only compiled for the Pico 2 W.

use freertos_rust::*;
use picodroid_core::hal::types::LinkKind;
use picodroid_core::hal::NetLink;

/// WiFi SSID (override at build time with `PICODROID_WIFI_SSID` env var).
const WIFI_SSID: &str = match option_env!("PICODROID_WIFI_SSID") {
    Some(s) => s,
    None => "",
};

/// WiFi password (override at build time with `PICODROID_WIFI_PASS` env var).
const WIFI_PASS: &str = match option_env!("PICODROID_WIFI_PASS") {
    Some(s) => s,
    None => "",
};

/// WiFi auth mode (override at build time with `PICODROID_WIFI_AUTH`).
/// Accepted values: `open`, `wpa2`, `wpa3` (SAE-only), `wpa2wpa3` (SAE with
/// WPA2-PSK fallback — the right choice for mixed-mode APs). Unset keeps
/// the historical automatic choice: OPEN without a password, WPA2 with one.
const WIFI_AUTH: &str = match option_env!("PICODROID_WIFI_AUTH") {
    Some(s) => s,
    None => "",
};

/// Map `PICODROID_WIFI_AUTH` to a `CYW43_AUTH_*` value (NET-8).
fn wifi_auth_mode() -> Option<u32> {
    match WIFI_AUTH {
        "" => None,
        "open" => Some(super::auth::OPEN),
        "wpa2" => Some(super::auth::WPA2_AES),
        "wpa3" => Some(super::auth::WPA3_SAE_AES),
        "wpa2wpa3" | "wpa3wpa2" => Some(super::auth::WPA3_WPA2_AES),
        other => {
            defmt::warn!(
                "wifi: unknown PICODROID_WIFI_AUTH \"{=str}\" — using automatic auth",
                other
            );
            None
        }
    }
}

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

/// The CYW43439 over the family's PIO gSPI transport (`pio_spi.rs`).
pub struct Cyw43Link;

impl NetLink for Cyw43Link {
    const KIND: LinkKind = LinkKind::Wifi;
    const NAME: &'static str = "cyw43";
    /// The host-wake IRQ (NET-5) and TX-side notifications are the real
    /// wake sources; the timeout is only a safety net, so it can be long —
    /// it used to be the sole RX path at 100 ms.
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

        // Power the chip, download WiFi firmware + CLM, bring the STA interface up.
        defmt::info!("wifi: cyw43 set_up (STA)");
        unsafe {
            super::wifi_set_up(super::itf::STA, true, super::COUNTRY_WORLDWIDE);
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

    /// Join WiFi if credentials are configured.
    fn bring_up(&mut self) {
        if WIFI_SSID.is_empty() {
            defmt::warn!("wifi: no SSID configured (PICODROID_WIFI_SSID) — not joining");
        } else {
            match unsafe {
                super::wifi_join(WIFI_SSID.as_bytes(), WIFI_PASS.as_bytes(), wifi_auth_mode())
            } {
                Ok(()) => defmt::info!("wifi: join \"{=str}\" requested", WIFI_SSID),
                Err(e) => defmt::warn!("wifi: join \"{=str}\" failed: {=i32}", WIFI_SSID, e),
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
    }
}

/// Poll-loop iteration counter for the Bug B decision tree; `no_mangle` so a
/// gdb batch script can read it by name alongside the C-side counters.
#[no_mangle]
pub static INSTR_CYW43_POLLS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
