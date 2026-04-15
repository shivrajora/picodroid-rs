//! papk-pack: Packages compiled Java .class files into a PAPK (Picodroid APK) binary file.
//!
//! Usage:
//!   papk-pack \
//!     --main-class helloworld/HelloWorld \
//!     --package-name helloworld \
//!     --version 1.0 \
//!     --classes-dir build/classes/helloworld \
//!     --output build/apks/helloworld.papk

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

// ── PAPK format constants ─────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"PAPK";
const VERSION_MAJOR: u16 = 1;
// Bumped 0 → 1 when the `framework-map-version` manifest key was introduced
// (M1 of class/method name shrinking). Binary layout is unchanged, so
// VERSION_MAJOR stays at 1 and older parsers still walk the file.
const VERSION_MINOR: u16 = 1;

// Section tags (ASCII tag stored as little-endian u32)
// "MANI" = 0x494E414D, "CLSS" = 0x53534C43
const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");

// ── CLI argument parsing ──────────────────────────────────────────────────────

struct Args {
    main_class: Option<String>,
    activity: Option<String>,
    application: Option<String>,
    package_name: String,
    version: String,
    framework_map_version: String,
    classes_dir: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut main_class = None;
    let mut activity = None;
    let mut application = None;
    let mut package_name = None;
    let mut version = None;
    let mut framework_map_version = None;
    let mut classes_dir = None;
    let mut output = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--main-class" => {
                i += 1;
                main_class = Some(args.get(i).ok_or("--main-class requires a value")?.clone());
            }
            "--activity" => {
                i += 1;
                activity = Some(args.get(i).ok_or("--activity requires a value")?.clone());
            }
            "--application" => {
                i += 1;
                application = Some(args.get(i).ok_or("--application requires a value")?.clone());
            }
            "--package-name" => {
                i += 1;
                package_name = Some(
                    args.get(i)
                        .ok_or("--package-name requires a value")?
                        .clone(),
                );
            }
            "--version" => {
                i += 1;
                version = Some(args.get(i).ok_or("--version requires a value")?.clone());
            }
            "--framework-map-version" => {
                i += 1;
                framework_map_version = Some(
                    args.get(i)
                        .ok_or("--framework-map-version requires a value")?
                        .clone(),
                );
            }
            "--classes-dir" => {
                i += 1;
                classes_dir = Some(PathBuf::from(
                    args.get(i).ok_or("--classes-dir requires a value")?,
                ));
            }
            "--output" => {
                i += 1;
                output = Some(PathBuf::from(
                    args.get(i).ok_or("--output requires a value")?,
                ));
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(format!("Unknown argument: {other}"));
            }
        }
        i += 1;
    }

    if main_class.is_none() && activity.is_none() && application.is_none() {
        return Err("either --main-class, --activity, or --application is required".into());
    }

    Ok(Args {
        main_class,
        activity,
        application,
        package_name: package_name.ok_or("--package-name is required")?,
        version: version.ok_or("--version is required")?,
        framework_map_version: framework_map_version
            .ok_or("--framework-map-version is required")?,
        classes_dir: classes_dir.ok_or("--classes-dir is required")?,
        output: output.ok_or("--output is required")?,
    })
}

fn print_usage() {
    eprintln!(
        "Usage: papk-pack \\\n\
         \x20 [--main-class <jvm/ClassName>] \\\n\
         \x20 [--activity <jvm/ClassName>] \\\n\
         \x20 [--application <jvm/ClassName>] \\\n\
         \x20 --package-name <name> \\\n\
         \x20 --version <x.y> \\\n\
         \x20 --framework-map-version <semver> \\\n\
         \x20 --classes-dir <dir> \\\n\
         \x20 --output <file.papk>\n\
         \n\
         At least one of --main-class, --activity, or --application must be provided."
    );
}

// ── Class file discovery ──────────────────────────────────────────────────────

/// Recursively collects all .class files under `dir`.
/// Returns (jvm_name, file_bytes) pairs, where jvm_name uses forward slashes
/// and has no `.class` suffix (e.g. "helloworld/HelloWorld").
fn collect_classes(dir: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut result = Vec::new();
    collect_classes_inner(dir, dir, &mut result)?;
    // Sort for deterministic output order
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}

fn collect_classes_inner(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_classes_inner(root, &path, out)?;
        } else if path.extension().map_or(false, |e| e == "class") {
            let rel = path
                .strip_prefix(root)
                .expect("class file must be under root");
            // Convert OS path separators to forward slashes, strip .class suffix
            let jvm_name = rel.with_extension("").to_string_lossy().replace('\\', "/");
            let bytes = fs::read(&path)?;
            out.push((jvm_name, bytes));
        }
    }
    Ok(())
}

