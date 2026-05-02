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
// "MANI" = 0x494E414D, "CLSS" = 0x53534C43, "ASST" = 0x54535341
const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");
const TAG_ASSETS: u32 = u32::from_le_bytes(*b"ASST");

/// LVGL `lv_color_format_t` value for native RGB565 little-endian. Verified
/// against `vendor/lvgl/src/misc/lv_color_format.h` (`LV_COLOR_FORMAT_RGB565`).
const LV_COLOR_FORMAT_RGB565: u8 = 0x12;

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
    /// Optional directory of image assets to bundle. PNGs are decoded on the
    /// host into LVGL-native RGB565 (little-endian per pixel) and emitted in
    /// the new `ASST` section.
    assets_dir: Option<PathBuf>,
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
    let mut assets_dir: Option<PathBuf> = None;

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
            "--assets-dir" => {
                i += 1;
                assets_dir = Some(PathBuf::from(
                    args.get(i).ok_or("--assets-dir requires a value")?,
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
        assets_dir,
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
         \x20 [--assets-dir <dir>] \\\n\
         \x20 --output <file.papk>\n\
         \n\
         At least one of --main-class, --activity, or --application must be provided.\n\
         --assets-dir is optional; PNG files in the directory are decoded into\n\
         LVGL-native RGB565 and bundled in the ASSETS (ASST) section."
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

// ── Asset discovery and decode ───────────────────────────────────────────────

/// One asset bundled into the ASSETS section.
struct Asset {
    name: String,
    width: u16,
    height: u16,
    cf: u8,
    /// Raw pixel bytes in the format described by `cf`.
    data: Vec<u8>,
}

/// Decode all `*.png` files in `dir` (flat, non-recursive) into LVGL-native
/// RGB565 little-endian-per-pixel buffers.
fn collect_assets(dir: &Path) -> Result<Vec<Asset>, String> {
    let mut out = Vec::new();
    let entries =
        fs::read_dir(dir).map_err(|e| format!("read assets dir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read assets dir entry: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext != "png" {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("asset {} has non-UTF-8 name", path.display()))?
            .to_owned();
        let asset = decode_png_to_rgb565(&path, name)
            .map_err(|e| format!("decode {}: {e}", path.display()))?;
        out.push(asset);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Decode a PNG file into an LVGL-native RGB565 (little-endian per pixel) buffer.
///
/// Per-pixel layout: `[low_byte, high_byte]` of the 16-bit value
/// `(R5 << 11) | (G6 << 5) | B5`. The framebuffer's `LV_COLOR_16_SWAP=1`
/// configuration handles the eventual byte swap on the SPI write to the
/// ST7789, so the source data stays in the standard little-endian form
/// LVGL refers to as `LV_COLOR_FORMAT_RGB565`.
fn decode_png_to_rgb565(path: &Path, name: String) -> Result<Asset, String> {
    let img = image::open(path).map_err(|e| format!("{e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 {
        return Err("zero-sized image".into());
    }
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return Err(format!("image too large ({w}x{h}); max 65535x65535"));
    }
    let mut buf = Vec::with_capacity((w as usize) * (h as usize) * 2);
    for px in rgba.pixels() {
        // Discard alpha; RGB565 has no alpha channel.
        let r = (px[0] >> 3) as u16; // 5 bits
        let g = (px[1] >> 2) as u16; // 6 bits
        let b = (px[2] >> 3) as u16; // 5 bits
        let v: u16 = (r << 11) | (g << 5) | b;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    Ok(Asset {
        name,
        width: w as u16,
        height: h as u16,
        cf: LV_COLOR_FORMAT_RGB565,
        data: buf,
    })
}

/// Build the ASSETS section data. The data of each asset starts on a 4-byte
/// boundary within the section and each record is followed by 0..3 pad bytes
/// so the next record also starts on a 4-byte boundary.
fn build_assets_data(assets: &[Asset]) -> Vec<u8> {
    let mut data = Vec::new();
    write_u32_le(&mut data, assets.len() as u32);
    for a in assets {
        write_str_u16(&mut data, &a.name);
        write_u16_le(&mut data, a.width);
        write_u16_le(&mut data, a.height);
        data.push(a.cf);
        data.push(0); // reserved0
        write_u16_le(&mut data, 0u16); // stride: 0 = derive from width + cf
        write_u32_le(&mut data, a.data.len() as u32);
        // Pad to 4-byte boundary before the data.
        while data.len() % 4 != 0 {
            data.push(0);
        }
        data.extend_from_slice(&a.data);
        // Pad to 4-byte boundary before the next record.
        while data.len() % 4 != 0 {
            data.push(0);
        }
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
    assets: &[Asset],
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
    let assets_data = if assets.is_empty() {
        Vec::new()
    } else {
        build_assets_data(assets)
    };

    let manifest_hdr = build_section_header(TAG_MANIFEST, manifest_data.len() as u32);
    let classes_hdr = build_section_header(TAG_CLASSES, classes_data.len() as u32);
    let assets_hdr = build_section_header(TAG_ASSETS, assets_data.len() as u32);

    // File header is 24 bytes.
    // MANIFEST section starts immediately after.
    let manifest_offset: u32 = 24;
    let classes_offset: u32 =
        manifest_offset + manifest_hdr.len() as u32 + manifest_data.len() as u32;
    // 0 means "no ASSETS section". Legacy parsers see zero in the slot they
    // formerly read as `reserved` and behave unchanged.
    let assets_offset: u32 = if assets.is_empty() {
        0
    } else {
        classes_offset + classes_hdr.len() as u32 + classes_data.len() as u32
    };
    let section_count: u32 = if assets.is_empty() { 2 } else { 3 };

    let mut file = Vec::new();

    // File header (24 bytes)
    file.extend_from_slice(MAGIC);
    write_u16_le(&mut file, VERSION_MAJOR);
    write_u16_le(&mut file, VERSION_MINOR);
    write_u32_le(&mut file, section_count);
    write_u32_le(&mut file, manifest_offset);
    write_u32_le(&mut file, classes_offset);
    write_u32_le(&mut file, assets_offset);

    // MANIFEST section
    file.extend_from_slice(&manifest_hdr);
    file.extend_from_slice(&manifest_data);

    // CLASSES section
    file.extend_from_slice(&classes_hdr);
    file.extend_from_slice(&classes_data);

    // ASSETS section (optional)
    if !assets.is_empty() {
        file.extend_from_slice(&assets_hdr);
        file.extend_from_slice(&assets_data);
    }

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

    let assets: Vec<Asset> = match &args.assets_dir {
        Some(dir) if dir.is_dir() => match collect_assets(dir) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Error reading assets: {e}");
                std::process::exit(1);
            }
        },
        Some(dir) => {
            eprintln!(
                "Warning: --assets-dir '{}' is not a directory; skipping",
                dir.display()
            );
            Vec::new()
        }
        None => Vec::new(),
    };

    let papk = build_papk(
        args.main_class.as_deref(),
        args.activity.as_deref(),
        args.application.as_deref(),
        &args.package_name,
        &args.version,
        &args.framework_map_version,
        &classes,
        &assets,
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
                "==> Wrote {} ({} bytes, {} classes, {} assets)",
                args.output.display(),
                papk.len(),
                classes.len(),
                assets.len()
            );
        }
        Err(e) => {
            eprintln!("Error writing output: {e}");
            std::process::exit(1);
        }
    }
}
