use std::thread;
use std::time::Duration;

use crate::protocol::{recv_response, send_frame, CMD_PING, STATUS_OK};

const PROBE_BAUD: u32 = 115_200;
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub fn run() {
    let devices = scan();

    if devices.is_empty() {
        println!("no picodroid devices found");
    } else {
        for (name, version) in &devices {
            println!("{name}  {version}");
        }
    }
}

/// Scan all serial ports and return those that respond to a picodroid PING.
pub fn scan() -> Vec<(String, String)> {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    // Skip tty.* ports (block on open waiting for carrier detect) and Bluetooth.
    // On macOS, cu.* ("call-up") ports are the usable ones.
    let candidates: Vec<_> = ports
        .into_iter()
        .filter(|info| {
            let name = &info.port_name;
            !name.contains("/tty.") && !name.contains("Bluetooth")
        })
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    // Probe in parallel so the total wait is ~1 s instead of 1 s × N.
    let handles: Vec<_> = candidates
        .into_iter()
        .map(|info| {
            thread::spawn(move || probe(&info.port_name).map(|version| (info.port_name, version)))
        })
        .collect();

    handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect()
}

/// Try a quick PING on `port_name`. Returns the version string on success.
fn probe(port_name: &str) -> Option<String> {
    let mut port = serialport::new(port_name, PROBE_BAUD)
        .timeout(PROBE_TIMEOUT)
        .open()
        .ok()?;

    send_frame(port.as_mut(), CMD_PING, b"").ok()?;

    let (status, payload) = recv_response(port.as_mut()).ok()?;
    if status != STATUS_OK || payload.len() < 18 {
        return None;
    }

    let version = std::str::from_utf8(&payload[..14])
        .unwrap_or("?")
        .trim_end_matches('\0');
    let max_papk =
        u32::from_le_bytes([payload[14], payload[15], payload[16], payload[17]]) as usize;

    Some(format!("{version}  (max PAPK: {} KB)", max_papk / 1024))
}
