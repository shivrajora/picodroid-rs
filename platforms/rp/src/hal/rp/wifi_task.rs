// SPDX-License-Identifier: GPL-3.0-only
//! CYW43 WiFi task — initialises the CYW43 driver, starts the FreeRTOS+TCP
//! IP stack, joins a WiFi network, and enters the driver poll loop.
//!
//! Gated behind the `network_cyw43` cfg; only compiled for the Pico 2 W.

use freertos_rust::*;

use super::cyw43;

extern "C" {
    /// C glue in net_init.c — registers the CYW43 network interface,
    /// creates a DHCP endpoint, and calls FreeRTOS_IPInit_Multi().
    fn picodroid_net_stack_init(mac: *const u8);
}

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
        "open" => Some(cyw43::auth::OPEN),
        "wpa2" => Some(cyw43::auth::WPA2_AES),
        "wpa3" => Some(cyw43::auth::WPA3_SAE_AES),
        "wpa2wpa3" | "wpa3wpa2" => Some(cyw43::auth::WPA3_WPA2_AES),
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

/// Entry point for the "cyw43" FreeRTOS task.
///
/// # Safety
/// All CYW43 FFI calls are unsafe.  This function is the sole caller of the
/// driver's init/poll/join functions, running in its own dedicated task.
pub fn run_cyw43_task() -> ! {
    // Reset driver state (no hardware access yet).
    unsafe {
        cyw43::init();
    }

    // Register this task so the CYW43 ISR can wake us via task notification.
    // freertos-rust returns the handle as `*const c_void`; FreeRTOS itself treats
    // task handles as opaque `void*` so the const-to-mut cast is a no-op in C.
    let task = Task::current().unwrap();
    unsafe {
        cyw43::set_poll_task(task.raw_handle() as *mut core::ffi::c_void);
    }

    // Power the chip, download WiFi firmware + CLM, bring the STA interface up.
    defmt::info!("wifi: cyw43 set_up (STA)");
    unsafe {
        cyw43::wifi_set_up(cyw43::itf::STA, true, cyw43::COUNTRY_WORLDWIDE);
    }

    // Arm the GP24 host-wake IRQ (NET-5): RX now wakes this task the moment
    // the chip asserts the wake line instead of waiting out the poll
    // timeout. The ISR masks the (level-high) interrupt when it fires and
    // CYW43_POST_POLL_HOOK re-arms it after each poll.
    crate::hal::gpio::hostwake::init();

    // Read MAC address (OTP-derived once set_up succeeded) and start the
    // FreeRTOS+TCP IP stack.
    let mac = unsafe { cyw43::get_mac().expect("cyw43 get_mac failed") };
    defmt::info!(
        "wifi: mac {=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}:{=u8:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );
    unsafe {
        picodroid_net_stack_init(mac.as_ptr());
    }

    // Join WiFi if credentials are configured.
    if WIFI_SSID.is_empty() {
        defmt::warn!("wifi: no SSID configured (PICODROID_WIFI_SSID) — not joining");
    } else {
        match unsafe {
            cyw43::wifi_join(WIFI_SSID.as_bytes(), WIFI_PASS.as_bytes(), wifi_auth_mode())
        } {
            Ok(()) => defmt::info!("wifi: join \"{=str}\" requested", WIFI_SSID),
            Err(e) => defmt::warn!("wifi: join \"{=str}\" failed: {=i32}", WIFI_SSID, e),
        }
    }

    // Driver poll loop. Primary wake sources are the host-wake IRQ (RX,
    // async events; NET-5) and TX-side notifications; the timeout is only
    // a safety net now, so it can be long — it used to be the sole RX
    // path at 100 ms.
    loop {
        CurrentTask::take_notification(true, Duration::ms(1000));
        // Silent poll counter (gdb-read only, never logged): proves the loop
        // itself is running when diagnosing RX stalls (Bug B in
        // docs/designs/cyw43-pio-transport.md).
        INSTR_CYW43_POLLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        unsafe {
            cyw43::poll();
        }
    }
}

/// Poll-loop iteration counter for the Bug B decision tree; `no_mangle` so a
/// gdb batch script can read it by name alongside the C-side counters.
#[no_mangle]
pub static INSTR_CYW43_POLLS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
