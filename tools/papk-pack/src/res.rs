// SPDX-License-Identifier: GPL-3.0-only
//! The resource compiler: an app's `res/` tree in, a RESOURCES section and
//! an `R.java` out.
//!
//! ```text
//!   res/values/*.xml     <string> <color> <dimen> <integer> <bool>
//!   res/layout/*.xml     one view tree per file
//!   res/drawable/*.png   packed into ASSETS as "res/drawable/<file>"
//! ```
//!
//! Both consumers — `papk-pack gen-r` before `compileJava`, `papk-pack
//! --res-dir` after it — run [`compile`] over the same directory, and ids
//! come out of sorted names, so the `R.java` an app was compiled against and
//! the table it is packed with agree by construction rather than through a
//! file passed between them.
//!
//! There are no configurations: one display, one density (`dp` = `sp` =
//! `px`), one locale. A `values-night/` or `drawable-hdpi/` directory is an
//! error rather than something silently ignored. Every reference
//! (`@color/accent`, `@dimen/gap`) is resolved here, at build time; the only
//! thing a layout leaves for the device to look up is a string.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use papk_format::res::{
    self as fmt, layout, ResTableBuilder, TYPE_BOOL, TYPE_COLOR, TYPE_DIMEN, TYPE_DRAWABLE,
    TYPE_ID, TYPE_INTEGER, TYPE_LAYOUT, TYPE_STRING,
};

/// The ASSETS-entry prefix of a `res/drawable/` image.
pub const DRAWABLE_ASSET_PREFIX: &str = "res/drawable/";

/// A compiled `res/` tree.
#[derive(Debug)]
pub struct Compiled {
    /// RESOURCES section data; empty when `res/` defines nothing a table
    /// holds (no section is written then).
    pub table: Vec<u8>,
    /// `(type, name, id)` for `R.java`, sorted by type then name.
    pub symbols: Vec<(u8, String, u32)>,
    /// `res/drawable/*.png`, sorted: `(ASSETS entry name, file)`.
    pub drawables: Vec<(String, PathBuf)>,
    /// Attributes and elements that were skipped, for the packer to print.
    pub warnings: Vec<String>,
}

// ── A minimal DOM over xml-rs ─────────────────────────────────────────────────

struct Element {
    name: String,
    /// `(namespace prefix, local name, value)`.
    attrs: Vec<(Option<String>, String, String)>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(_, n, _)| n == local)
            .map(|(_, _, v)| v.as_str())
    }
}

fn parse_xml(path: &Path) -> Result<Element, String> {
    use xml::reader::{EventReader, XmlEvent};
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    for event in EventReader::new(std::io::BufReader::new(file)) {
        match event.map_err(|e| format!("{}: {e}", path.display()))? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => stack.push(Element {
                name: name.local_name,
                attrs: attributes
                    .into_iter()
                    .map(|a| (a.name.prefix, a.name.local_name, a.value))
                    .collect(),
                children: Vec::new(),
                text: String::new(),
            }),
            XmlEvent::EndElement { .. } => {
                let done = stack.pop().expect("xml-rs balances elements");
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => root = Some(done),
                }
            }
            XmlEvent::Characters(t) | XmlEvent::CData(t) | XmlEvent::Whitespace(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t);
                }
            }
            _ => {}
        }
    }
    root.ok_or_else(|| format!("{}: no root element", path.display()))
}

// ── Names ─────────────────────────────────────────────────────────────────────

const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "void",
    "volatile",
    "while",
];

/// A resource name as `R` spells it: Android maps `.` to `_`; everything else
/// must already be a Java identifier.
fn r_name(name: &str, what: &str) -> Result<String, String> {
    let ident = name.replace('.', "_");
    let mut chars = ident.chars();
    let ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok || JAVA_KEYWORDS.contains(&ident.as_str()) {
        return Err(format!(
            "{what}: '{name}' is not a usable resource name (letters, digits and '_', not \
             starting with a digit, not a Java keyword)"
        ));
    }
    Ok(ident)
}

/// File-based resources: the name is the file stem, lower-case as on Android.
fn file_res_name(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("{}: file name is not UTF-8", path.display()))?;
    let ok = stem
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return Err(format!(
            "{}: file-based resource names must be [a-z0-9_] only",
            path.display()
        ));
    }
    r_name(stem, &path.display().to_string())
}

fn sorted_files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some(ext) {
            return Err(format!(
                "{}: only *.{ext} files belong in {}",
                path.display(),
                dir.display()
            ));
        }
        out.push(path);
    }
    out.sort();
    Ok(out)
}

// ── Value parsing ─────────────────────────────────────────────────────────────

/// `#RGB`, `#ARGB`, `#RRGGBB`, `#AARRGGBB` → ARGB.
fn parse_color(s: &str) -> Option<u32> {
    let hex = s.strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let wide = |h: &str| -> String { h.chars().flat_map(|c| [c, c]).collect() };
    let full = match hex.len() {
        3 => format!("ff{}", wide(hex)),
        4 => wide(hex),
        6 => format!("ff{hex}"),
        8 => hex.to_string(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok()
}

/// `12dp`, `1.5sp`, `8px` → pixels. One density: every supported unit is a
/// pixel. Physical units have no meaning without a DPI and are refused.
fn parse_dimen(s: &str) -> Result<f32, String> {
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let value: f32 = num
        .parse()
        .map_err(|_| format!("'{s}' is not a dimension (expected e.g. 12dp)"))?;
    match unit {
        "px" | "dp" | "dip" | "sp" => Ok(value),
        "" => Err(format!("'{s}': a dimension needs a unit (px, dp or sp)")),
        _ => Err(format!(
            "'{s}': unit '{unit}' is not supported (px, dp, sp — all one pixel here)"
        )),
    }
}

fn parse_int(s: &str) -> Option<i32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok().map(|v| v as i32),
        None => s.parse().ok(),
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Android's `<string>` text rules: surrounding whitespace is trimmed and
/// inner runs collapse to one space unless the text is double-quoted;
/// backslash escapes `\n \t \' \" \\ \@ \?` and `\uXXXX`.
fn unescape_string(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let quoted = trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"');
    let body = if quoted {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let cp = u32::from_str_radix(&hex, 16)
                    .ok()
                    .filter(|_| hex.len() == 4)
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("bad \\u escape in '{raw}'"))?;
                out.push(cp);
            }
            Some(other @ ('\'' | '"' | '\\' | '@' | '?')) => out.push(other),
            Some(other) => return Err(format!("unknown escape '\\{other}' in '{raw}'")),
            None => return Err(format!("trailing backslash in '{raw}'")),
        }
    }
    Ok(out)
}

