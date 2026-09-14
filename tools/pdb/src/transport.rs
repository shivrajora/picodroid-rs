// SPDX-License-Identifier: GPL-3.0-only
//! Where the bytes go: a serial port for a device, a Unix socket for a
//! simulator.
//!
//! The framing in `protocol` never cared — it reads and writes `dyn Read` /
//! `dyn Write` — but every command used to open its own `serialport`. This
//! is the one opener, and [`Link`] is the one thing a command may ask of what
//! it opened beyond reading and writing: a timeout.
//!
//! # Targets
//!
//! `-s` takes a serial port name (`/dev/ttyACM0`, `/dev/cu.usbmodem1402`),
//! a simulator's socket path, or the word `sim`, which `main` resolves to the
//! one simulator that is running. A path is a simulator's when it exists and
//! is a Unix socket, or when it does not exist (yet, or any more) and ends in
//! `.sock` — a simulator between its reboot's exec and its rebind, which
//! `install::wait_for_reboot` polls exactly as it polls a re-enumerating
//! device. Every simulator listens under [`sim_socket_dir`] as
//! `pdb-<pid>.sock` unless told otherwise, so several can run at once and
//! `pdb devices` finds them all (`crates/picodroid-core/src/hal/sim/pdb.rs`).

use std::io::{self, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The device's CDC line coding, from `pdb_protocol::usb`'s descriptor set.
pub const BAUD_RATE: u32 = 115_200;

/// The `-s` word that means "the running simulator".
pub const SIM: &str = "sim";

/// What a simulator's socket file is named like.
const SOCKET_SUFFIX: &str = ".sock";

/// An open byte pipe to a device or a simulator.
pub trait Link: Read + Write + Send {
    /// How long a read or write may block before it fails with a timeout.
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()>;
}

impl Link for Box<dyn serialport::SerialPort> {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        serialport::SerialPort::set_timeout(self.as_mut(), timeout).map_err(io::Error::from)
    }
}

impl Link for UnixStream {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.set_read_timeout(Some(timeout))?;
        self.set_write_timeout(Some(timeout))
    }
}

/// What a `-s` value names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A serial port, opened at [`BAUD_RATE`].
    Serial(String),
    /// A simulator's Unix socket.
    Sim(PathBuf),
}

/// Classify a target string (never `sim` itself — `main` resolves that to
/// a socket path first).
pub fn classify(target: &str) -> Target {
    let path = Path::new(target);
    match std::fs::metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => Target::Sim(path.to_path_buf()),
        Ok(_) => Target::Serial(target.to_owned()),
        // Gone, or not yet there: a simulator mid-reboot keeps its name.
        Err(_) if target.ends_with(SOCKET_SUFFIX) => Target::Sim(path.to_path_buf()),
        Err(_) => Target::Serial(target.to_owned()),
    }
}

/// Whether `target` names a simulator rather than a serial port.
pub fn is_sim(target: &str) -> bool {
    matches!(classify(target), Target::Sim(_))
}

/// Open `target` with `timeout` on every read and write.
pub fn open(target: &str, timeout: Duration) -> io::Result<Box<dyn Link>> {
    match classify(target) {
        Target::Sim(path) => {
            let mut stream = UnixStream::connect(&path)?;
            stream.set_timeout(timeout)?;
            Ok(Box::new(stream))
        }
        Target::Serial(name) => {
            let port = serialport::new(name, BAUD_RATE).timeout(timeout).open()?;
            Ok(Box::new(port))
        }
    }
}

/// The directory simulators listen in: `picodroid-sim` under the platform's
/// temp dir — the same rule `hal/sim/pdb.rs` uses to pick it, so the two
/// agree whenever they run in the same environment (a different `TMPDIR`
/// in another shell hides them; the path the simulator prints at boot,
/// passed to `-s`, always works).
pub fn sim_socket_dir() -> PathBuf {
    std::env::temp_dir().join("picodroid-sim")
}

/// Every simulator socket in [`sim_socket_dir`], sorted. Some may belong to
/// simulators that are gone; `devices` probes and prunes them.
pub fn sim_sockets() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(sim_socket_dir()) else {
        return Vec::new();
    };
    let mut sockets: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("pdb-")
                && name.ends_with(SOCKET_SUFFIX)
                && e.file_type().is_ok_and(|t| t.is_socket())
        })
        .map(|e| e.path())
        .collect();
    sockets.sort();
    sockets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdb-transport-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_bound_socket_is_a_simulator() {
        let path = scratch("live.sock");
        let _ = std::fs::remove_file(&path);
        let _listener = UnixListener::bind(&path).unwrap();
        assert_eq!(classify(path.to_str().unwrap()), Target::Sim(path.clone()));
        assert!(is_sim(path.to_str().unwrap()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_sock_path_is_still_a_simulator_but_other_names_are_serial() {
        let gone = scratch("gone.sock");
        let _ = std::fs::remove_file(&gone);
        assert!(is_sim(gone.to_str().unwrap()));
        assert_eq!(
            classify("/dev/ttyACM0"),
            Target::Serial("/dev/ttyACM0".to_owned())
        );
        assert_eq!(
            classify("/dev/cu.usbmodem1402"),
            Target::Serial("/dev/cu.usbmodem1402".to_owned())
        );
    }

    #[test]
    fn a_regular_file_is_not_a_simulator() {
        let file = scratch("plain.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(!is_sim(file.to_str().unwrap()));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn opening_a_simulator_socket_connects_and_sets_the_timeout() {
        let path = scratch("open.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let mut link = open(path.to_str().unwrap(), Duration::from_millis(50)).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        link.write_all(b"hi").unwrap();
        let mut got = [0u8; 2];
        server.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"hi");
        // Nothing to read: the timeout, not a hang.
        let mut byte = [0u8; 1];
        assert!(link.read(&mut byte).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
