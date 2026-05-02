// SPDX-License-Identifier: GPL-3.0-only
use std::path::Path;
use std::process;
use std::time::Duration;

use std::io::Write;

use crate::devices::find_by_vid_pid;
use crate::papk_meta;
use crate::protocol::{
    recv_response, send_frame, send_install_data, send_install_header, status_str, CMD_PING,
    INSTALL_PEEK_BYTES, POLL_ATTEMPTS, POLL_TIMEOUT, STATUS_INCOMPAT, STATUS_OK, STATUS_READY,
};

const BAUD_RATE: u32 = 115_200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for the STATUS_READY response: device needs ~10-15 s to erase 1 MB of flash.
const ERASE_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for the STATUS_OK response after streaming the full PAPK.
/// USB CDC is much faster than 115200 baud UART; 10 s is plenty of margin.
const STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// Initial delay before polling PING after STATUS_OK.
/// Covers JVM graceful exit + MCU reboot + USB re-enumeration (~500 ms).
const REBOOT_DELAY: Duration = Duration::from_secs(4);

/// Legacy greeting from firmware that predates the
/// `framework-map-version` advertisement. Hard-refused — the user must
/// reflash via SWD before pdb can guarantee install compatibility.
const LEGACY_GREETING: &str = "picodroid/2.0";

/// Behavior knobs for `install::run`. Mostly used by HIL tests.
#[derive(Default)]
pub struct InstallOptions {
    /// Bypass the host-side compat pre-flight so the device's own check
    /// is exercised (HIL test for the device-side rejection path).
    pub skip_host_check: bool,
    /// Invert exit codes: success when the install is rejected, failure
    /// when it actually goes through. Used by HIL `install-reject-*` rows.
    pub expect_rejected: bool,
}

/// What we learned about the device from its PING greeting.
struct DeviceInfo {
    version: String,
    max_papk: usize,
    framework_map_version: String,
}

/// Parse the new (picodroid/2.1) PING greeting payload.
///
/// Layout:
///   [14] version-string sentinel ("picodroid/2.1\0")
///   [4]  max_papk_bytes (u32 LE)
///   [1]  framework_map_version_len
///   [N]  framework-map-version bytes
fn parse_ping_payload(payload: &[u8]) -> Result<DeviceInfo, String> {
    if payload.len() < 18 {
        return Err(format!("PING payload too short ({} bytes)", payload.len()));
    }
    let version = std::str::from_utf8(&payload[..14])
        .unwrap_or("?")
        .trim_end_matches('\0')
        .to_string();
    let max_papk =
        u32::from_le_bytes([payload[14], payload[15], payload[16], payload[17]]) as usize;

    if version == LEGACY_GREETING {
        return Err(format!(
            "Firmware advertises {LEGACY_GREETING:?}, which predates the \
             framework-map-version protocol field.\n\
             pdb cannot verify install compatibility against this firmware.\n\
             Reflash firmware via SWD (./scripts/flash.sh) to install over USB."
        ));
    }
    if !version.starts_with("picodroid/") {
        return Err(format!("unrecognized firmware greeting: {version:?}"));
    }

    if payload.len() < 19 {
        return Err("PING payload missing framework_map_version field".into());
    }
    let fmv_len = payload[18] as usize;
    if payload.len() < 19 + fmv_len {
        return Err("PING payload truncated in framework_map_version".into());
    }
    let fmv = std::str::from_utf8(&payload[19..19 + fmv_len])
        .map_err(|e| format!("framework_map_version is not UTF-8: {e}"))?
        .to_string();

    Ok(DeviceInfo {
        version,
        max_papk,
        framework_map_version: fmv,
    })
}

/// Print a uniform "refusing to install" message and exit per `opts`.
fn refuse(reason: &str, opts: &InstallOptions) -> ! {
    eprintln!("Refusing to install: {reason}");
    if opts.expect_rejected {
        // HIL "install-reject-*" path: rejection IS success.
        process::exit(0);
    }
    process::exit(1);
}

