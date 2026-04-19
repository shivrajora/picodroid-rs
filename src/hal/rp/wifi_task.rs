//! CYW43 WiFi task — initialises the CYW43 driver, starts the FreeRTOS+TCP
//! IP stack, joins a WiFi network, and enters the driver poll loop.
//!
//! Gated behind `net-cyw43`; only compiled for the Pico 2 W.

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

/// Entry point for the "cyw43" FreeRTOS task.
///
/// # Safety
/// All CYW43 FFI calls are unsafe.  This function is the sole caller of the
/// driver's init/poll/join functions, running in its own dedicated task.
pub fn run_cyw43_task() -> ! {
    // Initialise the CYW43 hardware (SPI bus, firmware download).
    unsafe {
        cyw43::init().expect("cyw43 init failed");
    }

    // Register this task so the CYW43 ISR can wake us via task notification.
    // freertos-rust returns the handle as `*const c_void`; FreeRTOS itself treats
    // task handles as opaque `void*` so the const-to-mut cast is a no-op in C.
    let task = Task::current().unwrap();
    unsafe {
        cyw43::set_poll_task(task.raw_handle() as *mut core::ffi::c_void);
    }

    // Read MAC address and start the FreeRTOS+TCP IP stack.
    let mac = unsafe { cyw43::get_mac().expect("cyw43 get_mac failed") };
    unsafe {
        picodroid_net_stack_init(mac.as_ptr());
    }

    // Join WiFi if credentials are configured.
    if !WIFI_SSID.is_empty() {
        unsafe {
            let _ = cyw43::wifi_join(WIFI_SSID.as_bytes(), WIFI_PASS.as_bytes());
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