// ── PAPK serialization ────────────────────────────────────────────────────────

fn write_u16_le(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u32_le(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_str_u16(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_u16_le(buf, bytes.len() as u16);
    buf.extend_from_slice(bytes);
}

/// Build the MANIFEST section data (key/value pairs).
fn build_manifest_data(
    main_class: Option<&str>,
    activity: Option<&str>,
    application: Option<&str>,
    package_name: &str,
    version: &str,
    framework_map_version: &str,
) -> Vec<u8> {
    let mut data = Vec::new();
    if let Some(mc) = main_class {
        write_str_u16(&mut data, "main-class");
        write_str_u16(&mut data, mc);
    }
    if let Some(act) = activity {
        write_str_u16(&mut data, "activity");
        write_str_u16(&mut data, act);
    }
    if let Some(app) = application {
        write_str_u16(&mut data, "application");
        write_str_u16(&mut data, app);
    }
    write_str_u16(&mut data, "package-name");
    write_str_u16(&mut data, package_name);
    write_str_u16(&mut data, "version");
    write_str_u16(&mut data, version);
    write_str_u16(&mut data, "framework-map-version");
    write_str_u16(&mut data, framework_map_version);
    data
}

/// Build the CLASSES section data.
fn build_classes_data(classes: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    write_u32_le(&mut data, classes.len() as u32);
    for (name, bytes) in classes {
        write_str_u16(&mut data, name);
        write_u32_le(&mut data, bytes.len() as u32);
        data.extend_from_slice(bytes);
    }
    data
}

/// Build a section header (16 bytes).
fn build_section_header(tag: u32, data_len: u32) -> Vec<u8> {
    let mut hdr = Vec::with_capacity(16);
    write_u32_le(&mut hdr, tag);
    write_u32_le(&mut hdr, data_len);
    write_u32_le(&mut hdr, 0); // crc32: unchecked in v1
    write_u32_le(&mut hdr, 0); // reserved
    hdr
}

fn build_papk(
    main_class: Option<&str>,
    activity: Option<&str>,
    application: Option<&str>,
    package_name: &str,
    version: &str,
    framework_map_version: &str,
    classes: &[(String, Vec<u8>)],
) -> Vec<u8> {
    let manifest_data = build_manifest_data(
        main_class,
        activity,
        application,
        package_name,
        version,
        framework_map_version,
    );
    let classes_data = build_classes_data(classes);

    let manifest_hdr = build_section_header(TAG_MANIFEST, manifest_data.len() as u32);
    let classes_hdr = build_section_header(TAG_CLASSES, classes_data.len() as u32);

    // File header is 24 bytes.
    // MANIFEST section starts immediately after.
    let manifest_offset: u32 = 24;
    let classes_offset: u32 =
        manifest_offset + manifest_hdr.len() as u32 + manifest_data.len() as u32;

    let mut file = Vec::new();

    // File header (24 bytes)
    file.extend_from_slice(MAGIC);
    write_u16_le(&mut file, VERSION_MAJOR);
    write_u16_le(&mut file, VERSION_MINOR);
    write_u32_le(&mut file, 2); // section_count
    write_u32_le(&mut file, manifest_offset);
    write_u32_le(&mut file, classes_offset);
    write_u32_le(&mut file, 0); // reserved

    // MANIFEST section
    file.extend_from_slice(&manifest_hdr);
    file.extend_from_slice(&manifest_data);

    // CLASSES section
    file.extend_from_slice(&classes_hdr);
    file.extend_from_slice(&classes_data);

    file
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {e}");
            print_usage();
            std::process::exit(1);
        }
    };

    if !args.classes_dir.is_dir() {
        eprintln!(
            "Error: --classes-dir '{}' is not a directory",
            args.classes_dir.display()
        );
        std::process::exit(1);
    }

    let classes = match collect_classes(&args.classes_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading classes: {e}");
            std::process::exit(1);
        }
    };

    if classes.is_empty() {
        eprintln!(
            "Warning: no .class files found in '{}'",
            args.classes_dir.display()
        );
    }

    let papk = build_papk(
        args.main_class.as_deref(),
        args.activity.as_deref(),
        args.application.as_deref(),
        &args.package_name,
        &args.version,
        &args.framework_map_version,
        &classes,
    );

    if let Some(parent) = args.output.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Error creating output directory: {e}");
            std::process::exit(1);
        }
    }

    match fs::write(&args.output, &papk) {
        Ok(()) => {
            eprintln!(
                "==> Wrote {} ({} bytes, {} classes)",
                args.output.display(),
                papk.len(),
                classes.len()
            );
        }
        Err(e) => {
            eprintln!("Error writing output: {e}");
            std::process::exit(1);
        }
    }
}