pub fn run(port_name: &str, papk_path: &Path, opts: InstallOptions) {
    // ── Read PAPK file ────────────────────────────────────────────────────────
    let papk = match std::fs::read(papk_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", papk_path.display());
            process::exit(1);
        }
    };

    // ── Open serial port ──────────────────────────────────────────────────────
    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(CONNECT_TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    // ── PING — identify device, get max PAPK size + framework_map_version ───
    if let Err(e) = send_frame(port.as_mut(), CMD_PING, b"") {
        eprintln!("error: PING send failed: {e}");
        process::exit(1);
    }
    let (status, ping_payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: PING response failed: {e}");
            eprintln!("       Is the device connected and running picodroid firmware?");
            process::exit(1);
        }
    };
    if status != STATUS_OK {
        eprintln!("error: PING returned {}", status_str(status));
        process::exit(1);
    }
    let device = match parse_ping_payload(&ping_payload) {
        Ok(d) => d,
        Err(msg) => refuse(&msg, &opts),
    };

    println!(
        "Connected: {}  (max PAPK: {} KB, framework-map-version: {})",
        device.version,
        device.max_papk / 1024,
        device.framework_map_version,
    );

    // ── Validate size ─────────────────────────────────────────────────────────
    if papk.len() > device.max_papk {
        eprintln!(
            "error: PAPK is {} KB but device supports max {} KB",
            papk.len().div_ceil(1024),
            device.max_papk / 1024,
        );
        process::exit(1);
    }

    // ── Structural validation ────────────────────────────────────────────────
    // Unconditional: reject a garbled / truncated PAPK before touching flash.
    // Must run independently of --skip-host-check because --skip-host-check is
    // meant to bypass *compat* arithmetic (so the device-side reject path can
    // be exercised); it is NOT licence to stream random bytes to the device.
    // Without this, a stub file (e.g. 100 bytes) in no-shrink mode slipped
    // through: read_framework_map_version returned None, compat::check saw
    // None vs firmware 0.0.0 and accepted, and the stub got written to flash —
    // bricking the device on next boot.
    if let Err(e) = papk_meta::validate_structure(&papk) {
        refuse(&format!("PAPK file is not a valid PAPK: {e}"), &opts);
    }

    // ── Pre-flight compat check (host-side) ──────────────────────────────────
    let papk_fmv = papk_meta::read_framework_map_version(&papk);
    if !opts.skip_host_check {
        if let Err(e) = compat::check(papk_fmv, &device.framework_map_version) {
            let reason = format!(
                "PAPK is incompatible with running firmware.\n\
                  PAPK     framework-map-version = {}\n\
                  Firmware framework-map-version = {}\n\
                  Reason: {}\n\
                  Rebuild the PAPK with matching --shrink setting (see docs/shrinker.md).",
                papk_fmv.unwrap_or("(none)"),
                device.framework_map_version,
                match e {
                    compat::CompatError::Mismatch =>
                        "version mismatch (asymmetric --shrink, or PAPK newer than firmware)",
                    compat::CompatError::Missing =>
                        "PAPK predates the framework-map-version manifest key",
                    compat::CompatError::BadVersion => "unparseable version string",
                },
            );
            refuse(&reason, &opts);
        }
    }

    println!(
        "Installing {} ({} KB)...",
        papk_path.display(),
        papk.len().div_ceil(1024)
    );

    // ── Phase A: send install header + inline peek, wait for READY/INCOMPAT ──
    //
    // Protocol: the device runs a pre-erase compat check on the first
    // `INSTALL_PEEK_BYTES` of the PAPK. We must send those bytes inline
    // right after the header, otherwise the device's read blocks
    // indefinitely (it's parked, not draining USB).
    port.set_timeout(ERASE_TIMEOUT).ok();

    if let Err(e) = send_install_header(port.as_mut(), papk.len() as u32) {
        eprintln!("error: INSTALL header send failed: {e}");
        process::exit(1);
    }
    let peek_len = papk.len().min(INSTALL_PEEK_BYTES);
    if let Err(e) = port.write_all(&papk[..peek_len]).and_then(|_| port.flush()) {
        eprintln!("error: INSTALL peek send failed: {e}");
        process::exit(1);
    }

    println!("Erasing flash (~10-15 s)...");

    let (status, payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: waiting for READY: {e}");
            process::exit(1);
        }
    };

    if status == STATUS_INCOMPAT {
        // Device-side compat check fired (e.g. our host check was bypassed
        // via --skip-host-check, or this binary is missing the host check).
        // Existing PAPK on flash is intact; this is a clean refusal.
        let msg = String::from_utf8_lossy(&payload);
        let reason = format!(
            "device rejected install: STATUS_INCOMPAT — {msg}\n\
              PAPK     framework-map-version = {}\n\
              Firmware framework-map-version = {}\n\
              Rebuild the PAPK with matching --shrink setting (see docs/shrinker.md).",
            papk_fmv.unwrap_or("(none)"),
            device.framework_map_version,
        );
        refuse(&reason, &opts);
    }

    if status != STATUS_READY {
        let msg = String::from_utf8_lossy(&payload);
        eprintln!("error: expected READY, got {} — {msg}", status_str(status));
        process::exit(1);
    }

    // ── Phase B: stream remaining PAPK bytes + CRC32 ─────────────────────────
    //
    // The first `peek_len` bytes were already sent inline in Phase A; the
    // device buffered them and will replay them through its CRC hasher
    // alongside the bytes we send here. The host CRC must still cover the
    // full PAPK to match.
    port.set_timeout(STREAM_TIMEOUT).ok();

    println!("Streaming {} KB...", papk.len().div_ceil(1024));

    if let Err(e) = send_install_data(port.as_mut(), papk.len() as u32, &papk, &papk[peek_len..]) {
        eprintln!("error: INSTALL data send failed: {e}");
        process::exit(1);
    }

    let (status, payload) = match recv_response(port.as_mut()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: INSTALL response failed: {e}");
            process::exit(1);
        }
    };

    if status != STATUS_OK {
        let msg = String::from_utf8_lossy(&payload);
        eprintln!("error: INSTALL returned {} — {msg}", status_str(status));
        process::exit(1);
    }

    println!("PAPK written. Waiting for device to reboot...");

    // ── Poll PING until device comes back after reboot ────────────────────────
    //
    // After reboot the USB CDC device disconnects and re-enumerates.
    // Drop the old port, wait for the VID/PID to reappear, then re-open.
    drop(port);
    std::thread::sleep(REBOOT_DELAY);

    for attempt in 0..POLL_ATTEMPTS {
        std::thread::sleep(POLL_TIMEOUT);

        // Try to find the device by VID/PID first (fast).
        let port_name = match find_by_vid_pid() {
            Some(name) => name,
            None => {
                if attempt == 0 {
                    // First attempt — USB may not have re-enumerated yet.
                    continue;
                }
                // Fall back to the original port name.
                port_name.to_string()
            }
        };

        let mut port = match serialport::new(&port_name, BAUD_RATE)
            .timeout(POLL_TIMEOUT)
            .open()
        {
            Ok(p) => p,
            Err(_) => continue,
        };

        if send_frame(port.as_mut(), CMD_PING, b"").is_err() {
            continue;
        }

        match recv_response(port.as_mut()) {
            Ok((STATUS_OK, _)) => {
                if opts.expect_rejected {
                    // We told the user to expect rejection, but the install
                    // actually went through. That's a test failure.
                    eprintln!(
                        "error: --expect-rejected was set but install completed successfully"
                    );
                    process::exit(1);
                }
                println!("Install complete.");
                return;
            }
            _ => continue,
        }
    }

    eprintln!("warning: device did not respond to PING within 20 s after reboot.");
    eprintln!("         The install was written to flash; the app will load on next boot.");
}

pub fn ping(port_name: &str) {
    let mut port = match serialport::new(port_name, BAUD_RATE)
        .timeout(CONNECT_TIMEOUT)
        .open()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot open {port_name}: {e}");
            process::exit(1);
        }
    };

    if let Err(e) = send_frame(port.as_mut(), CMD_PING, b"") {
        eprintln!("error: PING send failed: {e}");
        process::exit(1);
    }

    match recv_response(port.as_mut()) {
        Ok((STATUS_OK, payload)) => match parse_ping_payload(&payload) {
            Ok(d) => println!(
                "{}  (max PAPK: {} KB, framework-map-version: {})",
                d.version,
                d.max_papk / 1024,
                d.framework_map_version,
            ),
            Err(e) => {
                eprintln!("error: {e}");
                process::exit(1);
            }
        },
        Ok((status, _)) => {
            eprintln!("error: PING returned {}", status_str(status));
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    }
}
