use std::path::Path;
use std::{env, fs, process};

// ── Binary format constants ────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"PAPK";
const FILE_HEADER_LEN: usize = 24;
const SECTION_HEADER_LEN: usize = 16;
const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");
const TAG_ASSETS: u32 = u32::from_le_bytes(*b"ASST");

// ── Low-level read helpers ─────────────────────────────────────────────────────

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let b = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let b = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_bytes_u16<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let len = read_u16(data, *pos)? as usize;
    *pos += 2;
    let slice = data.get(*pos..*pos + len)?;
    *pos += len;
    Some(slice)
}

fn read_bytes_u32<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let len = read_u32(data, *pos)? as usize;
    *pos += 4;
    let slice = data.get(*pos..*pos + len)?;
    *pos += len;
    Some(slice)
}

// ── PAPK parsing ──────────────────────────────────────────────────────────────

struct FileHeader {
    version_major: u16,
    version_minor: u16,
    section_count: u32,
    manifest_offset: usize,
    classes_offset: usize,
    /// 0 = no ASSETS section (legacy v1.0, or v1.1 without --assets-dir).
    assets_offset: usize,
}

struct SectionHeader {
    tag: u32,
    length: usize,
}

fn parse_file_header(data: &[u8]) -> Result<FileHeader, String> {
    if data.len() < FILE_HEADER_LEN {
        return Err(format!(
            "File too short: {} bytes (need at least {FILE_HEADER_LEN})",
            data.len()
        ));
    }
    if &data[0..4] != MAGIC {
        return Err(format!(
            "Not a PAPK file: expected magic {:?}, got {:?}",
            MAGIC,
            &data[0..4]
        ));
    }
    Ok(FileHeader {
        version_major: read_u16(data, 4).unwrap(),
        version_minor: read_u16(data, 6).unwrap(),
        section_count: read_u32(data, 8).unwrap(),
        manifest_offset: read_u32(data, 12).unwrap() as usize,
        classes_offset: read_u32(data, 16).unwrap() as usize,
        assets_offset: read_u32(data, 20).unwrap() as usize,
    })
}

fn parse_section_header(data: &[u8], offset: usize) -> Result<SectionHeader, String> {
    if data.len() < offset + SECTION_HEADER_LEN {
        return Err(format!("Truncated section header at offset {offset:#x}"));
    }
    Ok(SectionHeader {
        tag: read_u32(data, offset).unwrap(),
        length: read_u32(data, offset + 4).unwrap() as usize,
    })
}

fn section_data<'a>(data: &'a [u8], offset: usize, expected_tag: u32) -> Result<&'a [u8], String> {
    let hdr = parse_section_header(data, offset)?;
    if hdr.tag != expected_tag {
        return Err(format!(
            "Unexpected section tag at {offset:#x}: expected {expected_tag:#010x}, got {:#010x}",
            hdr.tag
        ));
    }
    let start = offset + SECTION_HEADER_LEN;
    let end = start + hdr.length;
    data.get(start..end)
        .ok_or_else(|| format!("Section data at {offset:#x} extends beyond file end"))
}

fn parse_manifest(data: &[u8]) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let Some(key) = read_bytes_u16(data, &mut pos) else {
            break;
        };
        let Some(val) = read_bytes_u16(data, &mut pos) else {
            break;
        };
        let key = String::from_utf8_lossy(key).into_owned();
        let val = String::from_utf8_lossy(val).into_owned();
        entries.push((key, val));
    }
    entries
}

fn parse_classes(data: &[u8]) -> Result<Vec<(String, usize)>, String> {
    if data.len() < 4 {
        return Err("Classes section too short".into());
    }
    let count = read_u32(data, 0).unwrap() as usize;
    let mut pos = 4;
    let mut classes = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(name) = read_bytes_u16(data, &mut pos) else {
            return Err("Truncated class name".into());
        };
        let Some(class_data) = read_bytes_u32(data, &mut pos) else {
            return Err("Truncated class data".into());
        };
        classes.push((String::from_utf8_lossy(name).into_owned(), class_data.len()));
    }
    Ok(classes)
}

/// Decoded asset row for the dump table.
struct AssetInfo {
    name: String,
    width: u16,
    height: u16,
    cf: u8,
    data_size: usize,
}

fn parse_assets(data: &[u8]) -> Result<Vec<AssetInfo>, String> {
    if data.len() < 4 {
        return Err("Assets section too short".into());
    }
    let count = read_u32(data, 0).unwrap() as usize;
    let mut pos = 4;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let Some(name) = read_bytes_u16(data, &mut pos) else {
            return Err("Truncated asset name".into());
        };
        if pos + 12 > data.len() {
            return Err("Truncated asset header".into());
        }
        let width = read_u16(data, pos).unwrap();
        let height = read_u16(data, pos + 2).unwrap();
        let cf = data[pos + 4];
        let data_size = read_u32(data, pos + 8).unwrap() as usize;
        pos += 12;
        // Pad to 4-byte boundary before pixel data.
        pos = (pos + 3) & !3;
        if pos + data_size > data.len() {
            return Err("Truncated asset pixel data".into());
        }
        pos += data_size;
        // Pad to 4-byte boundary before next record.
        pos = (pos + 3) & !3;
        out.push(AssetInfo {
            name: String::from_utf8_lossy(name).into_owned(),
            width,
            height,
            cf,
            data_size,
        });
    }
    Ok(out)
}

/// Translate an LVGL color format byte to a friendly label. Values match
/// `vendor/lvgl/src/misc/lv_color.h` `lv_color_format_t`.
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
        format!("{:.1} KiB", bytes as f64 / 1024.0)
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
    let hdr = parse_file_header(&data)?;
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

    // ── Manifest section ─────────────────────────────────────────────────────
    let manifest_hdr = parse_section_header(&data, hdr.manifest_offset)?;
    let manifest_data = section_data(&data, hdr.manifest_offset, TAG_MANIFEST)
        .map_err(|e| format!("MANIFEST section: {e}"))?;
    let manifest_entries = parse_manifest(manifest_data);

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
    let classes_hdr = parse_section_header(&data, hdr.classes_offset)?;
    let classes_data = section_data(&data, hdr.classes_offset, TAG_CLASSES)
        .map_err(|e| format!("CLASSES section: {e}"))?;
    let classes = parse_classes(classes_data)?;

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
    if hdr.assets_offset != 0 {
        let assets_hdr = parse_section_header(&data, hdr.assets_offset)?;
        let assets_data = section_data(&data, hdr.assets_offset, TAG_ASSETS)
            .map_err(|e| format!("ASSETS section: {e}"))?;
        let assets = parse_assets(assets_data)?;
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
