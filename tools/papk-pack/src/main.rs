// SPDX-License-Identifier: GPL-3.0-only
//! papk-pack: Packages compiled Java .class files into a PAPK (Picodroid APK) binary file.
//!
//! Usage:
//!   papk-pack \
//!     --main-class helloworld/HelloWorld \
//!     --package-name helloworld \
//!     --version 1.0 \
//!     --classes-dir build/classes/helloworld \
//!     --output build/apks/helloworld.papk

mod classcheck;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use papk_format::{AssetSpec, EntryPoint, ManifestSpec, PapkBuilder};

/// LVGL `lv_color_format_t` value for native RGB565 little-endian. Verified
/// against `vendor/lvgl/src/misc/lv_color.h` (`LV_COLOR_FORMAT_RGB565`) —
/// drift-guarded by the test module at the bottom of this file.
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
    /// The shrink map `--classes-dir` was rewritten with, when it was. The
    /// entry-point check compares descriptors against what is actually in
    /// the class files, so its `java/**` names must be shrunk the same way;
    /// and an app-shrunk map (`--shrink-app`) renames the entry class
    /// itself, so the manifest entry is spelled through it too.
    shrink_map: Option<PathBuf>,
    /// The entry class as the manifest named it, when the map renamed it —
    /// for error messages only.
    entry_original: Option<String>,
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
    let mut shrink_map: Option<PathBuf> = None;

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
            "--shrink-map" => {
                i += 1;
                shrink_map = Some(PathBuf::from(
                    args.get(i).ok_or("--shrink-map requires a value")?,
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

    let entry_flags =
        main_class.is_some() as u8 + activity.is_some() as u8 + application.is_some() as u8;
    if entry_flags == 0 {
        return Err("either --main-class, --activity, or --application is required".into());
    }
    if entry_flags > 1 {
        return Err("exactly one of --main-class, --activity, or --application may be set".into());
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
        shrink_map,
        entry_original: None,
    })
}

/// Spell the manifest entry class the way the packed class files do: an
/// app-shrunk map (`class-shrink cut-app`) renames app classes under `c/`,
/// so the class the manifest names is present under its shrunk name. The
/// release map alone never renames an app class, so this is the identity
/// for a plain `--shrink` build and without a map.
fn shrink_entry_point(args: &mut Args) -> Result<(), String> {
    let Some(map) = args.shrink_map.as_deref() else {
        return Ok(());
    };
    for slot in [
        &mut args.main_class,
        &mut args.activity,
        &mut args.application,
    ] {
        if let Some(name) = slot.as_deref() {
            let shrunk = shrunk_class(name, Some(map))?;
            if shrunk != name {
                args.entry_original = Some(name.to_string());
                *slot = Some(shrunk);
            }
        }
    }
    Ok(())
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
         \x20 [--shrink-map <map.toml>] \\\n\
         \x20 --output <file.papk>\n\
         \n\
         At least one of --main-class, --activity, or --application must be provided.\n\
         --assets-dir is optional; PNG files in the directory are decoded into\n\
         LVGL-native RGB565 and bundled in the ASSETS (ASST) section.\n\
         --shrink-map names the class-shrink map --classes-dir was rewritten with,\n\
         so the entry-point check matches descriptors in their shrunk spelling and\n\
         an app-shrunk (cut-app) map renames the manifest entry class itself."
    );
}

// ── Class file discovery ──────────────────────────────────────────────────────

/// Validate the manifest entry point against the packed classes. Errors when
/// the named class is absent (or — for a main-class — lacks `static main`);
/// warns (not errors) when an activity/application lacks `onCreate`, since
/// inheriting the framework default is legal.
fn validate_entry_point(args: &Args, classes: &[(String, Vec<u8>)]) -> Result<(), String> {
    let (entry, kind) = if let Some(c) = &args.main_class {
        (c, "main-class")
    } else if let Some(c) = &args.activity {
        (c, "activity")
    } else if let Some(c) = &args.application {
        (c, "application")
    } else {
        return Ok(());
    };

    let found = classes.iter().find(|(name, _)| name == entry);
    let bytes = match found {
        Some((_, b)) => b,
        None => {
            let mut names: Vec<&str> = classes.iter().map(|(n, _)| n.as_str()).collect();
            names.sort_unstable();
            let original = args
                .entry_original
                .as_deref()
                .map(|o| format!(" (manifest spelling '{o}')"))
                .unwrap_or_default();
            return Err(format!(
                "manifest {kind} '{entry}'{original} is not among the packed classes.\n  \
                 Packed: {}",
                names.join(", ")
            ));
        }
    };

    match kind {
        "main-class" => {
            // A main-class must declare `public static void main(String[])`.
            // `Some(false)` = parsed and absent; `None` (unparseable) degrades
            // gracefully — we don't reject a class we merely failed to read.
            // Shrunk classes spell `java/lang/String` as `b/…`, so the
            // expected descriptor is rewritten with the same map first.
            let main_desc =
                shrunk_descriptor("([Ljava/lang/String;)V", args.shrink_map.as_deref())?;
            if shrunk_member("main", args.shrink_map.as_deref())? != "main" {
                return Err("the shrink map renames `main` — sdk/keep.toml must keep it".into());
            }
            if matches!(
                classcheck::class_has_method(bytes, "main", &main_desc, classcheck::ACC_STATIC,),
                Some(false)
            ) {
                return Err(format!(
                    "main-class '{entry}' has no `static void main(String[])` — \
                     the app would fail to start on device"
                ));
            }
        }
        _ => {
            // onCreate is optional (the framework default is a no-op), so a
            // missing one is only a hint.
            let on_create = shrunk_member("onCreate", args.shrink_map.as_deref())?;
            if matches!(
                classcheck::class_has_method(bytes, &on_create, "", 0),
                Some(false)
            ) {
                eprintln!(
                    "Warning: {kind} '{entry}' declares no onCreate() — it will \
                     inherit the framework default (a no-op). Intentional?"
                );
            }
        }
    }
    Ok(())
}

