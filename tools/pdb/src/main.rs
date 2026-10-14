// SPDX-License-Identifier: GPL-3.0-only

#![forbid(unsafe_code)]

mod devices;
mod input;
mod install;
mod logcat;
mod packages;
mod protocol;
mod select;
mod sysmon;
mod transport;

use std::{env, path::Path, process, time::Duration};

use select::Selection;

const USAGE: &str = "\
Usage: pdb [-s <serial> | -d | -e] <command> [args]

Commands:
  devices [-l]               List devices and running simulators, as adb does:
                             a header, then <serial>\t<state> (device, or
                             offline when listed but silent). -l adds port:,
                             board:, release:, max_papk: and usb:<bus position>
  get-serialno               Print the device's serial (\"unknown\" if it has none)
  wait-for-device            Wait until the device answers (with -s, that one)
  ping                       Ping a picodroid device
  install <file.papk>        Push a PAPK to a picodroid device
  list                       List the installed apps (multi-app firmware)
  uninstall <package>        Erase an installed app and reboot (multi-app firmware)
  sysmon                     Show system monitor stats (heap, tasks, CPU%)
  input <cmd> [args]         Inject input (tap/swipe/keyevent) — see 'pdb input'
  logcat --stdin [opts]      Filter picodroid logs on stdin by tag/level

logcat options:
  --stdin                    Read already-decoded log text from stdin
  --tag <T>                  Keep only lines tagged <T> ([T] or 'T:')
  --level <V|D|I|W|E>        Keep only lines at this level or higher
  e.g.  ./scripts/sim.sh --app foo | pdb logcat --stdin --tag Foo --level W

install options:
  --skip-host-check          Skip the host-side compat pre-flight (HIL test
                             knob — exercises the device-side rejection path)
  --expect-rejected          Invert exit codes: success when the install is
                             rejected, failure when it goes through. Used by
                             HIL install-reject-* test rows.

