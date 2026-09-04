// SPDX-License-Identifier: GPL-3.0-only
use std::path::Path;
use std::{env, fs, process};

use papk_format::{FileHeader, Papk, PapkError, FILE_HEADER_LEN};

// ── Row extraction (papk_format parser + declared-count checks) ────────────────

/// Collect `(name, size)` rows from the CLASSES section.
///
/// `ClassIter` silently stops early when the section is truncated mid-record;
/// papk-info is a diagnostic tool and must not print a short table for a
/// corrupt file, so compare the yielded count against the declared count and
/// error on shortfall (preserving the old hand-rolled reader's
/// Err-on-truncation behavior).
fn collect_class_rows(papk: &Papk) -> Result<Vec<(String, usize)>, String> {
    let declared = papk
        .class_count()
        .map_err(|e| format!("CLASSES section: {e}"))?;
    let rows: Vec<(String, usize)> = papk
        .classes()
        .map_err(|e| format!("CLASSES section: {e}"))?
        .map(|c| (String::from_utf8_lossy(c.name).into_owned(), c.data.len()))
        .collect();
    if (rows.len() as u32) < declared {
        return Err(format!(
            "CLASSES section truncated: declared {declared} classes, found {}",
            rows.len()
        ));
    }
    Ok(rows)
}

/// Decoded asset row for the dump table (constructed from
/// `papk_format::AssetEntry`).
#[derive(Debug)]
struct AssetInfo {
    name: String,
    width: u16,
    height: u16,
    cf: u8,
    data_size: usize,
}

/// Collect [`AssetInfo`] rows from the ASSETS section, or `None` if the papk
/// has no ASSETS section. Same declared-vs-yielded shortfall check as
/// [`collect_class_rows`].
fn collect_asset_rows(papk: &Papk) -> Result<Option<Vec<AssetInfo>>, String> {
    let Some(iter) = papk.assets().map_err(|e| format!("ASSETS section: {e}"))? else {
        return Ok(None);
    };
    // `assets()` returned an iterator, so the declared count is readable too.
    let declared = papk
        .asset_count()
        .map_err(|e| format!("ASSETS section: {e}"))?
        .unwrap_or(0);
    let rows: Vec<AssetInfo> = iter
        .map(|a| AssetInfo {
            name: String::from_utf8_lossy(a.name).into_owned(),
            width: a.width,
            height: a.height,
            cf: a.cf,
            data_size: a.data.len(),
        })
        .collect();
    if (rows.len() as u32) < declared {
        return Err(format!(
            "ASSETS section truncated: declared {declared} assets, found {}",
            rows.len()
        ));
    }
    Ok(Some(rows))
}

/// Translate an LVGL color format byte to a friendly label. Values match
/// `third_party/lvgl/src/misc/lv_color.h` `lv_color_format_t`.
fn cf_label(cf: u8) -> &'static str {
    match cf {
        0x0F => "RGB888",
        0x10 => "ARGB8888",
        0x12 => "RGB565",
        0x14 => "RGB565A8",
        0x1A => "ARGB8888_PRE",
        0x1B => "RGB565_SWAPPED",
        _ => "?",
    }
}

fn print_assets_table(assets: &[AssetInfo]) {
    const NAME_MIN: usize = 16;
    let name_col = assets
        .iter()
        .map(|a| a.name.len())
        .max()
        .unwrap_or(NAME_MIN)
        .max(NAME_MIN);
    let dim_col = 11; // " 1234x5678 "
    let cf_col = 16;
    let size_col = 9;

    println!(
        "  ┌{n}┬{d}┬{c}┬{s}┐",
        n = "─".repeat(name_col + 2),
        d = "─".repeat(dim_col + 2),
        c = "─".repeat(cf_col + 2),
        s = "─".repeat(size_col + 2),
    );
    println!(
        "  │ {:<name_col$} │ {:<dim_col$} │ {:<cf_col$} │ {:>size_col$} │",
        "Asset",
        "Dim",
        "Format",
        "Size",
        name_col = name_col,
        dim_col = dim_col,
        cf_col = cf_col,
        size_col = size_col,
    );
    println!(
        "  ├{n}┼{d}┼{c}┼{s}┤",
        n = "─".repeat(name_col + 2),
        d = "─".repeat(dim_col + 2),
        c = "─".repeat(cf_col + 2),
        s = "─".repeat(size_col + 2),
    );
    for a in assets {
        let dim = format!("{}x{}", a.width, a.height);
        let cf_text = format!("{} ({:#04x})", cf_label(a.cf), a.cf);
        println!(
            "  │ {:<name_col$} │ {:<dim_col$} │ {:<cf_col$} │ {:>size_col$} │",
            a.name,
            dim,
            cf_text,
            fmt_size(a.data_size),
            name_col = name_col,
            dim_col = dim_col,
            cf_col = cf_col,
            size_col = size_col,
        );
    }
    println!(
        "  └{n}┴{d}┴{c}┴{s}┘",
        n = "─".repeat(name_col + 2),
        d = "─".repeat(dim_col + 2),
        c = "─".repeat(cf_col + 2),
        s = "─".repeat(size_col + 2),
    );
}

