// SPDX-License-Identifier: GPL-3.0-only
use std::path::Path;
use std::process;
use std::time::Duration;

use std::io::Write;

use crate::devices::find_by_vid_pid;
use crate::protocol::{
    recv_response, send_frame, send_install_data, send_install_header, status_str, CMD_PING,
    INSTALL_PEEK_BYTES, POLL_ATTEMPTS, POLL_TIMEOUT, STATUS_INCOMPAT, STATUS_NO_ROOM, STATUS_OK,
    STATUS_READY,
};
use pdb_protocol::greeting::{AppsInfo, Greeting, GreetingError, LEGACY_VERSION, VERSION_PREFIX};

const BAUD_RATE: u32 = 115_200;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for the STATUS_READY response: the device places the run, may
/// compact the app region first (up to ~30 s for a full 1.5 MB region), and
/// erases the run's sectors.
const ERASE_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for the STATUS_OK response after streaming the full PAPK.
/// USB CDC is much faster than 115200 baud UART; 10 s is plenty of margin.
const STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// Initial delay before polling PING after STATUS_OK.
/// Covers JVM graceful exit + MCU reboot + USB re-enumeration (~500 ms).
const REBOOT_DELAY: Duration = Duration::from_secs(4);

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
pub struct DeviceInfo {
    pub version: String,
    pub max_papk: usize,
    pub framework_map_version: String,
    /// The package directory, on multi-app firmware.
    pub apps: Option<AppsInfo>,
}

impl DeviceInfo {
    /// `apps 3/8, free 1428 KB (largest 1024 KB)` on multi-app firmware,
    /// `single-app` otherwise.
    pub fn apps_summary(&self) -> String {
        match &self.apps {
            Some(a) => format!(
                "apps {}/{}, free {} KB (largest {} KB)",
                a.installed,
                a.max,
                a.total_free / 1024,
                a.largest_free / 1024
            ),
            None => "single-app".to_string(),
        }
    }
}

/// Interpret a PING greeting. The layout lives in `pdb_protocol::greeting`
/// (shared with the firmware's encoder); what stays here is policy — which
/// versions are refused, and with what message.
pub fn parse_ping_payload(payload: &[u8]) -> Result<DeviceInfo, String> {
    let g = Greeting::parse(payload).map_err(|e| match e {
        GreetingError::TooShort(n) => format!("PING payload too short ({n} bytes)"),
        GreetingError::MissingFmv => "PING payload missing framework_map_version field".into(),
        GreetingError::TruncatedFmv => "PING payload truncated in framework_map_version".into(),
        GreetingError::FmvNotUtf8 => "framework_map_version is not UTF-8".into(),
    })?;

    if g.is_legacy() {
        // Hard-refused — the user must reflash via SWD before pdb can
        // guarantee install compatibility.
        return Err(format!(
            "Firmware advertises {LEGACY_VERSION:?}, which predates the \
             framework-map-version protocol field.\n\
             pdb cannot verify install compatibility against this firmware.\n\
             Reflash firmware via SWD (./scripts/flash.sh) to install over USB."
        ));
    }
    if !g.version.starts_with(VERSION_PREFIX) {
        return Err(format!("unrecognized firmware greeting: {:?}", g.version));
    }

    Ok(DeviceInfo {
        version: g.version.to_string(),
        max_papk: g.max_papk as usize,
        framework_map_version: g.framework_map_version.to_string(),
        apps: g.apps,
    })
}

/// PING the device at `port_name` and interpret the greeting.
pub fn query_device(port_name: &str, timeout: Duration) -> Result<DeviceInfo, String> {
    let mut port = serialport::new(port_name, BAUD_RATE)
        .timeout(timeout)
        .open()
        .map_err(|e| format!("cannot open {port_name}: {e}"))?;
    send_frame(port.as_mut(), CMD_PING, b"").map_err(|e| format!("PING send failed: {e}"))?;
    let (status, payload) = recv_response(port.as_mut()).map_err(|e| {
        format!("PING response failed: {e}\n       Is the device connected and running picodroid firmware?")
    })?;
    if status != STATUS_OK {
        return Err(format!("PING returned {}", status_str(status)));
    }
    parse_ping_payload(&payload)
}

