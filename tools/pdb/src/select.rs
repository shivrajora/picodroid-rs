// SPDX-License-Identifier: GPL-3.0-only
//! Which device a command talks to, chosen the way adb chooses: `-s
//! <serial>`, `PICODROID_SERIAL`, `-d` (the one USB device), `-e` (the one
//! simulator), or the only one there is (docs/designs/device-serial-2026-10.md
//! §2.6).
//!
//! A device's serial is its USB `iSerialNumber`, so a USB device is found by
//! reading the host's port list — no port is opened, so a neighbour's CDC and
//! the debug probes' UARTs are never written to — and found again, on
//! whatever tty it comes back on, after a reboot. A simulator's serial is
//! `emulator-<pid>`, the pid its default socket is named after. A port path
//! or a socket path is still accepted wherever a serial is.

use serialport::SerialPortType;

use crate::devices::{self, Kind, Row, State};
use crate::transport;

// The device's descriptors are built from these same two constants, so the
// scan and the firmware cannot disagree about what a picodroid looks like.
use pdb_protocol::usb::{PID as PICODROID_PID, VID as PICODROID_VID};

/// The environment variable `ANDROID_SERIAL` is to adb.
pub const SERIAL_ENV: &str = "PICODROID_SERIAL";

/// What a simulator's serial starts with.
pub const EMULATOR_PREFIX: &str = "emulator-";

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Nothing named: the one device or simulator there is.
    Any,
    /// `-d`: the one USB device.
    Usb,
    /// `-e` (or `-s sim`): the one simulator.
    Emulator,
    /// A device serial, from `-s` or [`SERIAL_ENV`].
    Serial(String),
    /// A serial port or a simulator socket, named directly.
    Path(String),
}

impl Selection {
    /// From the global options. `-s` wins over `-d`/`-e`, and any of them
    /// over the environment, as with adb. Two of `-s`/`-d`/`-e` together
    /// are refused.
    pub fn from_options(
        s: Option<&str>,
        d: bool,
        e: bool,
        env: Option<&str>,
        ports: &[PortEntry],
    ) -> Result<Selection, String> {
        if usize::from(s.is_some()) + usize::from(d) + usize::from(e) > 1 {
            return Err("-s, -d and -e are exclusive".into());
        }
        if let Some(s) = s {
            return Ok(Selection::named(s, ports));
        }
        if d {
            return Ok(Selection::Usb);
        }
        if e {
            return Ok(Selection::Emulator);
        }
        match env.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => Ok(Selection::named(v, ports)),
            None => Ok(Selection::Any),
        }
    }

    /// A `-s` value: the word `sim`, a path, a port the host lists by that
    /// name (`COM3` on Windows), or else a serial.
    fn named(s: &str, ports: &[PortEntry]) -> Selection {
        let looks_like_path = s.contains('/') || s.contains('\\') || s.ends_with(".sock");
        if s == transport::SIM {
            Selection::Emulator
        } else if looks_like_path || ports.iter().any(|p| p.name == s) {
            Selection::Path(s.to_owned())
        } else {
            Selection::Serial(s.to_owned())
        }
    }
}

/// A serial port with the picodroid VID/PID, as the host lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortEntry {
    pub name: String,
    /// The USB `iSerialNumber`; `None` for firmware older than
    /// `picodroid/2.4`, which sent none.
    pub serial: Option<String>,
}

/// Every port the host lists with the picodroid VID/PID. macOS lists each
/// CDC twice, as `/dev/cu.*` and `/dev/tty.*`; the `tty.` one blocks on
/// open waiting for carrier, so only `cu.` is kept.
pub fn picodroid_ports() -> Vec<PortEntry> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    ports
        .into_iter()
        .filter(|p| !p.port_name.contains("/tty.") && !p.port_name.contains("Bluetooth"))
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(usb)
                if usb.vid == PICODROID_VID && usb.pid == PICODROID_PID =>
            {
                Some(PortEntry {
                    name: p.port_name,
                    serial: usb.serial_number.filter(|s| usable_serial(s)),
                })
            }
            _ => None,
        })
        .collect()
}