/// `name` as it appears in class files rewritten with `map`'s `[[member]]`
/// rows (the Gradle `shrinkMembers` pass); unchanged without a map.
fn shrunk_member(name: &str, map: Option<&Path>) -> Result<String, String> {
    let Some(map_path) = map else {
        return Ok(name.to_string());
    };
    let map = class_shrink::mapping::ShrinkMap::load(map_path)
        .map_err(|e| format!("--shrink-map {}: {e}", map_path.display()))?;
    Ok(map.member_target(name).unwrap_or(name).to_string())
}

/// `name` as it appears in class files rewritten with `map`'s `[[class]]`
/// rows; unchanged when the map does not rename it or without a map.
fn shrunk_class(name: &str, map: Option<&Path>) -> Result<String, String> {
    let Some(map_path) = map else {
        return Ok(name.to_string());
    };
    let map = class_shrink::mapping::ShrinkMap::load(map_path)
        .map_err(|e| format!("--shrink-map {}: {e}", map_path.display()))?;
    Ok(map
        .classes
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string()))
}

/// `desc` as it appears in class files rewritten with `map` — every
/// `L<class>;` whose class the map renames is spelled the shrunk way.
/// Without a map the descriptor is returned unchanged.
fn shrunk_descriptor(desc: &str, map: Option<&Path>) -> Result<String, String> {
    let Some(map_path) = map else {
        return Ok(desc.to_string());
    };
    let map = class_shrink::mapping::ShrinkMap::load(map_path)
        .map_err(|e| format!("--shrink-map {}: {e}", map_path.display()))?;
    let byte_map = map
        .iter_classes()
        .map(|(from, to)| (from.as_bytes().to_vec(), to.as_bytes().to_vec()))
        .collect();
    let rewritten = class_shrink::descriptor::rewrite_descriptor(desc.as_bytes(), &byte_map);
    String::from_utf8(rewritten).map_err(|e| format!("shrunk descriptor is not UTF-8: {e}"))
}

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
        } else if path.extension().is_some_and(|e| e == "class") {
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

// ── PAPK serialization ────────────────────────────────────────────────────────

/// Map the parsed CLI arguments plus the collected classes/assets onto the
/// shared `papk_format` writer. This is the single real build path — `main()`
/// and the consumer-level integration test both go through it. The emitted
/// bytes are identical to the historical hand-rolled writer (pinned by
/// papk-format's golden-fixture tests).
fn build_papk(
    args: &Args,
    classes: &[(String, Vec<u8>)],
    assets: &[Asset],
) -> Result<Vec<u8>, papk_format::BuildError> {
    let entry = if let Some(mc) = args.main_class.as_deref() {
        EntryPoint::MainClass(mc)
    } else if let Some(act) = args.activity.as_deref() {
        EntryPoint::Activity(act)
    } else if let Some(app) = args.application.as_deref() {
        EntryPoint::Application(app)
    } else {
        unreachable!("parse_args enforces exactly one entry-point flag")
    };

    let mut builder = PapkBuilder::new(ManifestSpec {
        entry,
        package_name: &args.package_name,
        version: &args.version,
        framework_map_version: &args.framework_map_version,
    });
    for (name, bytes) in classes {
        builder.class(name, bytes);
    }
    for a in assets {
        builder.asset(AssetSpec {
            name: &a.name,
            width: a.width,
            height: a.height,
            cf: a.cf,
            stride: 0, // 0 = derive from width + cf
            data: &a.data,
        });
    }
    builder.build()
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

    // Spell the entry class the way the packed classes do: a plain --shrink
    // build leaves app classes under their own names, an --shrink-app build
    // renames them under c/, and the manifest must name what is packed.
    let mut args = args;
    if let Err(msg) = shrink_entry_point(&mut args) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }

    // Validate the manifest entry point against the packed classes — catches
    // a typo'd `activity=`/`main-class=`/`application=` at build time instead
    // of a runtime NoSuchMethod on device.
    if let Err(msg) = validate_entry_point(&args, &classes) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
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

    let papk = match build_papk(&args, &classes, &assets) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error building PAPK: {e}");
            std::process::exit(1);
        }
    };

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

