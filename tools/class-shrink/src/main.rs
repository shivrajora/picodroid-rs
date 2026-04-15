//! class-shrink CLI.
//!
//! M1 subcommands:
//!   class-shrink print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
//!       Prints the active map version (semver or "0.0.0" sentinel) to stdout.
//!
//! Later milestones add `framework`, `app`, and `cut-release` subcommands.

use std::path::PathBuf;
use std::process::ExitCode;

use class_shrink::version;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("{}", USAGE);
        return ExitCode::from(1);
    }
    match args[1].as_str() {
        "print-version" => cmd_print_version(&args[2..]),
        "--help" | "-h" | "help" => {
            println!("{}", USAGE);
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("Error: unknown subcommand '{other}'\n\n{}", USAGE);
            ExitCode::from(1)
        }
    }
}

fn cmd_print_version(args: &[String]) -> ExitCode {
    let mut cargo_toml: Option<PathBuf> = None;
    let mut shrink_maps_dir: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cargo-toml" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("Error: --cargo-toml requires a value");
                    return ExitCode::from(1);
                };
                cargo_toml = Some(PathBuf::from(v));
            }
            "--shrink-maps-dir" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    eprintln!("Error: --shrink-maps-dir requires a value");
                    return ExitCode::from(1);
                };
                shrink_maps_dir = Some(PathBuf::from(v));
            }
            other => {
                eprintln!("Error: unknown flag '{other}'");
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    let cargo_toml = cargo_toml.unwrap_or_else(|| PathBuf::from("Cargo.toml"));
    let shrink_maps_dir = shrink_maps_dir.unwrap_or_else(|| PathBuf::from("sdk/shrink-maps"));

    let pkg_version = match version::read_picodroid_version(&cargo_toml) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };
    let active = version::resolve_active_version(&pkg_version, &shrink_maps_dir);
    println!("{active}");
    ExitCode::SUCCESS
}

const USAGE: &str = "\
class-shrink — Java class/method name shrinker for picodroid

Usage:
  class-shrink print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
      Print the active map version for the current picodroid package version.
      Resolves to the highest committed v<semver>.toml under the maps dir that
      is ≤ the package version, or '0.0.0' if none exists.
";
