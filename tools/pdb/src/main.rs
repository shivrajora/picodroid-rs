mod devices;
mod install;
mod protocol;

use std::{env, path::Path, process};

const USAGE: &str = "\
Usage: pdb [-s <port>] <command> [args]

Commands:
  devices                    List available serial ports
  ping                       Ping a picodroid device
  install <file.papk>        Push a PAPK to a picodroid device

Options:
  -s <port>   Serial port to use (e.g. /dev/cu.usbserial-0001)
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut idx = 0;

    // Parse optional -s <port>
    let mut port: Option<String> = None;
    if args.get(idx).map(|s| s.as_str()) == Some("-s") {
        idx += 1;
        port = Some(match args.get(idx) {
            Some(p) => {
                idx += 1;
                p.clone()
            }
            None => {
                eprintln!("error: -s requires a port argument");
                process::exit(1);
            }
        });
    }

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
        "devices" => devices::run(),

        "ping" => {
            let port_name = require_port(port.as_deref());
            install::ping(&port_name);
        }

        "install" => {
            let papk_path = match args.get(idx) {
                Some(p) => Path::new(p).to_owned(),
                None => {
                    eprintln!("error: install requires a <file.papk> argument");
                    process::exit(1);
                }
            };
            let port_name = require_port(port.as_deref());
            install::run(&port_name, &papk_path);
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

fn require_port(port: Option<&str>) -> String {
    if let Some(p) = port {
        return p.to_owned();
    }

    // Auto-detect: scan for picodroid devices.
    let found = devices::scan();
    match found.len() {
        0 => {
            eprintln!("error: no picodroid devices found");
            eprintln!("       Is the device connected and running picodroid firmware?");
            process::exit(1);
        }
        1 => {
            eprintln!("auto-detected {}", found[0].0);
            found.into_iter().next().unwrap().0
        }
        _ => {
            eprintln!("error: multiple picodroid devices found — use -s to pick one:");
            for (name, version) in &found {
                eprintln!("  {name}  {version}");
            }
            process::exit(1);
        }
    }
}
