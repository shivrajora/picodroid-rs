// SPDX-License-Identifier: GPL-3.0-only
//! CYW43 WiFi task — initialises the CYW43 driver, starts the FreeRTOS+TCP
//! IP stack, joins a WiFi network, and enters the driver poll loop.
//!
//! Gated behind the `network_cyw43` cfg; only compiled for the Pico 2 W.

use freertos_rust::*;

use crate::drivers::cyw43;

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

/// Network up/down events from FreeRTOS+TCP (net_init.c hook).
/// `ip_nbo` is the endpoint address in network byte order within a
/// little-endian u32, so the first octet is the low byte.
#[no_mangle]
pub extern "C" fn picodroid_net_ip_event(up: u32, ip_nbo: u32) {
    let o = ip_nbo.to_le_bytes();
    if up != 0 {
        defmt::info!(
            "net: up, ip {=u8}.{=u8}.{=u8}.{=u8}",
            o[0],
            o[1],
            o[2],
            o[3]
        );
    } else {
        defmt::warn!("net: down");
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
        match unsafe { cyw43::wifi_join(WIFI_SSID.as_bytes(), WIFI_PASS.as_bytes()) } {
            Ok(()) => defmt::info!("wifi: join \"{=str}\" requested", WIFI_SSID),
            Err(e) => defmt::warn!("wifi: join \"{=str}\" failed: {=i32}", WIFI_SSID, e),
        }
    }

    // Driver poll loop — woken by ISR notifications or 100 ms timeout.
    loop {
        CurrentTask::take_notification(true, Duration::ms(100));
        unsafe {
            cyw43::poll();
        }
    }
}