/// `@type/name` or `@+type/name` → `(type, R name)`.
fn parse_ref(s: &str) -> Option<(&str, String)> {
    let body = s.strip_prefix("@+").or_else(|| s.strip_prefix('@'))?;
    let (ty, name) = body.split_once('/')?;
    Some((ty, name.replace('.', "_")))
}

// ── Values ────────────────────────────────────────────────────────────────────

/// Raw `res/values` entries of one type: R name → `(text, where)`.
type RawValues = BTreeMap<String, (String, String)>;

struct Values {
    raw: BTreeMap<u8, RawValues>,
}

fn value_type(tag: &str) -> Option<u8> {
    Some(match tag {
        "string" => TYPE_STRING,
        "color" => TYPE_COLOR,
        "dimen" => TYPE_DIMEN,
        "integer" => TYPE_INTEGER,
        "bool" => TYPE_BOOL,
        _ => return None,
    })
}

impl Values {
    fn load(dir: &Path) -> Result<Self, String> {
        let mut raw: BTreeMap<u8, RawValues> = BTreeMap::new();
        for path in sorted_files(dir, "xml")? {
            let root = parse_xml(&path)?;
            let at = path.display();
            if root.name != "resources" {
                return Err(format!(
                    "{at}: root element is <{}>, expected <resources>",
                    root.name
                ));
            }
            for el in &root.children {
                let Some(ty) = value_type(&el.name) else {
                    return Err(format!(
                        "{at}: <{}> is not supported (string, color, dimen, integer, bool)",
                        el.name
                    ));
                };
                let name = el
                    .attr("name")
                    .ok_or_else(|| format!("{at}: <{}> without a name", el.name))?;
                let name = r_name(name, &format!("{at}: <{}>", el.name))?;
                if !el.children.is_empty() {
                    return Err(format!(
                        "{at}: <{} name=\"{name}\"> contains markup; only plain text is supported",
                        el.name
                    ));
                }
                let where_ = format!("{at}: <{} name=\"{name}\">", el.name);
                if let Some((_, first)) = raw
                    .entry(ty)
                    .or_default()
                    .insert(name.clone(), (el.text.clone(), where_.clone()))
                {
                    return Err(format!("{where_} is already defined at {first}"));
                }
            }
        }
        Ok(Self { raw })
    }

    fn names(&self, ty: u8) -> impl Iterator<Item = &String> {
        self.raw.get(&ty).into_iter().flat_map(|m| m.keys())
    }

    /// The text of `ty/name` with `@ty/other` aliases followed to the end.
    fn resolve_text(&self, ty: u8, name: &str, from: &str) -> Result<&str, String> {
        let type_name = fmt::type_name(ty).unwrap_or("?");
        let mut name = name.to_string();
        for _ in 0..16 {
            let (text, _) = self
                .raw
                .get(&ty)
                .and_then(|m| m.get(&name))
                .ok_or_else(|| format!("{from}: @{type_name}/{name} is not defined"))?;
            let text = text.trim();
            match parse_ref(text) {
                Some((t, next)) => {
                    if t != type_name {
                        return Err(format!(
                            "{from}: @{type_name}/{name} refers to @{t}/{next}, a different type"
                        ));
                    }
                    name = next;
                }
                None => return Ok(text),
            }
        }
        Err(format!("{from}: @{type_name}/{name} is a reference cycle"))
    }

    /// `text` as a value of `ty`: a literal, or an `@ty/name` reference.
    fn word(&self, ty: u8, text: &str, from: &str) -> Result<u32, String> {
        let text = text.trim();
        let literal = match parse_ref(text) {
            Some((t, name)) => {
                if Some(t) != fmt::type_name(ty) {
                    return Err(format!(
                        "{from}: '{text}' is not a @{} reference",
                        fmt::type_name(ty).unwrap_or("?")
                    ));
                }
                self.resolve_text(ty, &name, from)?.trim()
            }
            None => text,
        };
        match ty {
            TYPE_COLOR => parse_color(literal).ok_or_else(|| {
                format!("{from}: '{literal}' is not a color (#RGB, #ARGB, #RRGGBB, #AARRGGBB)")
            }),
            TYPE_DIMEN => parse_dimen(literal)
                .map(f32::to_bits)
                .map_err(|e| format!("{from}: {e}")),
            TYPE_INTEGER => parse_int(literal)
                .map(|v| v as u32)
                .ok_or_else(|| format!("{from}: '{literal}' is not an integer")),
            TYPE_BOOL => parse_bool(literal)
                .map(u32::from)
                .ok_or_else(|| format!("{from}: '{literal}' is not true or false")),
            _ => unreachable!("word() is for value-word types"),
        }
    }
}

// ── Layouts ───────────────────────────────────────────────────────────────────

const MATCH_PARENT: i32 = -1;
const WRAP_CONTENT: i32 = -2;

/// `picodroid.view.Gravity`; a papk-pack test holds these to the Java file.
pub(crate) const GRAVITY: &[(&str, u32)] = &[
    ("top", 0x30),
    ("bottom", 0x50),
    ("left", 0x03),
    ("right", 0x05),
    ("center_vertical", 0x10),
    ("center_horizontal", 0x01),
    ("center", 0x11),
    ("fill_vertical", 0x70),
    ("fill_horizontal", 0x07),
    ("fill", 0x77),
    ("start", 0x0080_0003),
    ("end", 0x0080_0005),
];

/// `picodroid.text.InputType`, by Android's `android:inputType` spellings.
pub(crate) const INPUT_TYPE: &[(&str, u32)] = &[
    ("text", 0x01),
    ("number", 0x02),
    ("phone", 0x03),
    ("datetime", 0x04),
    ("textUri", 0x11),
    ("textEmailAddress", 0x21),
    ("textPassword", 0x81),
    ("numberSigned", 0x1002),
    ("numberDecimal", 0x2002),
];

/// `picodroid.widget.ImageView.SCALE_*`.
pub(crate) const SCALE_TYPE: &[(&str, u32)] = &[
    ("fitCenter", 0),
    ("centerCrop", 1),
    ("fitXY", 2),
    ("center", 4),
];

