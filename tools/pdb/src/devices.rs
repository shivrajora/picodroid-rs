// SPDX-License-Identifier: GPL-3.0-only
use std::thread;
use std::time::Duration;

use serialport::SerialPortType;

use crate::protocol::{recv_response, send_frame, CMD_PING, STATUS_OK};

/// USB VID/PID for picodroid CDC device (pid.codes open-source VID).
pub const PICODROID_VID: u16 = 0x1209;
pub const PICODROID_PID: u16 = 0xCDC0;

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
/// Prefers VID/PID-based detection for USB CDC; falls back to PING probe.
pub fn scan() -> Vec<(String, String)> {
    let ports = match serialport::available_ports() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    // Fast path: match by VID/PID first.
    let mut vid_pid_ports = Vec::new();
    let mut other_candidates = Vec::new();

    for info in ports {
        let name = &info.port_name;
        if name.contains("/tty.") || name.contains("Bluetooth") {
            continue;
        }
        if matches!(
            &info.port_type,
            SerialPortType::UsbPort(usb) if usb.vid == PICODROID_VID && usb.pid == PICODROID_PID
        ) {
            vid_pid_ports.push(info);
        } else {
            other_candidates.push(info);
        }
    }

    // Probe VID/PID matches (should respond quickly).
    let mut results: Vec<(String, String)> = vid_pid_ports
        .into_iter()
        .filter_map(|info| probe(&info.port_name).map(|ver| (info.port_name, ver)))
        .collect();

    if !results.is_empty() {
        return results;
    }

    // Fallback: probe remaining candidates in parallel.
    if other_candidates.is_empty() {
        return Vec::new();
    }

    let handles: Vec<_> = other_candidates
        .into_iter()
        .map(|info| {
            thread::spawn(move || probe(&info.port_name).map(|version| (info.port_name, version)))
        })
        .collect();

    results.extend(handles.into_iter().filter_map(|h| h.join().ok().flatten()));
    results
}

/// Try a quick PING on `port_name`. Returns the version string on success.
fn probe(port_name: &str) -> Option<String> {
    let mut port = match serialport::new(port_name, PROBE_BAUD)
        .timeout(PROBE_TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(_) => return None,
    };

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

/// Find the first serial port matching the picodroid VID/PID.
pub fn find_by_vid_pid() -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    ports.into_iter().find_map(|info| {
        if matches!(
            &info.port_type,
            SerialPortType::UsbPort(usb) if usb.vid == PICODROID_VID && usb.pid == PICODROID_PID
        ) {
            Some(info.port_name)
        } else {
            None
        }
    })
}
