// SPDX-License-Identifier: GPL-3.0-only
//! class-shrink CLI.
//!
//! Subcommands:
//!
//!   print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
//!       Print the active map version (semver or "0.0.0" sentinel).
//!
//!   cut-release --classes-dir <dir> --keep <keep.toml> --out <file.toml>
//!                [--base <prev-map.toml>]
//!       Generate a new release map covering every non-kept class under
//!       <classes-dir>. When --base is given, its entries are copied
//!       verbatim and only net-new classes get fresh short names (the
//!       append-only rule). Deterministic: same input → same output.
//!
//!   shrink-dir --in <dir> --out <dir> --map <file.toml>
//!       Rewrite every .class file under --in using --map's classes and
//!       write results under --out. Files without renamed internal names
//!       keep their original name.

use std::path::PathBuf;
use std::process::ExitCode;

use class_shrink::keep::KeepList;
use class_shrink::mapping::ShrinkMap;
use class_shrink::{shrink, version};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("{}", USAGE);
        return ExitCode::from(1);
    }
    match args[1].as_str() {
        "print-version" => cmd_print_version(&args[2..]),
        "cut-release" => cmd_cut_release(&args[2..]),
        "shrink-dir" => cmd_shrink_dir(&args[2..]),
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
    for i in 0..args.len() {
        match args[i].as_str() {
            "--cargo-toml" => cargo_toml = Some(PathBuf::from(args.get(i + 1).expect("value"))),
            "--shrink-maps-dir" => {
                shrink_maps_dir = Some(PathBuf::from(args.get(i + 1).expect("value")))
            }
            _ => {}
        }
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

fn cmd_cut_release(args: &[String]) -> ExitCode {
    let mut classes_dir: Option<PathBuf> = None;
    let mut keep_path: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut base_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--classes-dir" => {
                classes_dir = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--keep" => {
                keep_path = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--out" => {
                out_path = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--base" => {
                base_path = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            _ => {
                eprintln!("Error: unknown flag '{}'", args[i]);
                return ExitCode::from(1);
            }
        }
    }
    let classes_dir = match classes_dir {
        Some(p) => p,
        None => {
            eprintln!("Error: --classes-dir is required");
            return ExitCode::from(1);
        }
    };
    let out_path = match out_path {
        Some(p) => p,
        None => {
            eprintln!("Error: --out is required");
            return ExitCode::from(1);
        }
    };
    let keep = match keep_path {
        Some(p) => match KeepList::load(&p) {
            Ok(k) => k,
            Err(e) => {
                eprintln!("Error loading keep list {}: {e}", p.display());
                return ExitCode::from(1);
            }
        },
        None => KeepList::default(),
    };
    let base = match base_path {
        Some(p) => match ShrinkMap::load(&p) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Error loading base map {}: {e}", p.display());
                return ExitCode::from(1);
            }
        },
        None => ShrinkMap::new(),
    };
    let map = match shrink::cut_release(&classes_dir, &keep, base) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error cutting release: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = map.save(&out_path) {
        eprintln!("Error saving map: {e}");
        return ExitCode::from(1);
    }
    eprintln!(
        "Cut release map with {} classes → {}",
        map.classes.len(),
        out_path.display()
    );
    ExitCode::SUCCESS
}

fn cmd_shrink_dir(args: &[String]) -> ExitCode {
    let mut in_dir: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut map_path: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--in" => {
                in_dir = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--out" => {
                out_dir = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--map" => {
                map_path = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            _ => {
                eprintln!("Error: unknown flag '{}'", args[i]);
                return ExitCode::from(1);
            }
        }
    }
    let in_dir = match in_dir {
        Some(p) => p,
        None => {
            eprintln!("Error: --in is required");
            return ExitCode::from(1);
        }
    };
    let out_dir = match out_dir {
        Some(p) => p,
        None => {
            eprintln!("Error: --out is required");
            return ExitCode::from(1);
        }
    };
    let map_path = match map_path {
        Some(p) => p,
        None => {
            eprintln!("Error: --map is required");
            return ExitCode::from(1);
        }
    };
    let map = match ShrinkMap::load(&map_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading map: {e}");
            return ExitCode::from(1);
        }
    };
    match shrink::shrink_directory(&in_dir, &out_dir, &map) {
        Ok(n) => {
            eprintln!("Shrunk {n} class files → {}", out_dir.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error shrinking: {e}");
            ExitCode::from(1)
        }
    }
}

const USAGE: &str = "\
class-shrink — Java class-name shrinker for picodroid

Subcommands:

  print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
      Print the active map version for the current picodroid package.

  cut-release --classes-dir <dir> --keep <keep.toml> --out <file.toml>
              [--base <prev-map.toml>]
      Generate a release map covering non-kept classes. Append-only
      when --base is provided (existing entries are preserved).

  shrink-dir --in <dir> --out <dir> --map <file.toml>
      Rewrite every .class file under --in using --map's classes,
      writing results under --out at their new internal names.
";