#[cfg(test)]
mod pack_integration {
    //! Consumer-level check (papk-format design §(d)6): pack the checked-in
    //! fixture `.class` through the real CLI build path (collect_classes →
    //! validate_entry_point → build_papk) and assert the shared parser
    //! accepts the output — papk-pack's writer path is no longer untested.
    use super::*;

    #[test]
    fn packing_the_fixture_class_yields_a_parseable_papk() {
        let fixtures = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../papk-format/tests/fixtures"
        ));
        let class_bytes = fs::read(fixtures.join("Main.class")).expect("fixture Main.class");

        // Stage a classes-dir layout so collect_classes derives the JVM name
        // from the path, exactly as the real CLI invocation does.
        let work = std::env::temp_dir().join(format!("papk-pack-it-{}", std::process::id()));
        let classes_dir = work.join("classes");
        fs::create_dir_all(classes_dir.join("fixture")).unwrap();
        fs::write(classes_dir.join("fixture/Main.class"), &class_bytes).unwrap();

        let args = Args {
            main_class: Some("fixture/Main".into()),
            activity: None,
            application: None,
            package_name: "fixture".into(),
            version: "1.0".into(),
            framework_map_version: "0.0.0".into(),
            classes_dir: classes_dir.clone(),
            output: work.join("out.papk"),
            assets_dir: None,
            shrink_map: None,
            entry_original: None,
        };

        let classes = collect_classes(&args.classes_dir).unwrap();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].0, "fixture/Main");
        validate_entry_point(&args, &classes).unwrap();

        let papk = build_papk(&args, &classes, &[]).unwrap();
        let parsed = papk_format::Papk::parse(&papk).expect("papk-pack output must parse");
        assert_eq!(parsed.main_class(), Some("fixture/Main"));
        assert_eq!(parsed.class_count(), Ok(1));
        let entry = parsed.classes().unwrap().next().unwrap();
        assert_eq!(entry.name, b"fixture/Main");
        assert_eq!(entry.data, class_bytes.as_slice());
        assert!(parsed.assets().unwrap().is_none());

        // These are the exact inputs that produced the pre-refactor golden
        // fixture (see fixtures/README.md), so the CLI mapping must still
        // reproduce it byte-for-byte — a stale build/apks/*.papk stays
        // reproducible.
        let golden = fs::read(fixtures.join("minimal.papk")).expect("fixture minimal.papk");
        assert_eq!(papk, golden);

        fs::remove_dir_all(&work).ok();
    }
}

#[cfg(test)]
mod color_format_guard {
    //! papk-pack bakes `LV_COLOR_FORMAT_RGB565` into every image asset it
    //! writes; firmware feeds that byte straight to LVGL. This mirrors the
    //! drift guard in picodroid-core/src/lvgl_ffi.rs (deliberately copied,
    //! not shared — a host tool should not depend on the firmware core crate
    //! for one constant).
    use super::LV_COLOR_FORMAT_RGB565;

    const LV_COLOR_HEADER: &str = include_str!("../../../vendor/lvgl/src/misc/lv_color.h");

    fn lookup_assigned_hex(body: &str, name: &str) -> Option<u32> {
        for line in body.lines() {
            let trimmed = line.trim_start();
            let Some(ident_end) = trimmed.find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            else {
                continue;
            };
            if &trimmed[..ident_end] != name {
                continue;
            }
            let rhs = trimmed[ident_end..].split_once('=')?.1;
            let rhs = rhs.split(',').next().unwrap_or(rhs).trim();
            let hex = rhs.strip_prefix("0x").or_else(|| rhs.strip_prefix("0X"))?;
            return u32::from_str_radix(hex, 16).ok();
        }
        None
    }

    #[test]
    fn rgb565_matches_vendored_header() {
        let close = LV_COLOR_HEADER
            .find("} lv_color_format_t")
            .expect("enum close");
        let open = LV_COLOR_HEADER[..close]
            .rfind("typedef enum")
            .expect("enum open");
        let body = &LV_COLOR_HEADER[open..close];
        assert_eq!(
            lookup_assigned_hex(body, "LV_COLOR_FORMAT_RGB565"),
            Some(LV_COLOR_FORMAT_RGB565 as u32),
            "papk-pack's RGB565 color-format byte drifted from vendored lv_color.h — \
             every packed image asset would render corrupted."
        );
    }
}