/// The first port with the picodroid VID/PID: how a reboot of firmware
/// too old to report a serial is followed when nothing named the device.
pub fn find_by_vid_pid() -> Option<String> {
    picodroid_ports().into_iter().next().map(|p| p.name)
}

/// Whether a serial identifies one device: not empty and not
/// [`UNKNOWN_SERIAL`](pdb_protocol::greeting::UNKNOWN_SERIAL), which every
/// device without one would share.
pub fn usable_serial(s: &str) -> bool {
    !s.is_empty() && s != pdb_protocol::greeting::UNKNOWN_SERIAL
}

/// The port whose USB serial is `serial`, from a port list.
pub fn port_for_serial(ports: &[PortEntry], serial: &str) -> Option<String> {
    ports
        .iter()
        .find(|p| p.serial.as_deref() == Some(serial))
        .map(|p| p.name.clone())
}

/// A simulator's default socket for an `emulator-<pid>` serial, if that is
/// what `serial` is.
pub fn emulator_socket(serial: &str) -> Option<std::path::PathBuf> {
    let pid = serial.strip_prefix(EMULATOR_PREFIX)?;
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(transport::sim_socket_dir().join(format!("pdb-{pid}.sock")))
}

/// Find a serial without opening any port when it can be found that way:
/// a USB device through the host's port list, a simulator through its
/// default socket name. A simulator on a socket of its own choosing
/// (`PICODROID_SIM_PDB_SOCKET`) is found by asking each listed socket.
/// `None` when nothing answers to it right now.
pub fn find_serial(serial: &str) -> Option<String> {
    if let Some(sock) = emulator_socket(serial) {
        if sock.exists() {
            return Some(sock.to_string_lossy().into_owned());
        }
    }
    if let Some(port) = port_for_serial(&picodroid_ports(), serial) {
        return Some(port);
    }
    devices::scan_sims()
        .into_iter()
        .find(|r| r.serial == serial)
        .map(|r| r.target)
}

/// The port or socket a command should open for `sel`, or the message to
/// print when there is none (or more than one).
pub fn resolve(sel: &Selection) -> Result<String, String> {
    match sel {
        Selection::Path(p) => Ok(p.clone()),
        Selection::Serial(s) => find_serial(s).ok_or_else(|| format!("device '{s}' not found")),
        Selection::Any => one_of(
            devices::scan(),
            "no devices/emulators found",
            "device/emulator",
        ),
        Selection::Usb => one_of(devices::scan_serial(), "no devices found", "device"),
        Selection::Emulator => one_of(devices::scan_sims(), "no emulators found", "emulator"),
    }
}

