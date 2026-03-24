use std::path::Path;
use std::{env, fs, process};

// ── Binary format constants ────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"PAPK";
const FILE_HEADER_LEN: usize = 24;
const SECTION_HEADER_LEN: usize = 16;
const TAG_MANIFEST: u32 = u32::from_le_bytes(*b"MANI");
const TAG_CLASSES: u32 = u32::from_le_bytes(*b"CLSS");

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