const VISIBILITY: &[(&str, u32)] = &[("visible", 0), ("invisible", 4), ("gone", 8)];
const ORIENTATION: &[(&str, u32)] = &[("horizontal", 0), ("vertical", 1)];
const ELLIPSIZE: &[(&str, u32)] = &[
    ("none", 0),
    ("start", 1),
    ("middle", 2),
    ("end", 3),
    ("marquee", 4),
];

fn parse_enum(table: &[(&str, u32)], text: &str, from: &str) -> Result<u32, String> {
    table
        .iter()
        .find(|(n, _)| *n == text.trim())
        .map(|&(_, v)| v)
        .ok_or_else(|| {
            let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
            format!("{from}: '{text}' is not one of {}", names.join(", "))
        })
}

fn parse_flags(table: &[(&str, u32)], text: &str, from: &str) -> Result<u32, String> {
    text.split('|')
        .try_fold(0, |acc, part| Ok(acc | parse_enum(table, part, from)?))
}

struct LayoutCompiler<'a> {
    values: &'a Values,
    /// String R name → entry index.
    string_index: &'a BTreeMap<String, u16>,
    /// Literal layout strings, pooled after the named ones.
    pooled: &'a mut Vec<String>,
    named_strings: u16,
    ids: &'a BTreeMap<String, u16>,
    drawable_index: &'a BTreeMap<String, u16>,
    warnings: &'a mut Vec<String>,
}

impl LayoutCompiler<'_> {
    fn string_id(&mut self, text: &str, from: &str) -> Result<u32, String> {
        if let Some((ty, name)) = parse_ref(text.trim()) {
            if ty != "string" {
                return Err(format!("{from}: '{text}' is not a @string reference"));
            }
            let index = self
                .string_index
                .get(&name)
                .ok_or_else(|| format!("{from}: @string/{name} is not defined"))?;
            return Ok(fmt::res_id(TYPE_STRING, *index));
        }
        let literal = unescape_string(text).map_err(|e| format!("{from}: {e}"))?;
        let slot = match self.pooled.iter().position(|s| *s == literal) {
            Some(i) => i,
            None => {
                self.pooled.push(literal);
                self.pooled.len() - 1
            }
        };
        let index = u16::try_from(self.named_strings as usize + slot)
            .map_err(|_| format!("{from}: too many strings"))?;
        Ok(fmt::res_id(TYPE_STRING, index))
    }

    fn pixels(&self, text: &str, from: &str) -> Result<u32, String> {
        let px = f32::from_bits(self.values.word(TYPE_DIMEN, text, from)?);
        Ok(px.round() as i32 as u32)
    }

    fn size(&self, text: &str, from: &str) -> Result<u32, String> {
        match text.trim() {
            "match_parent" | "fill_parent" => Ok(MATCH_PARENT as u32),
            "wrap_content" => Ok(WRAP_CONTENT as u32),
            other => self.pixels(other, from),
        }
    }

    fn float(&self, text: &str, from: &str) -> Result<u32, String> {
        text.trim()
            .parse::<f32>()
            .map(f32::to_bits)
            .map_err(|_| format!("{from}: '{text}' is not a number"))
    }

    /// One XML attribute → zero or more `(attr, value)` words.
    fn attr(&mut self, name: &str, text: &str, from: &str) -> Result<Vec<(u32, u32)>, String> {
        use layout::attr as a;
        let v = self.values;
        let one = |code: u32, value: u32| Ok(vec![(code, value)]);
        match name {
            "id" => {
                let (ty, id) = parse_ref(text.trim())
                    .ok_or_else(|| format!("{from}: '{text}' is not @+id/name"))?;
                if ty != "id" {
                    return Err(format!("{from}: '{text}' is not an @id reference"));
                }
                let index = self
                    .ids
                    .get(&id)
                    .ok_or_else(|| format!("{from}: @id/{id} is not defined"))?;
                one(a::ID, fmt::res_id(TYPE_ID, *index))
            }
            "layout_width" => one(a::LAYOUT_WIDTH, self.size(text, from)?),
            "layout_height" => one(a::LAYOUT_HEIGHT, self.size(text, from)?),
            "layout_weight" => one(a::LAYOUT_WEIGHT, self.float(text, from)?),
            "layout_gravity" => one(a::LAYOUT_GRAVITY, parse_flags(GRAVITY, text, from)?),
            "padding" => {
                let p = self.pixels(text, from)?;
                Ok(vec![
                    (a::PADDING_LEFT, p),
                    (a::PADDING_TOP, p),
                    (a::PADDING_RIGHT, p),
                    (a::PADDING_BOTTOM, p),
                ])
            }
            "paddingHorizontal" => {
                let p = self.pixels(text, from)?;
                Ok(vec![(a::PADDING_LEFT, p), (a::PADDING_RIGHT, p)])
            }
            "paddingVertical" => {
                let p = self.pixels(text, from)?;
                Ok(vec![(a::PADDING_TOP, p), (a::PADDING_BOTTOM, p)])
            }
            "paddingLeft" | "paddingStart" => one(a::PADDING_LEFT, self.pixels(text, from)?),
            "paddingTop" => one(a::PADDING_TOP, self.pixels(text, from)?),
            "paddingRight" | "paddingEnd" => one(a::PADDING_RIGHT, self.pixels(text, from)?),
            "paddingBottom" => one(a::PADDING_BOTTOM, self.pixels(text, from)?),
            "background" => {
                if text.trim().starts_with("@drawable/") {
                    return Err(format!(
                        "{from}: drawable backgrounds are not supported; use a color"
                    ));
                }
                one(a::BACKGROUND, v.word(TYPE_COLOR, text, from)?)
            }
            "visibility" => one(a::VISIBILITY, parse_enum(VISIBILITY, text, from)?),
            "enabled" => one(a::ENABLED, v.word(TYPE_BOOL, text, from)?),
            "focusable" => one(a::FOCUSABLE, v.word(TYPE_BOOL, text, from)?),
            "alpha" => one(a::ALPHA, self.float(text, from)?),
            "text" => one(a::TEXT, self.string_id(text, from)?),
            "textColor" => one(a::TEXT_COLOR, v.word(TYPE_COLOR, text, from)?),
            "hint" => one(a::HINT, self.string_id(text, from)?),
            "singleLine" => one(a::SINGLE_LINE, v.word(TYPE_BOOL, text, from)?),
            "maxLines" => one(a::MAX_LINES, v.word(TYPE_INTEGER, text, from)?),
            "ellipsize" => one(a::ELLIPSIZE, parse_enum(ELLIPSIZE, text, from)?),
            "orientation" => one(a::ORIENTATION, parse_enum(ORIENTATION, text, from)?),
            "gravity" => one(a::GRAVITY, parse_flags(GRAVITY, text, from)?),
            "src" => {
                let (ty, name) = parse_ref(text.trim())
                    .ok_or_else(|| format!("{from}: '{text}' is not @drawable/name"))?;
                if ty != "drawable" {
                    return Err(format!("{from}: '{text}' is not a @drawable reference"));
                }
                let index = self
                    .drawable_index
                    .get(&name)
                    .ok_or_else(|| format!("{from}: @drawable/{name} is not defined"))?;
                one(a::SRC, fmt::res_id(TYPE_DRAWABLE, *index))
            }
            "scaleType" => one(a::SCALE_TYPE, parse_enum(SCALE_TYPE, text, from)?),
            "tint" => one(a::TINT, v.word(TYPE_COLOR, text, from)?),
            "checked" => one(a::CHECKED, v.word(TYPE_BOOL, text, from)?),
            "progress" => one(a::PROGRESS, v.word(TYPE_INTEGER, text, from)?),
            "max" => one(a::MAX, v.word(TYPE_INTEGER, text, from)?),
            "min" => one(a::MIN, v.word(TYPE_INTEGER, text, from)?),
            "progressTint" => one(a::PROGRESS_TINT, v.word(TYPE_COLOR, text, from)?),
            "progressBackgroundTint" => {
                one(a::PROGRESS_BACKGROUND_TINT, v.word(TYPE_COLOR, text, from)?)
            }
            "indeterminateTint" => one(a::INDETERMINATE_TINT, v.word(TYPE_COLOR, text, from)?),
            "inputType" => one(a::INPUT_TYPE, parse_flags(INPUT_TYPE, text, from)?),
            "textOn" => one(a::TEXT_ON, self.string_id(text, from)?),
            "textOff" => one(a::TEXT_OFF, self.string_id(text, from)?),
            _ => {
                self.warnings
                    .push(format!("{from}: attribute is not supported; ignored"));
                Ok(Vec::new())
            }
        }
    }

    fn node(&mut self, el: &Element, file: &str, out: &mut Vec<u32>) -> Result<(), String> {
        let class = layout::class::ALL
            .iter()
            .find(|(n, _)| *n == el.name)
            .map(|&(_, c)| c)
            .ok_or_else(|| {
                let hint = match el.name.as_str() {
                    "include" | "merge" => " (<include> and <merge> are not supported yet)",
                    n if n.contains('.') => " (custom views cannot be inflated: no reflection)",
                    _ => "",
                };
                let names: Vec<&str> = layout::class::ALL.iter().map(|(n, _)| *n).collect();
                format!(
                    "{file}: <{}> cannot be inflated{hint}. Supported: {}",
                    el.name,
                    names.join(", ")
                )
            })?;
        let mut words = Vec::new();
        for (prefix, name, text) in &el.attrs {
            // Design-time attributes never reach the device, as on Android.
            if prefix.as_deref() == Some("tools") {
                continue;
            }
            let from = format!("{file}: <{}> {name}=\"{text}\"", el.name);
            words.extend(self.attr(name, text, &from)?);
        }
        // Android reads `min`/`max` before `progress` whatever the XML order;
        // the inflater applies words in stream order, so put the range first.
        words.sort_by_key(|(code, _)| !matches!(*code, layout::attr::MIN | layout::attr::MAX));
        let attr_count = u8::try_from(words.len())
            .map_err(|_| format!("{file}: <{}> has too many attributes", el.name))?;
        let is_group = matches!(
            class,
            layout::class::LINEAR_LAYOUT
                | layout::class::FRAME_LAYOUT
                | layout::class::SCROLL_VIEW
                | layout::class::RADIO_GROUP
        );
        if !is_group && !el.children.is_empty() {
            return Err(format!(
                "{file}: <{}> is not a ViewGroup and cannot have children",
                el.name
            ));
        }
        let child_count = u16::try_from(el.children.len())
            .map_err(|_| format!("{file}: <{}> has too many children", el.name))?;
        out.push(layout::node_header(class, attr_count, child_count));
        for (code, value) in words {
            out.push(code);
            out.push(value);
        }
        for child in &el.children {
            self.node(child, file, out)?;
        }
        Ok(())
    }
}

