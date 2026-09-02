// SPDX-License-Identifier: GPL-3.0-only
//! class-shrink CLI.
//!
//! Subcommands:
//!
//!   print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
//!       Print the active map version (semver or "0.0.0" sentinel).
//!
//!   cut-release --classes-dir <dir> --keep <keep.toml> --out <file.toml>
//!                [--base <prev-map.toml>] [--extra-names <file>]...
//!                [--members --version <semver> --keep-contract <tsv>
//!                 [--reserve <dir>]...]
//!       Generate a new release map covering every non-kept class under
//!       <classes-dir> (allocated under `a/`) plus every `java/**` name
//!       those classes reference (allocated under `b/`, which pico-jvm
//!       reverse-translates). --extra-names adds names from a text file —
//!       one per line, or tab-separated rows such as sdk/api-contract.tsv,
//!       which lists every java/** class pico-jvm serves. When --base is
//!       given, its entries are copied verbatim and only net-new names get
//!       fresh short names (the append-only rule). Deterministic: same
//!       input → same output.
//!       --members also allocates [[member]] targets for the framework's
//!       method and field names: --keep-contract's member column is never
//!       mapped, --reserve trees (the kotlin-shim) never yield a target, and
//!       --version becomes `member-floor` on the first member cut.
//!
//!   shrink-dir --in <dir> --out <dir> --map <file.toml>
//!       Rewrite every .class file under --in using --map's classes and
//!       write results under --out. Files without renamed internal names
//!       keep their original name.
//!
//!   verify <map.toml> [<map.toml> ...]
//!       Check each map is a 1:1 original → shrunk mapping (no duplicate
//!       shrunk names). Exits non-zero and lists collisions on failure.

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
        "verify" => cmd_verify(&args[2..]),
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
    let mut extra_paths: Vec<PathBuf> = Vec::new();
    let mut members = false;
    let mut reserve_dirs: Vec<PathBuf> = Vec::new();
    let mut contract_path: Option<PathBuf> = None;
    let mut cut_version: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--members" => {
                members = true;
                i += 1;
            }
            "--reserve" => {
                reserve_dirs.push(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--keep-contract" => {
                contract_path = Some(PathBuf::from(args.get(i + 1).expect("value")));
                i += 2;
            }
            "--version" => {
                cut_version = Some(args.get(i + 1).expect("value").to_string());
                i += 2;
            }
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
            "--extra-names" => {
                extra_paths.push(PathBuf::from(args.get(i + 1).expect("value")));
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
    let mut extra_names: Vec<String> = Vec::new();
    for p in &extra_paths {
        match shrink::read_extra_names(p) {
            Ok(names) => extra_names.extend(names),
            Err(e) => {
                eprintln!("Error reading --extra-names {}: {e}", p.display());
                return ExitCode::from(1);
            }
        }
    }
    let mut map = match shrink::cut_release(&classes_dir, &keep, &extra_names, base) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error cutting release: {e}");
            return ExitCode::from(1);
        }
    };
    if members {
        let Some(version) = cut_version else {
            eprintln!("Error: --members requires --version <semver>");
            return ExitCode::from(1);
        };
        let contract_names = match &contract_path {
            Some(p) => match shrink::read_contract_member_names(p) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("Error reading --keep-contract {}: {e}", p.display());
                    return ExitCode::from(1);
                }
            },
            None => {
                eprintln!("Error: --members requires --keep-contract <sdk/api-contract.tsv>");
                return ExitCode::from(1);
            }
        };
        let opts = shrink::MemberCut {
            reserve_dirs: &reserve_dirs,
            contract_names: &contract_names,
            version: &version,
        };
        if let Err(e) = shrink::cut_release_members(&classes_dir, &keep, &opts, &mut map) {
            eprintln!("Error cutting member map: {e}");
            return ExitCode::from(1);
        }
    }
    if let Err(e) = map.save(&out_path) {
        eprintln!("Error saving map: {e}");
        return ExitCode::from(1);
    }
    eprintln!(
        "Cut release map with {} classes, {} members → {}",
        map.classes.len(),
        map.members.len(),
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

fn cmd_verify(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: verify needs at least one map file\n\n{USAGE}");
        return ExitCode::from(1);
    }
    let mut failed = false;
    for arg in args {
        let path = PathBuf::from(arg);
        let map = match ShrinkMap::load(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Error loading {}: {e}", path.display());
                failed = true;
                continue;
            }
        };
        match map.verify_injective() {
            Ok(()) => eprintln!(
                "ok: {} ({} classes, {} members, 1:1)",
                path.display(),
                map.classes.len(),
                map.members.len()
            ),
            Err(e) => {
                eprintln!("FAIL: {}: {e}", path.display());
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

const USAGE: &str = "\
class-shrink — Java class/member-name shrinker for picodroid

Subcommands:

  print-version [--cargo-toml <path>] [--shrink-maps-dir <dir>]
      Print the active map version for the current picodroid package.

  cut-release --classes-dir <dir> --keep <keep.toml> --out <file.toml>
              [--base <prev-map.toml>] [--extra-names <file>]...
              [--members --version <semver> --keep-contract <tsv>
               [--reserve <dir>]...]
      Generate a release map covering non-kept classes (a/) and the
      java/** names they reference (b/). --extra-names adds names from
      a text file (sdk/api-contract.tsv works as-is). Append-only when
      --base is provided (existing entries are preserved).
      --members also maps the framework's method/field names: the
      --keep-contract member column is never mapped, --reserve trees
      (the kotlin-shim) never yield a target, --version becomes the
      member-floor on the first member cut.

  shrink-dir --in <dir> --out <dir> --map <file.toml>
      Rewrite every .class file under --in using --map's classes,
      writing results under --out at their new internal names. Member
      renames are applied by the Gradle ShrinkMembersTask (ASM), not here.

  verify <map.toml> [<map.toml> ...]
      Check each map is a 1:1 original -> shrunk mapping (no duplicate
      shrunk names). Exits non-zero and lists collisions on failure.
";