Options:
  -s <serial> The device with this serial (pdb devices lists them): a board's
              USB serial number, or emulator-<pid> for a simulator
  -s <port>   A serial port (e.g. /dev/ttyACM0, /dev/cu.usbmodem1402, COM3)
  -s <socket> A simulator's socket, as ./scripts/sim.sh prints it at boot
              ([sim] pdb: listening on …)
  -d          The one USB device
  -e          The one running simulator (also -s sim)
  With none of these, $PICODROID_SERIAL names the device, else the only one
  attached is used.
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut idx = 0;

    // The global options, before the command, in any order.
    let mut s_opt: Option<String> = None;
    let (mut d_opt, mut e_opt) = (false, false);
    while let Some(arg) = args.get(idx) {
        match arg.as_str() {
            "-s" => {
                idx += 1;
                match args.get(idx) {
                    Some(v) => s_opt = Some(v.clone()),
                    None => {
                        eprintln!("error: -s requires a serial, port or socket argument");
                        process::exit(1);
                    }
                }
            }
            "-d" => d_opt = true,
            "-e" => e_opt = true,
            _ => break,
        }
        idx += 1;
    }
    let env_serial = env::var(select::SERIAL_ENV).ok();
    let selection = Selection::from_options(
        s_opt.as_deref(),
        d_opt,
        e_opt,
        env_serial.as_deref(),
        &select::picodroid_ports(),
    )
    .unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    });
    // A named device: after a reboot, follow that one and never re-scan.
    let explicit = selection != Selection::Any;

    let command = match args.get(idx) {
        Some(c) => {
            idx += 1;
            c.as_str()
        }
        None => {
            eprint!("{USAGE}");
            process::exit(1);
        }
    };

    match command {
        "devices" => match args.get(idx).map(String::as_str) {
            None => devices::run(false),
            Some("-l") => devices::run(true),
            Some(other) => {
                eprintln!("error: unexpected devices argument {other:?}");
                process::exit(1);
            }
        },

        "get-serialno" => {
            let port_name = require_port(&selection);
            let serial = devices::probe(&port_name)
                .map(|(serial, _)| serial)
                .filter(|s| select::usable_serial(s));
            println!(
                "{}",
                serial
                    .as_deref()
                    .unwrap_or(pdb_protocol::greeting::UNKNOWN_SERIAL)
            );
        }

        "wait-for-device" => wait_for_device(&selection),

        "ping" => {
            let port_name = require_port(&selection);
            install::ping(&port_name);
        }

        "sysmon" => {
            let port_name = require_port(&selection);
            sysmon::run(&port_name);
        }

        "list" => {
            let port_name = require_port(&selection);
            packages::list(&port_name);
        }

        "uninstall" => {
            let Some(package) = args.get(idx) else {
                eprintln!("error: uninstall requires a <package> argument");
                process::exit(1);
            };
            let port_name = require_port(&selection);
            packages::uninstall(&port_name, package, explicit);
        }

        "input" => {
            // The remaining args are the input subcommand + its operands.
            let sub_args = args[idx..].to_vec();
            let port_name = require_port(&selection);
            input::run(&port_name, &sub_args);
        }

        "logcat" => {
            // Pure stdin/stdout text filter — no device port needed.
            let code = logcat::run(&args[idx..]);
            if code != 0 {
                process::exit(code);
            }
        }

        "install" => {
            // Parse install-specific flags first; the positional <file.papk>
            // is the remaining non-flag arg.
            let mut opts = install::InstallOptions::default();
            let mut papk_path: Option<std::path::PathBuf> = None;
            while let Some(arg) = args.get(idx) {
                idx += 1;
                match arg.as_str() {
                    "--skip-host-check" => opts.skip_host_check = true,
                    "--expect-rejected" => opts.expect_rejected = true,
                    other if !other.starts_with("--") && papk_path.is_none() => {
                        papk_path = Some(Path::new(other).to_owned());
                    }
                    other => {
                        eprintln!("error: unexpected install argument {other:?}");
                        process::exit(1);
                    }
                }
            }
            let papk_path = match papk_path {
                Some(p) => p,
                None => {
                    eprintln!("error: install requires a <file.papk> argument");
                    process::exit(1);
                }
            };
            opts.explicit_port = explicit;
            let port_name = require_port(&selection);
            install::run(&port_name, &papk_path, opts);
        }

        "--help" | "-h" | "help" => {
            print!("{USAGE}");
        }

        other => {
            eprintln!("error: unknown command '{other}'");
            eprint!("{USAGE}");
            process::exit(1);
        }
    }
}

/// The port or socket the selection names, or exit with adb's message.
fn require_port(selection: &Selection) -> String {
    match select::resolve(selection) {
        Ok(target) => {
            if *selection == Selection::Any {
                eprintln!("auto-detected {target}");
            }
            target
        }
        Err(e) => {
            eprintln!("error: {e}");
            if *selection == Selection::Any {
                eprintln!("       Is the device connected and running picodroid firmware?");
                eprintln!("       (A simulator counts too: ./scripts/sim.sh, then pdb -e.)");
            }
            process::exit(1);
        }
    }
}

/// `wait-for-device`: until the selection answers a PING — with `-s`, that
/// device; otherwise any one, as adb waits. Bounded by the caller
/// (`timeout 30 pdb -s X wait-for-device`), as adb's is.
fn wait_for_device(selection: &Selection) -> ! {
    loop {
        let up = match selection {
            Selection::Any => devices::scan()
                .iter()
                .any(|r| r.state == devices::State::Device),
            Selection::Usb => devices::scan_serial()
                .iter()
                .any(|r| r.state == devices::State::Device),
            Selection::Emulator => !devices::scan_sims().is_empty(),
            Selection::Serial(_) | Selection::Path(_) => select::resolve(selection)
                .ok()
                .is_some_and(|t| devices::probe(&t).is_some()),
        };
        if up {
            process::exit(0);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}