/// Every `@+id/name` in a layout tree.
fn collect_ids(el: &Element, file: &str, ids: &mut BTreeSet<String>) -> Result<(), String> {
    if let Some(text) = el.attr("id") {
        if let Some(("id", name)) = parse_ref(text.trim()) {
            ids.insert(r_name(&name, &format!("{file}: id"))?);
        }
    }
    el.children
        .iter()
        .try_for_each(|c| collect_ids(c, file, ids))
}

// ── Entry points ──────────────────────────────────────────────────────────────

fn index_of<'a>(names: impl Iterator<Item = &'a String>) -> Result<BTreeMap<String, u16>, String> {
    names
        .enumerate()
        .map(|(i, n)| {
            let i = u16::try_from(i).map_err(|_| "too many resources of one type".to_string())?;
            Ok((n.clone(), i))
        })
        .collect()
}

/// Compile the `res/` tree at `res_dir`.
pub fn compile(res_dir: &Path) -> Result<Compiled, String> {
    let mut values_dir = None;
    let mut layout_dir = None;
    let mut drawable_dir = None;
    for entry in fs::read_dir(res_dir).map_err(|e| format!("{}: {e}", res_dir.display()))? {
        let path = entry
            .map_err(|e| format!("{}: {e}", res_dir.display()))?
            .path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        let slot = match name {
            "values" => &mut values_dir,
            "layout" => &mut layout_dir,
            "drawable" => &mut drawable_dir,
            _ => {
                let hint = if name.contains('-') {
                    " — there are no resource configurations (one display, one density, one \
                     locale), so qualified directories are not supported"
                } else {
                    ""
                };
                return Err(format!(
                    "{}: not a supported resource directory (values, layout, drawable){hint}",
                    path.display()
                ));
            }
        };
        if !path.is_dir() {
            return Err(format!("{}: expected a directory", path.display()));
        }
        *slot = Some(path);
    }

    let values = match &values_dir {
        Some(dir) => Values::load(dir)?,
        None => Values {
            raw: BTreeMap::new(),
        },
    };

    let mut drawables = Vec::new();
    let mut drawable_names = Vec::new();
    if let Some(dir) = &drawable_dir {
        for path in sorted_files(dir, "png")? {
            let name = file_res_name(&path)?;
            let file = path.file_name().unwrap().to_string_lossy();
            drawables.push((format!("{DRAWABLE_ASSET_PREFIX}{file}"), path.clone()));
            drawable_names.push(name);
        }
    }

    let mut layouts: Vec<(String, String, Element)> = Vec::new();
    let mut id_names = BTreeSet::new();
    if let Some(dir) = &layout_dir {
        for path in sorted_files(dir, "xml")? {
            let name = file_res_name(&path)?;
            let root = parse_xml(&path)?;
            let file = path.display().to_string();
            collect_ids(&root, &file, &mut id_names)?;
            layouts.push((name, file, root));
        }
    }
    let string_index = index_of(values.names(TYPE_STRING))?;
    let ids = index_of(id_names.iter())?;
    let drawable_index = index_of(drawable_names.iter())?;

    let mut table = ResTableBuilder::new();
    let mut symbols = Vec::new();
    let overflow = |e: papk_format::BuildError| format!("resource table: {e}");

    // Layouts first: they pool their literal strings, and the string type is
    // written named-then-pooled.
    let mut pooled = Vec::new();
    let mut warnings = Vec::new();
    let mut streams = Vec::new();
    {
        let mut lc = LayoutCompiler {
            values: &values,
            string_index: &string_index,
            pooled: &mut pooled,
            named_strings: string_index.len() as u16,
            ids: &ids,
            drawable_index: &drawable_index,
            warnings: &mut warnings,
        };
        for (_, file, root) in &layouts {
            let mut words = Vec::new();
            lc.node(root, file, &mut words)?;
            streams.push(words);
        }
    }

    for name in values.names(TYPE_STRING) {
        let from = &values.raw[&TYPE_STRING][name].1;
        let text = unescape_string(values.resolve_text(TYPE_STRING, name, from)?)
            .map_err(|e| format!("{from}: {e}"))?;
        let id = table.push_string(&text).map_err(overflow)?;
        symbols.push((TYPE_STRING, name.clone(), id));
    }
    for text in &pooled {
        table.push_string(text).map_err(overflow)?;
    }
    for ty in [TYPE_COLOR, TYPE_DIMEN, TYPE_INTEGER, TYPE_BOOL] {
        for name in values.names(ty) {
            let (text, from) = &values.raw[&ty][name];
            let id = table
                .push_value(ty, values.word(ty, text, from)?)
                .map_err(overflow)?;
            symbols.push((ty, name.clone(), id));
        }
    }
    for ((name, _, _), words) in layouts.iter().zip(streams) {
        let id = table.push_layout(words).map_err(overflow)?;
        symbols.push((TYPE_LAYOUT, name.clone(), id));
    }
    for (name, (asset, _)) in drawable_names.iter().zip(&drawables) {
        let id = table.push_drawable(asset).map_err(overflow)?;
        symbols.push((TYPE_DRAWABLE, name.clone(), id));
    }
    for (name, index) in &ids {
        symbols.push((TYPE_ID, name.clone(), fmt::res_id(TYPE_ID, *index)));
    }

    // Two names that differ only by '.' vs '_' collapse to one R field.
    for pair in symbols.windows(2) {
        if pair[0].0 == pair[1].0 && pair[0].1 == pair[1].1 {
            return Err(format!(
                "R.{}.{} is defined twice",
                fmt::type_name(pair[0].0).unwrap_or("?"),
                pair[0].1
            ));
        }
    }

    Ok(Compiled {
        table: if table.is_empty() {
            Vec::new()
        } else {
            table.build().map_err(overflow)?
        },
        symbols,
        drawables,
        warnings,
    })
}