/// After a reset, wait for the device to re-enumerate and answer a PING.
/// Returns `false` if it never does within the poll budget.
pub fn wait_for_reboot(port_name: &str) -> bool {
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
        if let Ok((STATUS_OK, _)) = recv_response(port.as_mut()) {
            return true;
        }
    }
    false
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
        "Connected: {}  (max PAPK: {} KB, framework-map-version: {}, {})",
        device.version,
        device.max_papk / 1024,
        device.framework_map_version,
        device.apps_summary(),
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
    if let Err(e) = papk_format::validate_structure(&papk) {
        refuse(&format!("PAPK file is not a valid PAPK: {e}"), &opts);
    }

    // ── Pre-flight compat check (host-side) ──────────────────────────────────
    let papk_fmv =
        papk_format::find_manifest_value(&papk, papk_format::keys::FRAMEWORK_MAP_VERSION);
    if !opts.skip_host_check {
        if let Err(e) = compat::check(papk_fmv, &device.framework_map_version) {
            let reason = format!(
                "PAPK is incompatible with running firmware.\n\
                  PAPK     framework-map-version = {}\n\
                  Firmware framework-map-version = {}\n\
                  Reason: {}\n\
                  Rebuild the PAPK with matching --shrink setting (see reference/shrinker in the docs).",
                papk_fmv.unwrap_or("(none)"),
                device.framework_map_version,
                match e {
                    compat::CompatError::Mismatch =>
                        "version mismatch (asymmetric --shrink, or PAPK newer than firmware)",
                    compat::CompatError::Missing =>
                        "PAPK predates the framework-map-version manifest key",
                    compat::CompatError::PredatesMemberShrink =>
                        "PAPK was shrunk before method/field names were (map < member floor); rebuild it",
                    compat::CompatError::BadVersion => "unparseable version string",
                },
            );
            refuse(&reason, &opts);
        }
    }

    // ── Package identity (multi-app firmware) ────────────────────────────
    // The device places the run by `package-name`; a PAPK without one
    // cannot be placed, and the device would refuse it after parking the
    // JVM. Refuse here, before it does.
    let package = papk_format::find_manifest_value(&papk, papk_format::keys::PACKAGE_NAME);
    if device.apps.is_some() && package.is_none() {
        refuse(
            "PAPK has no package-name; a multi-app device cannot place it.\n  \
             Repack it: papk-pack --repack <file.papk> --package-name <name> --output <new.papk>",
            &opts,
        );
    }

    println!(
        "Installing {} ({}, {} KB)...",
        papk_path.display(),
        package.unwrap_or("no package-name"),
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

    match &device.apps {
        Some(_) => {
            println!("Placing the app and erasing flash (compaction can take up to a minute)...")
        }
        None => println!("Erasing flash (~10-15 s)..."),
    }

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
              Rebuild the PAPK with matching --shrink setting (see reference/shrinker in the docs).",
            papk_fmv.unwrap_or("(none)"),
            device.framework_map_version,
        );
        refuse(&reason, &opts);
    }

    if status == STATUS_NO_ROOM {
        // The region has no contiguous run for it even after compaction, or
        // the directory is full. Nothing was erased; every installed app is
        // intact.
        let msg = String::from_utf8_lossy(&payload);
        let reason = format!(
            "device rejected install: STATUS_NO_ROOM — {msg}\n  \
             Free room with `pdb uninstall <package>`; `pdb list` shows what is installed."
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
    if wait_for_reboot(port_name) {
        if opts.expect_rejected {
            // We told the user to expect rejection, but the install
            // actually went through. That's a test failure.
            eprintln!("error: --expect-rejected was set but install completed successfully");
            process::exit(1);
        }
        println!("Install complete.");
        return;
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
                "{}  (max PAPK: {} KB, framework-map-version: {}, {})",
                d.version,
                d.max_papk / 1024,
                d.framework_map_version,
                d.apps_summary(),
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