/// The one answering row's target, as adb picks: none and several are both
/// errors, worded as adb's are; a row that is there but does not answer is
/// reported as offline rather than as missing.
fn one_of(rows: Vec<Row>, none: &str, what: &str) -> Result<String, String> {
    let online: Vec<&Row> = rows.iter().filter(|r| r.state == State::Device).collect();
    match online.as_slice() {
        [one] => Ok(one.target.clone()),
        [] if !rows.is_empty() => Err(format!("{what} offline: {}", rows[0].serial)),
        [] => Err(none.to_owned()),
        many => {
            let list: Vec<String> = many
                .iter()
                .map(|r| match r.kind {
                    Kind::Usb { .. } => format!("  {}", r.serial),
                    Kind::Sim => format!("  {}  (socket {})", r.serial, r.target),
                })
                .collect();
            Err(format!(
                "more than one {what}; pick one with -s <serial>:\n{}",
                list.join("\n")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports() -> Vec<PortEntry> {
        vec![
            PortEntry {
                name: "/dev/ttyACM3".into(),
                serial: Some("E6633861A37B9038".into()),
            },
            PortEntry {
                name: "/dev/ttyACM0".into(),
                serial: None,
            },
            PortEntry {
                name: "COM7".into(),
                serial: Some("68B1A1217192254D".into()),
            },
        ]
    }

    #[test]
    fn a_serial_is_found_by_its_usb_string_on_whatever_tty_it_is() {
        assert_eq!(
            port_for_serial(&ports(), "E6633861A37B9038").as_deref(),
            Some("/dev/ttyACM3")
        );
        assert_eq!(
            port_for_serial(&ports(), "68B1A1217192254D").as_deref(),
            Some("COM7")
        );
        assert_eq!(port_for_serial(&ports(), "E66258881755602D"), None);
    }

    #[test]
    fn s_takes_a_serial_a_path_a_listed_port_or_sim() {
        let p = ports();
        let named = |s: &str| Selection::from_options(Some(s), false, false, None, &p).unwrap();
        assert_eq!(
            named("E6633861A37B9038"),
            Selection::Serial("E6633861A37B9038".into())
        );
        assert_eq!(
            named("/dev/ttyACM3"),
            Selection::Path("/dev/ttyACM3".into())
        );
        assert_eq!(
            named("/tmp/x/pdb.sock"),
            Selection::Path("/tmp/x/pdb.sock".into())
        );
        assert_eq!(named("x.sock"), Selection::Path("x.sock".into()));
        assert_eq!(named("COM7"), Selection::Path("COM7".into()));
        assert_eq!(named("sim"), Selection::Emulator);
        assert_eq!(
            named("emulator-4242"),
            Selection::Serial("emulator-4242".into())
        );
    }

    #[test]
    fn s_beats_d_and_e_beat_the_environment() {
        let p = ports();
        let pick = |s, d, e, env| Selection::from_options(s, d, e, env, &p);
        assert_eq!(pick(None, false, false, None), Ok(Selection::Any));
        assert_eq!(
            pick(None, false, false, Some("SN1")),
            Ok(Selection::Serial("SN1".into()))
        );
        assert_eq!(pick(None, false, false, Some("  ")), Ok(Selection::Any));
        assert_eq!(pick(None, true, false, Some("SN1")), Ok(Selection::Usb));
        assert_eq!(
            pick(None, false, true, Some("SN1")),
            Ok(Selection::Emulator)
        );
        assert_eq!(
            pick(Some("SN2"), false, false, Some("SN1")),
            Ok(Selection::Serial("SN2".into()))
        );
        assert!(pick(Some("SN2"), true, false, None).is_err());
        assert!(pick(None, true, true, None).is_err());
    }

    #[test]
    fn an_emulator_serial_names_its_default_socket() {
        let sock = emulator_socket("emulator-4242").unwrap();
        assert!(sock.ends_with("picodroid-sim/pdb-4242.sock"), "{sock:?}");
        assert_eq!(emulator_socket("emulator-"), None);
        assert_eq!(emulator_socket("emulator-12a"), None);
        assert_eq!(emulator_socket("E6633861A37B9038"), None);
    }

    #[test]
    fn unknown_and_empty_serials_identify_nothing() {
        assert!(usable_serial("E6633861A37B9038"));
        assert!(!usable_serial(""));
        assert!(!usable_serial(pdb_protocol::greeting::UNKNOWN_SERIAL));
    }

    fn row(serial: &str, target: &str, state: State, kind: Kind) -> Row {
        Row {
            serial: serial.into(),
            target: target.into(),
            state,
            kind,
            info: None,
        }
    }

    fn usb() -> Kind {
        Kind::Usb { usb_path: None }
    }

    #[test]
    fn one_device_is_picked_none_and_several_are_errors_in_adbs_words() {
        let a = row("A", "/dev/ttyACM1", State::Device, usb());
        let b = row("B", "/dev/ttyACM2", State::Device, usb());
        let off = row("C", "/dev/ttyACM3", State::Offline, usb());
        assert_eq!(
            one_of(vec![a.clone(), off.clone()], "none", "device"),
            Ok("/dev/ttyACM1".into())
        );
        assert_eq!(
            one_of(vec![], "no devices found", "device"),
            Err("no devices found".into())
        );
        assert_eq!(
            one_of(vec![off], "no devices found", "device"),
            Err("device offline: C".into())
        );
        let err = one_of(vec![a, b], "none", "device/emulator").unwrap_err();
        assert!(err.starts_with("more than one device/emulator"), "{err}");
        assert!(err.contains("  A\n  B"), "{err}");
    }
}