// ── Display helpers ────────────────────────────────────────────────────────────

fn tag_name(tag: u32) -> String {
    let b = tag.to_le_bytes();
    String::from_utf8_lossy(&b).into_owned()
}

fn fmt_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn horizontal_rule(width: usize) -> String {
    "━".repeat(width)
}

fn print_table(classes: &[(String, usize)]) {
    const MIN_NAME_COL: usize = 20;
    const SIZE_COL: usize = 9; // " 1234 B "

    let name_col = classes
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(MIN_NAME_COL)
        .max(MIN_NAME_COL);

    // ┌─...─┬─...─┐
    println!(
        "  ┌{name}┬{size}┐",
        name = "─".repeat(name_col + 2),
        size = "─".repeat(SIZE_COL + 2),
    );
    // header row
    println!(
        "  │ {:<name_col$} │ {:>SIZE_COL$} │",
        "Class",
        "Size",
        name_col = name_col,
    );
    // ├─...─┼─...─┤
    println!(
        "  ├{name}┼{size}┤",
        name = "─".repeat(name_col + 2),
        size = "─".repeat(SIZE_COL + 2),
    );
    for (name, size) in classes {
        println!(
            "  │ {:<name_col$} │ {:>SIZE_COL$} │",
            name,
            fmt_size(*size),
            name_col = name_col,
        );
    }
    // └─...─┴─...─┘
    println!(
        "  └{name}┴{size}┘",
        name = "─".repeat(name_col + 2),
        size = "─".repeat(SIZE_COL + 2),
    );
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn run(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    let filename = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    let rule = horizontal_rule(40);

    println!("PAPK: {filename}");
    println!("{rule}");
    println!("Total size: {} bytes", data.len());

    // ── File header ──────────────────────────────────────────────────────────
    // FileHeader::parse checks magic + length only (no version_major bound),
    // so the header of a future-versioned file still gets dumped.
    let hdr = FileHeader::parse(&data).map_err(|e| match e {
        PapkError::Truncated => format!(
            "File too short: {} bytes (need at least {FILE_HEADER_LEN})",
            data.len()
        ),
        PapkError::BadMagic => "Not a PAPK file: magic bytes are not 'PAPK'".to_string(),
        other => format!("File header: {other}"),
    })?;
    println!();
    println!("File Header  ({FILE_HEADER_LEN} bytes @ {:#x})", 0);
    println!("  magic           \"PAPK\"");
    println!(
        "  version         {}.{}",
        hdr.version_major, hdr.version_minor
    );
    println!("  sections        {}", hdr.section_count);
    println!(
        "  manifest_off    {:#x}  ({})",
        hdr.manifest_offset, hdr.manifest_offset
    );
    println!(
        "  classes_off     {:#x}  ({})",
        hdr.classes_offset, hdr.classes_offset
    );
    if hdr.assets_offset != 0 {
        println!(
            "  assets_off      {:#x}  ({})",
            hdr.assets_offset, hdr.assets_offset
        );
    } else {
        println!("  assets_off      —  (no ASSETS section)");
    }

    // ── Full parse ───────────────────────────────────────────────────────────
    // A future-major file stops here, after the header dump (accepted change
    // vs the old hand-rolled reader, which walked sections of files whose
    // version it did not understand).
    let papk = Papk::parse(&data).map_err(|e| match e {
        PapkError::UnsupportedVersion => format!(
            "unsupported PAPK major version {} (papk-info supports {}) — \
             stopping after the header dump",
            hdr.version_major,
            papk_format::SUPPORTED_VERSION_MAJOR
        ),
        other => format!("{other}"),
    })?;

    // ── Manifest section ─────────────────────────────────────────────────────
    let (manifest_hdr, _) = papk
        .manifest_section()
        .map_err(|e| format!("MANIFEST section: {e}"))?;
    let manifest_entries: Vec<(String, String)> = papk
        .manifest()
        .map_err(|e| format!("MANIFEST section: {e}"))?
        .map(|e| {
            (
                String::from_utf8_lossy(e.key).into_owned(),
                String::from_utf8_lossy(e.value).into_owned(),
            )
        })
        .collect();

    println!();
    println!(
        "Manifest  ({} bytes @ {:#x})  tag \"{}\"",
        manifest_hdr.length,
        hdr.manifest_offset,
        tag_name(manifest_hdr.tag),
    );
    let key_width = manifest_entries
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or(12);
    for (key, val) in &manifest_entries {
        println!("  {:<key_width$}  {}", key, val, key_width = key_width);
    }

    // ── Classes section ──────────────────────────────────────────────────────
    let (classes_hdr, _) = papk
        .classes_section()
        .map_err(|e| format!("CLASSES section: {e}"))?;
    let classes = collect_class_rows(&papk)?;

    let total_bytecode: usize = classes.iter().map(|(_, s)| s).sum();

    println!();
    println!(
        "Classes  ({} bytes @ {:#x})  tag \"{}\"",
        classes_hdr.length,
        hdr.classes_offset,
        tag_name(classes_hdr.tag),
    );
    print_table(&classes);
    println!(
        "  {} classes · {} of bytecode",
        classes.len(),
        fmt_size(total_bytecode),
    );

    // ── Assets section (optional) ────────────────────────────────────────────
    if let Some((assets_hdr, _)) = papk
        .assets_section()
        .map_err(|e| format!("ASSETS section: {e}"))?
    {
        let assets = collect_asset_rows(&papk)?.unwrap_or_default();
        let total_pixels: usize = assets.iter().map(|a| a.data_size).sum();
        println!();
        println!(
            "Assets  ({} bytes @ {:#x})  tag \"{}\"",
            assets_hdr.length,
            hdr.assets_offset,
            tag_name(assets_hdr.tag),
        );
        if assets.is_empty() {
            println!("  (empty)");
        } else {
            print_assets_table(&assets);
            println!(
                "  {} assets · {} of pixel data",
                assets.len(),
                fmt_size(total_pixels),
            );
        }
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("Usage: papk-info <file.papk>");
        process::exit(if args.len() == 1 { 1 } else { 0 });
    }
    if let Err(e) = run(Path::new(&args[1])) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_size_bytes_below_one_kib() {
        assert_eq!(fmt_size(0), "0 B");
        assert_eq!(fmt_size(1023), "1023 B");
    }

    #[test]
    fn fmt_size_kib_range() {
        assert_eq!(fmt_size(1024), "1.0 KiB");
        assert_eq!(fmt_size(1536), "1.5 KiB");
        assert_eq!(fmt_size(1024 * 1024 - 1), "1024.0 KiB");
    }

    #[test]
    fn fmt_size_mib_range() {
        assert_eq!(fmt_size(1024 * 1024), "1.0 MiB");
        assert_eq!(fmt_size(2 * 1024 * 1024), "2.0 MiB");
        assert_eq!(fmt_size(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
    }

    #[test]
    fn tag_name_decodes_le_bytes() {
        assert_eq!(tag_name(u32::from_le_bytes(*b"MANI")), "MANI");
    }

    // ── Declared-vs-yielded shortfall checks (bytes doctored from the
    //    papk-format golden fixtures) ─────────────────────────────────────────

    static MINIMAL_FIXTURE: &[u8] =
        include_bytes!("../../../papk-format/tests/fixtures/minimal.papk");
    static WITH_ASSETS_FIXTURE: &[u8] =
        include_bytes!("../../../papk-format/tests/fixtures/with-assets.papk");

    #[test]
    fn class_shortfall_is_an_error_not_a_short_table() {
        // Inflate the declared class count: the iterator still yields only
        // the one real class, and papk-info must error instead of printing a
        // silently short table.
        let mut bytes = MINIMAL_FIXTURE.to_vec();
        let hdr = FileHeader::parse(&bytes).unwrap();
        let count_off = hdr.classes_offset as usize + papk_format::SECTION_HEADER_LEN;
        bytes[count_off..count_off + 4].copy_from_slice(&7u32.to_le_bytes());
        let papk = Papk::parse(&bytes).unwrap();
        let err = collect_class_rows(&papk).unwrap_err();
        assert_eq!(
            err,
            "CLASSES section truncated: declared 7 classes, found 1"
        );
    }

    #[test]
    fn asset_shortfall_is_an_error_not_a_short_table() {
        let mut bytes = WITH_ASSETS_FIXTURE.to_vec();
        let hdr = FileHeader::parse(&bytes).unwrap();
        let count_off = hdr.assets_offset as usize + papk_format::SECTION_HEADER_LEN;
        bytes[count_off..count_off + 4].copy_from_slice(&3u32.to_le_bytes());
        let papk = Papk::parse(&bytes).unwrap();
        let err = collect_asset_rows(&papk).unwrap_err();
        assert_eq!(err, "ASSETS section truncated: declared 3 assets, found 1");
    }

    #[test]
    fn valid_fixture_rows_match_declared_counts() {
        let papk = Papk::parse(WITH_ASSETS_FIXTURE).unwrap();
        let classes = collect_class_rows(&papk).unwrap();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].0, "fixture/Main");
        let assets = collect_asset_rows(&papk).unwrap().unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "gradient.png");
        assert_eq!(assets[0].data_size, 128);
    }
}