/// The text of `<package>/R.java`.
pub fn r_java(package: &str, symbols: &[(u8, String, u32)]) -> String {
    let mut s = String::new();
    s.push_str("// GENERATED by papk-pack gen-r from res/ — do not edit.\n");
    let _ = writeln!(s, "package {package};\n");
    s.push_str(
        "/** Resource ids. Compile-time constants: javac inlines them, R is never packed. */\n",
    );
    s.push_str("public final class R {\n  private R() {}\n");
    let mut current = None;
    for (ty, name, id) in symbols {
        if current != Some(*ty) {
            if current.is_some() {
                s.push_str("  }\n");
            }
            let type_name = fmt::type_name(*ty).unwrap_or("unknown");
            // `R.string`, `R.bool` … are lower-case by Android convention.
            let _ = writeln!(
                s,
                "\n  @SuppressWarnings(\"TypeName\")\n  public static final class {type_name} {{\n    private {type_name}() {{}}\n"
            );
            current = Some(*ty);
        }
        let _ = writeln!(s, "    public static final int {name} = 0x{id:08x};");
    }
    if current.is_some() {
        s.push_str("  }\n");
    }
    s.push_str("}\n");
    s
}

/// `papk-pack gen-r --res-dir <dir> --package <java.package> --out-dir <dir>`.
pub fn gen_r_main(args: &[String]) -> Result<(), String> {
    let mut res_dir = None;
    let mut package = None;
    let mut out_dir = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--res-dir" => &mut res_dir,
            "--package" => &mut package,
            "--out-dir" => &mut out_dir,
            other => return Err(format!("gen-r: unknown argument: {other}")),
        };
        *slot = Some(
            it.next()
                .ok_or_else(|| format!("gen-r: {flag} requires a value"))?
                .clone(),
        );
    }
    let res_dir = PathBuf::from(res_dir.ok_or("gen-r: --res-dir is required")?);
    let package = package.ok_or("gen-r: --package is required")?;
    let out_dir = PathBuf::from(out_dir.ok_or("gen-r: --out-dir is required")?);

    let compiled = compile(&res_dir)?;
    let dir = out_dir.join(package.replace('.', "/"));
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join("R.java");
    fs::write(&file, r_java(&package, &compiled.symbols))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    eprintln!(
        "==> Wrote {} ({} resource ids)",
        file.display(),
        compiled.symbols.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use papk_format::res::ResTable;

    fn tree(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "papk-pack-res-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        for (rel, text) in files {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        dir
    }

    fn id_of(c: &Compiled, ty: u8, name: &str) -> u32 {
        c.symbols
            .iter()
            .find(|(t, n, _)| *t == ty && n == name)
            .unwrap_or_else(|| panic!("no symbol {name}"))
            .2
    }

    const VALUES: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app_name">Res Demo</string>
    <string name="greeting">  Hello,\n   "World"  </string>
    <string name="alias">@string/app_name</string>
    <string name="kept">"  two  spaces "</string>
    <color name="accent">#36C</color>
    <color name="scrim">#80000000</color>
    <color name="brand">@color/accent</color>
    <dimen name="gap">12dp</dimen>
    <dimen name="hair">0.5px</dimen>
    <integer name="answer">-42</integer>
    <integer name="mask">0xFF</integer>
    <bool name="debug">true</bool>
</resources>"##;

    #[test]
    fn values_compile_and_ids_follow_sorted_names() {
        let dir = tree(&[("values/values.xml", VALUES)]);
        let c = compile(&dir).unwrap();
        let t = ResTable::parse(&c.table).unwrap();

        // Sorted: alias, app_name, greeting, kept.
        assert_eq!(id_of(&c, TYPE_STRING, "alias"), 0x7f01_0000);
        assert_eq!(id_of(&c, TYPE_STRING, "app_name"), 0x7f01_0001);
        let s =
            |n: &str| std::str::from_utf8(t.string(id_of(&c, TYPE_STRING, n)).unwrap()).unwrap();
        assert_eq!(s("app_name"), "Res Demo");
        assert_eq!(s("alias"), "Res Demo");
        assert_eq!(s("greeting"), "Hello,\n \"World\"");
        assert_eq!(s("kept"), "  two  spaces ");

        let v = |ty: u8, n: &str| t.value_of(ty, id_of(&c, ty, n)).unwrap();
        assert_eq!(v(TYPE_COLOR, "accent"), 0xFF33_66CC);
        assert_eq!(v(TYPE_COLOR, "brand"), 0xFF33_66CC);
        assert_eq!(v(TYPE_COLOR, "scrim"), 0x8000_0000);
        assert_eq!(f32::from_bits(v(TYPE_DIMEN, "gap")), 12.0);
        assert_eq!(f32::from_bits(v(TYPE_DIMEN, "hair")), 0.5);
        assert_eq!(v(TYPE_INTEGER, "answer") as i32, -42);
        assert_eq!(v(TYPE_INTEGER, "mask"), 0xFF);
        assert_eq!(v(TYPE_BOOL, "debug"), 1);
        assert!(c.warnings.is_empty());
    }

    const LAYOUT: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<LinearLayout xmlns:android="http://schemas.android.com/apk/res/android"
    xmlns:tools="http://schemas.android.com/tools"
    android:layout_width="match_parent"
    android:layout_height="match_parent"
    android:orientation="vertical"
    android:padding="@dimen/gap"
    tools:context=".Main">
    <TextView
        android:id="@+id/title"
        android:layout_width="wrap_content"
        android:layout_height="wrap_content"
        android:text="@string/app_name"
        android:textColor="@color/accent"
        android:textSize="18sp" />
    <Button
        android:id="@+id/ok"
        android:layout_width="0dp"
        android:layout_height="40dp"
        android:layout_weight="1"
        android:gravity="center_vertical|right"
        android:text="OK" />
    <Button android:text="OK" />
</LinearLayout>"##;

    #[test]
    fn layout_compiles_to_the_documented_word_stream() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[("values/values.xml", VALUES), ("layout/main.xml", LAYOUT)]);
        let c = compile(&dir).unwrap();
        let t = ResTable::parse(&c.table).unwrap();
        let l = t.layout(id_of(&c, TYPE_LAYOUT, "main")).unwrap();
        let words: Vec<u32> = (0..l.len()).map(|i| l.word(i).unwrap()).collect();

        let app_name = id_of(&c, TYPE_STRING, "app_name");
        // "OK" is pooled once, after the four named strings.
        let ok_text = fmt::res_id(TYPE_STRING, 4);
        assert_eq!(t.string(ok_text), Some(&b"OK"[..]));
        assert_eq!(t.entry_count(TYPE_STRING), 5);
        let title = id_of(&c, TYPE_ID, "title");
        let ok = id_of(&c, TYPE_ID, "ok");
        assert_eq!((ok, title), (0x7f08_0000, 0x7f08_0001));

        #[rustfmt::skip]
        let expected = vec![
            node_header(k::LINEAR_LAYOUT, 7, 3),
            a::LAYOUT_WIDTH, -1i32 as u32,
            a::LAYOUT_HEIGHT, -1i32 as u32,
            a::ORIENTATION, 1,
            a::PADDING_LEFT, 12, a::PADDING_TOP, 12, a::PADDING_RIGHT, 12, a::PADDING_BOTTOM, 12,
            node_header(k::TEXT_VIEW, 5, 0),
            a::ID, title,
            a::LAYOUT_WIDTH, -2i32 as u32,
            a::LAYOUT_HEIGHT, -2i32 as u32,
            a::TEXT, app_name,
            a::TEXT_COLOR, 0xFF33_66CC,
            node_header(k::BUTTON, 6, 0),
            a::ID, ok,
            a::LAYOUT_WIDTH, 0,
            a::LAYOUT_HEIGHT, 40,
            a::LAYOUT_WEIGHT, 1.0f32.to_bits(),
            a::GRAVITY, 0x15,
            a::TEXT, ok_text,
            node_header(k::BUTTON, 1, 0),
            a::TEXT, ok_text,
        ];
        assert_eq!(words, expected);

        // textSize is reported, tools:context is not.
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.warnings[0].contains("textSize"));
    }

    #[test]
    fn range_attributes_precede_progress_whatever_the_xml_order() {
        use layout::{attr as a, class as k, node_header};
        const BAR: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<ProgressBar xmlns:android="http://schemas.android.com/apk/res/android"
    android:progress="150"
    android:progressTint="#FF00AA00"
    android:min="10"
    android:max="200" />
"##;
        let dir = tree(&[("values/values.xml", VALUES), ("layout/bar.xml", BAR)]);
        let c = compile(&dir).unwrap();
        let t = ResTable::parse(&c.table).unwrap();
        let l = t.layout(id_of(&c, TYPE_LAYOUT, "bar")).unwrap();
        let words: Vec<u32> = (0..l.len()).map(|i| l.word(i).unwrap()).collect();
        // Android reads min/max before progress; the inflater applies words in
        // order, so the compiler moves them first (the rest keep XML order).
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::PROGRESS_BAR, 4, 0),
            a::MIN, 10,
            a::MAX, 200,
            a::PROGRESS, 150,
            a::PROGRESS_TINT, 0xFF00_AA00,
        ];
        assert_eq!(words, expected);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    }

    #[test]
    fn r_java_lists_every_type() {
        let dir = tree(&[("values/values.xml", VALUES), ("layout/main.xml", LAYOUT)]);
        let text = r_java("resdemo", &compile(&dir).unwrap().symbols);
        assert!(text.contains("package resdemo;"));
        assert!(text.contains("public static final class string {"));
        assert!(text.contains("    public static final int app_name = 0x7f010001;"));
        assert!(text.contains("public static final class layout {"));
        assert!(text.contains("    public static final int main = 0x7f060000;"));
        assert!(text.contains("public static final class id {"));
        assert!(text.contains("    public static final int title = 0x7f080001;"));
    }

    #[test]
    fn mistakes_are_build_errors_that_name_the_place() {
        let err = |files: &[(&str, &str)]| compile(&tree(files)).unwrap_err();
        let wrap = |body: &str| format!("<resources>{body}</resources>");

        assert!(err(&[("values-night/c.xml", "<resources/>")]).contains("configurations"));
        assert!(
            err(&[("values/a.xml", &wrap(r#"<color name="c">red</color>"#))])
                .contains("not a color")
        );
        assert!(
            err(&[("values/a.xml", &wrap(r#"<dimen name="d">4pt</dimen>"#))])
                .contains("'pt' is not supported")
        );
        assert!(
            err(&[("values/a.xml", &wrap(r#"<dimen name="d">4</dimen>"#))])
                .contains("needs a unit")
        );
        assert!(err(&[(
            "values/a.xml",
            &wrap(r#"<string name="s">a</string><string name="s">b</string>"#)
        )])
        .contains("already defined"));
        assert!(
            err(&[("values/a.xml", &wrap(r#"<string name="class">a</string>"#))])
                .contains("not a usable resource name")
        );
        assert!(err(&[(
            "values/a.xml",
            &wrap(r#"<color name="a">@color/b</color><color name="b">@color/a</color>"#)
        )])
        .contains("cycle"));
        assert!(err(&[("values/a.xml", &wrap(r#"<style name="s"/>"#))]).contains("not supported"));
        assert!(err(&[("layout/Main.xml", "<TextView/>")]).contains("[a-z0-9_]"));
        assert!(err(&[("layout/m.xml", "<com.example.Dial/>")]).contains("no reflection"));
        assert!(
            err(&[("layout/m.xml", "<TextView><Button/></TextView>")]).contains("not a ViewGroup")
        );
        assert!(
            err(&[("layout/m.xml", r#"<TextView text="@string/nope"/>"#)])
                .contains("@string/nope is not defined")
        );
        assert!(
            err(&[("layout/m.xml", r#"<ImageView src="@drawable/nope"/>"#)])
                .contains("@drawable/nope is not defined")
        );
    }

    #[test]
    fn an_empty_tree_is_no_section() {
        let dir = tree(&[("values/empty.xml", "<resources/>")]);
        let c = compile(&dir).unwrap();
        assert!(c.table.is_empty() && c.symbols.is_empty());
    }

    fn java_consts(rel: &str) -> BTreeMap<String, i64> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk/java/picodroid")
            .join(rel);
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut out = BTreeMap::new();
        for line in text.lines() {
            let Some(rest) = line.trim().split("static final int ").nth(1) else {
                continue;
            };
            let Some((name, value)) = rest.trim_end_matches(';').split_once(" = ") else {
                continue;
            };
            let value = value.trim();
            let parsed = match value.strip_prefix("0x") {
                Some(hex) => i64::from_str_radix(hex, 16).ok(),
                None => value.parse().ok(),
            };
            if let Some(v) = parsed {
                out.insert(name.trim().to_string(), v);
            }
        }
        out
    }

    /// The compiler and the SDK spell the same numbers in two languages;
    /// this is what keeps them from drifting.
    #[test]
    fn codes_match_the_java_sdk() {
        // LayoutInflater spells its codes as `case 15: // ATTR_TEXT` (see the
        // comment there for why they are not constants).
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk/java/picodroid/view/LayoutInflater.java");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut inflater = BTreeMap::new();
        for line in text.lines() {
            let Some((case, name)) = line.trim().split_once(": // ") else {
                continue;
            };
            let Some(code) = case
                .strip_prefix("case ")
                .and_then(|n| n.parse::<i64>().ok())
            else {
                continue;
            };
            assert!(
                inflater.insert(name.trim().to_string(), code).is_none(),
                "{name} labelled twice"
            );
        }
        for (name, code) in layout::attr::ALL {
            assert_eq!(
                inflater.get(&format!("ATTR_{name}")),
                Some(&(*code as i64)),
                "LayoutInflater ATTR_{name}"
            );
        }
        let attr_consts = inflater.keys().filter(|k| k.starts_with("ATTR_")).count();
        assert_eq!(attr_consts, layout::attr::ALL.len(), "stale ATTR_ label");
        for (element, code) in layout::class::ALL {
            let mut konst = String::from("CLASS");
            for c in element.chars() {
                if c.is_ascii_uppercase() {
                    konst.push('_');
                }
                konst.push(c.to_ascii_uppercase());
            }
            assert_eq!(
                inflater.get(&konst),
                Some(&(*code as i64)),
                "LayoutInflater {konst}"
            );
        }
        let class_consts = inflater.keys().filter(|k| k.starts_with("CLASS_")).count();
        assert_eq!(class_consts, layout::class::ALL.len(), "stale CLASS_ label");

        let gravity = java_consts("view/Gravity.java");
        for (name, value) in GRAVITY {
            let java = match *name {
                "center" => gravity["CENTER_VERTICAL"] | gravity["CENTER_HORIZONTAL"],
                "fill" => gravity["FILL_VERTICAL"] | gravity["FILL_HORIZONTAL"],
                n => gravity[&n.to_ascii_uppercase()],
            };
            assert_eq!(java, *value as i64, "Gravity {name}");
        }
        let input = java_consts("text/InputType.java");
        let it = |n: &str| input[n] as u32;
        let expect = [
            ("text", it("TYPE_CLASS_TEXT")),
            ("number", it("TYPE_CLASS_NUMBER")),
            ("phone", it("TYPE_CLASS_PHONE")),
            ("datetime", it("TYPE_CLASS_DATETIME")),
            (
                "textUri",
                it("TYPE_CLASS_TEXT") | it("TYPE_TEXT_VARIATION_URI"),
            ),
            (
                "textEmailAddress",
                it("TYPE_CLASS_TEXT") | it("TYPE_TEXT_VARIATION_EMAIL_ADDRESS"),
            ),
            (
                "textPassword",
                it("TYPE_CLASS_TEXT") | it("TYPE_TEXT_VARIATION_PASSWORD"),
            ),
            (
                "numberSigned",
                it("TYPE_CLASS_NUMBER") | it("TYPE_NUMBER_FLAG_SIGNED"),
            ),
            (
                "numberDecimal",
                it("TYPE_CLASS_NUMBER") | it("TYPE_NUMBER_FLAG_DECIMAL"),
            ),
        ];
        assert_eq!(INPUT_TYPE, &expect[..]);
        let image = java_consts("widget/ImageView.java");
        for (name, java) in [
            ("fitCenter", "SCALE_FIT_CENTER"),
            ("centerCrop", "SCALE_CENTER_CROP"),
            ("fitXY", "SCALE_FIT_XY"),
            ("center", "SCALE_CENTER"),
        ] {
            let ours = SCALE_TYPE.iter().find(|(n, _)| *n == name).unwrap().1;
            assert_eq!(image[java], ours as i64, "ImageView.{java}");
        }
        let view = java_consts("view/View.java");
        assert_eq!(
            (view["VISIBLE"], view["INVISIBLE"], view["GONE"]),
            (0, 4, 8)
        );
        let group = java_consts("view/ViewGroup.java");
        // Nested in ViewGroup.LayoutParams.
        assert_eq!(group["MATCH_PARENT"], MATCH_PARENT as i64);
        assert_eq!(group["WRAP_CONTENT"], WRAP_CONTENT as i64);
    }

    // ── The value grammar ────────────────────────────────────────────────
    // Wrong here is silent: a colour or a dimension that parses to the wrong
    // number ships in every app's resource table and nothing fails.

    #[test]
    fn colours_expand_every_android_shorthand_to_argb() {
        assert_eq!(parse_color("#fff"), Some(0xffff_ffff));
        assert_eq!(parse_color("#1a2"), Some(0xff11_aa22));
        assert_eq!(parse_color("#8fff"), Some(0x88ff_ffff));
        assert_eq!(parse_color("#336699"), Some(0xff33_6699));
        assert_eq!(parse_color("#80336699"), Some(0x8033_6699));
        assert_eq!(parse_color("#ABCDEF"), parse_color("#abcdef"));
    }

    #[test]
    fn malformed_colours_are_refused_not_guessed() {
        for bad in [
            "336699",
            "#",
            "#12",
            "#12345",
            "#1234567",
            "#123456789",
            "#ggg",
            "# fff",
        ] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn dimensions_need_a_known_unit() {
        assert_eq!(parse_dimen("12dp"), Ok(12.0));
        assert_eq!(parse_dimen("12dip"), Ok(12.0));
        assert_eq!(parse_dimen("1.5sp"), Ok(1.5));
        assert_eq!(parse_dimen("-4px"), Ok(-4.0));
        assert!(parse_dimen("12").unwrap_err().contains("needs a unit"));
        assert!(parse_dimen("12pt").unwrap_err().contains("not supported"));
        assert!(parse_dimen("dp").unwrap_err().contains("not a dimension"));
        assert!(parse_dimen("1-2dp")
            .unwrap_err()
            .contains("not a dimension"));
    }

    #[test]
    fn integers_are_decimal_or_hex_and_hex_wraps_like_java() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("-7"), Some(-7));
        assert_eq!(parse_int("0x10"), Some(16));
        // 0xFFFFFFFF is -1 as a Java int, which is what R values are.
        assert_eq!(parse_int("0XFFFFFFFF"), Some(-1));
        assert_eq!(parse_int("0x1FFFFFFFF"), None);
        assert_eq!(parse_int("4.5"), None);
        assert_eq!(parse_int(""), None);
    }

    #[test]
    fn booleans_are_exactly_true_or_false() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("True"), None);
        assert_eq!(parse_bool("1"), None);
    }

    #[test]
    fn unquoted_strings_collapse_whitespace_and_quoted_ones_keep_it() {
        // Collapsing happens before unescaping, so an escaped newline survives.
        assert_eq!(unescape_string(r"  two   words\n ").unwrap(), "two words\n");
        assert_eq!(
            unescape_string("\"  kept   as is  \"").unwrap(),
            "  kept   as is  "
        );
    }

    #[test]
    fn string_escapes_follow_android() {
        assert_eq!(
            unescape_string(r"line\nbreak\ttab").unwrap(),
            "line\nbreak\ttab"
        );
        assert_eq!(
            unescape_string(r"it\'s \@literal \?too \\").unwrap(),
            "it's @literal ?too \\"
        );
        assert_eq!(unescape_string(r"\u00e9").unwrap(), "\u{e9}");
        assert!(unescape_string(r"\u00")
            .unwrap_err()
            .contains("bad \\u escape"));
        assert!(unescape_string(r"\ud800")
            .unwrap_err()
            .contains("bad \\u escape"));
        assert!(unescape_string(r"\q")
            .unwrap_err()
            .contains("unknown escape"));
        assert!(unescape_string("oops\\")
            .unwrap_err()
            .contains("trailing backslash"));
    }

    #[test]
    fn references_split_type_and_name_and_flatten_dots() {
        assert_eq!(
            parse_ref("@string/app_name"),
            Some(("string", "app_name".to_string()))
        );
        assert_eq!(
            parse_ref("@+id/ok.button"),
            Some(("id", "ok_button".to_string()))
        );
        assert_eq!(parse_ref("@color/"), Some(("color", String::new())));
        assert_eq!(parse_ref("string/app_name"), None);
        assert_eq!(parse_ref("@noslash"), None);
    }
}
