// SPDX-License-Identifier: GPL-3.0-only
//! The resource compiler: an app's `res/` tree in, a RESOURCES section and
//! an `R.java` out.
//!
//! ```text
//!   res/values/*.xml     <string> <color> <dimen> <integer> <bool> <style>
//!   res/layout/*.xml     one view tree per file
//!   res/drawable/*.png   packed into ASSETS as "res/drawable/<file>"
//!   res/drawable/*.xml   <shape> backgrounds, flattened into the layouts that use them
//! ```
//!
//! Both consumers — `papk-pack gen-r` before `compileJava`, `papk-pack
//! --res-dir` after it — run [`compile`] over the same directory, and ids
//! come out of sorted names, so the `R.java` an app was compiled against and
//! the table it is packed with agree by construction rather than through a
//! file passed between them.
//!
//! One density (`dp` = `sp` = `px`) and one locale, and a closed subset of
//! configuration qualifiers for `values-…` and `layout-…` directories
//! (`papk_format::res::config`: `sw<N>dp`, `w<N>dp`, `h<N>dp`, `land`,
//! `port`, `notouch`, `finger`), compiled to override blocks the runtime
//! ranks once per launch. A `values-night/` or `drawable-hdpi/` directory is an
//! error rather than something silently ignored. Every reference
//! (`@color/accent`, `@dimen/gap`, `?attr/colorPrimary`) is resolved here, at
//! build time; the only thing a layout leaves for the device to look up is a
//! string.
//!
//! Styles and the theme are a build-time matter too. `style="@style/Label"`
//! on a view is expanded into the style's attributes, and `?attr/name` reads
//! the item `name` of the app's theme, which is the `<style>` called
//! [`APP_THEME`]. What reaches the device of a `<style>` is its
//! `R.style` id and the few colours the framework's widgets default to
//! (`papk_format::res::theme`), for `Context.setTheme`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use papk_format::res::{
    self as fmt,
    config::{QualifierError, Qualifiers},
    layout, theme, OverrideValue, ResTableBuilder, TYPE_BOOL, TYPE_COLOR, TYPE_DIMEN,
    TYPE_DRAWABLE, TYPE_ID, TYPE_INTEGER, TYPE_LAYOUT, TYPE_STRING, TYPE_STYLE,
};

/// The ASSETS-entry prefix of a `res/drawable/` image.
pub const DRAWABLE_ASSET_PREFIX: &str = "res/drawable/";

/// The `<style>` that is the app's theme: what `?attr/…` reads.
pub const APP_THEME: &str = "AppTheme";

/// How deep an `<include>`, a style's parents or a `?attr` chain may go
/// before it is taken for a cycle.
const MAX_NESTING: u32 = 16;

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

#[derive(Clone)]
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

/// One `<style>`: where it is, its explicit parent and its items in file
/// order, each `(name without any "android:" prefix, text)`.
#[derive(Clone)]
struct Style {
    at: String,
    parent: Option<String>,
    items: Vec<(String, String)>,
}

struct Values {
    raw: BTreeMap<u8, RawValues>,
    /// By the name the XML gives, dots and all.
    styles: BTreeMap<String, Style>,
}

/// An attribute or item name without its `android:` prefix.
fn bare(name: &str) -> &str {
    name.strip_prefix("android:").unwrap_or(name)
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
        let mut styles: BTreeMap<String, Style> = BTreeMap::new();
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
                if el.name == "style" {
                    let style = parse_style(el, &at.to_string())?;
                    let name = el.attr("name").unwrap_or_default().to_string();
                    if let Some(first) = styles.get(&name) {
                        return Err(format!("{} is already defined at {}", style.at, first.at));
                    }
                    styles.insert(name, style);
                    continue;
                }
                let Some(ty) = value_type(&el.name) else {
                    return Err(format!(
                        "{at}: <{}> is not supported (string, color, dimen, integer, bool, style)",
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
        Ok(Self { raw, styles })
    }

    /// These values with `other`'s laid over them: what a configuration
    /// variant's references resolve against (its own redefinitions first,
    /// then the base), as Android resolves them at run time.
    fn overlay(&self, other: &Values) -> Values {
        let mut raw = self.raw.clone();
        for (ty, names) in &other.raw {
            raw.entry(*ty).or_default().extend(names.clone());
        }
        Values {
            raw,
            styles: self.styles.clone(),
        }
    }

    /// The text of `item` in `style`, looked for in the style itself and then
    /// up its parents: the explicit `parent`, else the name before the last
    /// dot (`Label.Faint` inherits `Label`), as on Android. A parent that is
    /// not one of the app's styles (a framework theme) contributes nothing.
    fn style_item(&self, style: &str, item: &str) -> Option<&str> {
        let mut name = style.to_string();
        for _ in 0..MAX_NESTING {
            let s = self.styles.get(&name)?;
            if let Some((_, text)) = s.items.iter().rev().find(|(n, _)| n == item) {
                return Some(text.trim());
            }
            name = self.style_parent(&name, s)?;
        }
        None
    }

    fn style_parent(&self, name: &str, style: &Style) -> Option<String> {
        match &style.parent {
            Some(p) => {
                let p = p.strip_prefix("@style/").unwrap_or(p);
                (!p.is_empty()).then(|| p.to_string())
            }
            None => name.rsplit_once('.').map(|(head, _)| head.to_string()),
        }
    }

    /// Every item `style` ends up with: its parents' and its own, a child's
    /// item in place of a parent's of the same name.
    fn style_items(&self, style: &str, from: &str) -> Result<Vec<(String, String)>, String> {
        let mut chain = Vec::new();
        let mut name = style.to_string();
        loop {
            let Some(s) = self.styles.get(&name) else {
                if chain.is_empty() {
                    return Err(format!("{from}: @style/{style} is not defined"));
                }
                break; // a framework parent: nothing of it lives here
            };
            if chain.len() as u32 >= MAX_NESTING {
                return Err(format!("{from}: @style/{style} inherits from itself"));
            }
            chain.push(s);
            match self.style_parent(&name, s) {
                Some(parent) => name = parent,
                None => break,
            }
        }
        let mut items: Vec<(String, String)> = Vec::new();
        for (name, text) in chain.iter().rev().flat_map(|s| s.items.iter()) {
            items.retain(|(n, _)| n != name);
            items.push((name.clone(), text.clone()));
        }
        Ok(items)
    }

    /// `?attr/name`, `?android:attr/name` or `?name` → the theme's text for
    /// `name`. `None` when `text` is not a theme reference.
    fn theme_text(&self, text: &str, from: &str) -> Option<Result<&str, String>> {
        let attr = text.strip_prefix('?')?;
        let name = attr
            .strip_prefix("android:attr/")
            .or_else(|| attr.strip_prefix("attr/"))
            .or_else(|| attr.strip_prefix("android:"))
            .unwrap_or(attr);
        Some(self.style_item(APP_THEME, name).ok_or_else(|| {
            if self.styles.contains_key(APP_THEME) {
                format!("{from}: the theme (<style name=\"{APP_THEME}\">) has no item '{name}'")
            } else {
                format!(
                    "{from}: '{text}' reads the app's theme, and there is no \
                     <style name=\"{APP_THEME}\"> in res/values"
                )
            }
        }))
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

    /// `text` as a value of `ty`: a literal, an `@ty/name` reference, or a
    /// `?attr/name` reference into the theme.
    fn word(&self, ty: u8, text: &str, from: &str) -> Result<u32, String> {
        let mut text = text.trim();
        for _ in 0..MAX_NESTING {
            match self.theme_text(text, from) {
                Some(themed) => text = themed?,
                None => break,
            }
        }
        if text.starts_with('?') {
            return Err(format!("{from}: '{text}' is a theme reference cycle"));
        }
        let literal = match parse_ref(text) {
            // The framework colours a layout can name without declaring them:
            // `@android:color/transparent` is how Android spells a flat container.
            Some(("android:color", name)) if ty == TYPE_COLOR => match name.as_str() {
                "transparent" => return Ok(0x0000_0000),
                "black" => return Ok(0xFF00_0000),
                "white" => return Ok(0xFFFF_FFFF),
                _ => {
                    return Err(format!(
                        "{from}: '{text}' is not a framework colour picodroid ships \
                         (transparent, black, white); use a @color/ of your own"
                    ))
                }
            },
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

fn parse_style(el: &Element, file: &str) -> Result<Style, String> {
    let name = el
        .attr("name")
        .ok_or_else(|| format!("{file}: <style> without a name"))?;
    r_name(name, &format!("{file}: <style>"))?;
    let at = format!("{file}: <style name=\"{name}\">");
    let mut items = Vec::new();
    for item in &el.children {
        if item.name != "item" {
            return Err(format!("{at}: <{}> is not an <item>", item.name));
        }
        let item_name = item
            .attr("name")
            .ok_or_else(|| format!("{at}: <item> without a name"))?;
        items.push((bare(item_name).to_string(), item.text.trim().to_string()));
    }
    Ok(Style {
        at,
        parent: el.attr("parent").map(str::to_string),
        items,
    })
}

// ── Shape drawables ───────────────────────────────────────────────────────────

/// A `res/drawable/*.xml` `<shape>`: a filled rectangle with optional round
/// corners and a stroke. It exists only at build time: a layout that names
/// it as a background carries these four numbers instead.
struct Shape {
    color: u32,
    radius: u32,
    stroke_width: u32,
    stroke_color: u32,
}

fn parse_shape(path: &Path, values: &Values) -> Result<Shape, String> {
    let root = parse_xml(path)?;
    let at = path.display().to_string();
    if root.name != "shape" {
        return Err(format!(
            "{at}: <{}> is not supported; an XML drawable is a <shape>",
            root.name
        ));
    }
    if let Some(kind) = root.attr("shape") {
        if kind != "rectangle" {
            return Err(format!(
                "{at}: shape=\"{kind}\" is not supported (rectangle, with <corners> for a \
                 rounded one)"
            ));
        }
    }
    let mut shape = Shape {
        color: 0,
        radius: 0,
        stroke_width: 0,
        stroke_color: 0xFF00_0000,
    };
    let px = |text: &str, from: &str| -> Result<u32, String> {
        let v = f32::from_bits(values.word(TYPE_DIMEN, text, from)?);
        Ok(v.round().max(0.0) as u32)
    };
    for el in &root.children {
        let from = format!("{at}: <{}>", el.name);
        let need = |attr: &str| {
            el.attr(attr)
                .ok_or_else(|| format!("{from} needs android:{attr}"))
        };
        match el.name.as_str() {
            "solid" => shape.color = values.word(TYPE_COLOR, need("color")?, &from)?,
            "corners" => shape.radius = px(need("radius")?, &from)?,
            "stroke" => {
                shape.stroke_width = px(need("width")?, &from)?;
                shape.stroke_color = values.word(TYPE_COLOR, need("color")?, &from)?;
            }
            other => {
                return Err(format!(
                    "{at}: <{other}> is not supported in a <shape> (solid, corners, stroke)"
                ))
            }
        }
    }
    Ok(shape)
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
    shapes: &'a BTreeMap<String, Shape>,
    /// Every layout by name, for `<include>`: `(file, root element)`.
    layouts: &'a BTreeMap<String, (String, Element)>,
    warnings: &'a mut Vec<String>,
    /// A custom view's class name as the packed class files spell it: the
    /// `--shrink-app` map renames the app's classes, and the inflater looks
    /// the name up at run time (`Class.forName`), so the layout must carry
    /// the renamed one. `None` keeps the XML spelling.
    rename_class: &'a dyn Fn(&str) -> Option<String>,
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
            "layout_margin" => {
                let m = self.pixels(text, from)?;
                Ok(vec![
                    (a::LAYOUT_MARGIN_LEFT, m),
                    (a::LAYOUT_MARGIN_TOP, m),
                    (a::LAYOUT_MARGIN_RIGHT, m),
                    (a::LAYOUT_MARGIN_BOTTOM, m),
                ])
            }
            "layout_marginHorizontal" => {
                let m = self.pixels(text, from)?;
                Ok(vec![
                    (a::LAYOUT_MARGIN_LEFT, m),
                    (a::LAYOUT_MARGIN_RIGHT, m),
                ])
            }
            "layout_marginVertical" => {
                let m = self.pixels(text, from)?;
                Ok(vec![
                    (a::LAYOUT_MARGIN_TOP, m),
                    (a::LAYOUT_MARGIN_BOTTOM, m),
                ])
            }
            "layout_marginLeft" | "layout_marginStart" => {
                one(a::LAYOUT_MARGIN_LEFT, self.pixels(text, from)?)
            }
            "layout_marginTop" => one(a::LAYOUT_MARGIN_TOP, self.pixels(text, from)?),
            "layout_marginRight" | "layout_marginEnd" => {
                one(a::LAYOUT_MARGIN_RIGHT, self.pixels(text, from)?)
            }
            "layout_marginBottom" => one(a::LAYOUT_MARGIN_BOTTOM, self.pixels(text, from)?),
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
                if let Some(name) = text.trim().strip_prefix("@drawable/") {
                    let Some(shape) = self.shapes.get(&name.replace('.', "_")) else {
                        return Err(format!(
                            "{from}: a drawable background is a <shape> in res/drawable/{name}.xml \
                             (an image is not supported here); or use a color"
                        ));
                    };
                    let mut words = vec![(a::BACKGROUND, shape.color)];
                    if shape.radius > 0 {
                        words.push((a::BACKGROUND_RADIUS, shape.radius));
                    }
                    if shape.stroke_width > 0 {
                        words.push((a::BACKGROUND_STROKE_WIDTH, shape.stroke_width));
                        words.push((a::BACKGROUND_STROKE_COLOR, shape.stroke_color));
                    }
                    return Ok(words);
                }
                one(a::BACKGROUND, v.word(TYPE_COLOR, text, from)?)
            }
            "visibility" => one(a::VISIBILITY, parse_enum(VISIBILITY, text, from)?),
            "enabled" => one(a::ENABLED, v.word(TYPE_BOOL, text, from)?),
            "focusable" => one(a::FOCUSABLE, v.word(TYPE_BOOL, text, from)?),
            "keepScreenOn" => one(a::KEEP_SCREEN_ON, v.word(TYPE_BOOL, text, from)?),
            "minWidth" => one(a::MIN_WIDTH, self.pixels(text, from)?),
            "minHeight" => one(a::MIN_HEIGHT, self.pixels(text, from)?),
            "maxWidth" => one(a::MAX_WIDTH, self.pixels(text, from)?),
            "alpha" => one(a::ALPHA, self.float(text, from)?),
            "text" => one(a::TEXT, self.string_id(text, from)?),
            "textColor" => one(a::TEXT_COLOR, v.word(TYPE_COLOR, text, from)?),
            "textSize" => one(a::TEXT_SIZE, v.word(TYPE_DIMEN, text, from)?),
            "hint" => one(a::HINT, self.string_id(text, from)?),
            "singleLine" => one(a::SINGLE_LINE, v.word(TYPE_BOOL, text, from)?),
            "includeFontPadding" => one(a::INCLUDE_FONT_PADDING, v.word(TYPE_BOOL, text, from)?),
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
            // CircularProgressIndicator, with Material's attribute names.
            "indicatorColor" => one(a::INDICATOR_COLOR, v.word(TYPE_COLOR, text, from)?),
            "trackColor" => one(a::TRACK_COLOR, v.word(TYPE_COLOR, text, from)?),
            "trackThickness" => one(a::TRACK_THICKNESS, self.pixels(text, from)?),
            "indicatorSize" => one(a::INDICATOR_SIZE, self.pixels(text, from)?),
            "startAngle" => one(a::START_ANGLE, self.float(text, from)?),
            "sweepAngle" => one(a::SWEEP_ANGLE, self.float(text, from)?),
            _ => {
                self.warnings
                    .push(format!("{from}: attribute is not supported; ignored"));
                Ok(Vec::new())
            }
        }
    }

    /// An element's attributes in the order they apply: the items of its
    /// `style`, parents first, then its own, each of which replaces a style
    /// item of the same name.
    fn attributes(&self, el: &Element, file: &str) -> Result<Vec<(String, String)>, String> {
        let mut attrs: Vec<(String, String)> = Vec::new();
        if let Some((_, _, style)) = el
            .attrs
            .iter()
            .find(|(prefix, name, _)| prefix.is_none() && name == "style")
        {
            let from = format!("{file}: <{}> style=\"{style}\"", el.name);
            let name = style
                .trim()
                .strip_prefix("@style/")
                .ok_or_else(|| format!("{from}: not a @style reference"))?;
            attrs = self.values.style_items(name, &from)?;
        }
        for (prefix, name, text) in &el.attrs {
            // Design-time attributes never reach the device, as on Android.
            if prefix.as_deref() == Some("tools") || (prefix.is_none() && name == "style") {
                continue;
            }
            attrs.retain(|(n, _)| n != name);
            attrs.push((name.clone(), text.clone()));
        }
        Ok(attrs)
    }

    /// `<include layout="@layout/name"/>`: the named layout's tree in place
    /// of the element, with the include's own id, visibility and `layout_*`
    /// attributes in place of its root's.
    fn include(
        &mut self,
        el: &Element,
        file: &str,
        out: &mut Vec<u32>,
        depth: u32,
    ) -> Result<(), String> {
        let from = format!("{file}: <include>");
        if depth >= MAX_NESTING {
            return Err(format!(
                "{from}: includes nested too deep (a layout that includes itself?)"
            ));
        }
        if !el.children.is_empty() {
            return Err(format!("{from} cannot have children"));
        }
        let target = el
            .attrs
            .iter()
            .find(|(prefix, name, _)| prefix.is_none() && name == "layout")
            .map(|(_, _, v)| v.trim())
            .ok_or_else(|| format!("{from} needs layout=\"@layout/name\""))?;
        let name = match parse_ref(target) {
            Some(("layout", name)) => name,
            _ => return Err(format!("{from}: '{target}' is not a @layout reference")),
        };
        let layouts = self.layouts;
        let (inc_file, root) = layouts
            .get(&name)
            .ok_or_else(|| format!("{from}: @layout/{name} is not defined"))?;
        let mut merged = root.clone();
        for (prefix, attr, text) in &el.attrs {
            if prefix.as_deref() == Some("tools") || (prefix.is_none() && attr == "layout") {
                continue;
            }
            if attr != "id" && attr != "visibility" && !attr.starts_with("layout_") {
                return Err(format!(
                    "{from} {attr}=\"{text}\": an <include> takes android:id, android:visibility \
                     and layout_* attributes only"
                ));
            }
            merged.attrs.retain(|(_, n, _)| n != attr);
            merged
                .attrs
                .push((prefix.clone(), attr.clone(), text.clone()));
        }
        self.node(&merged, inc_file, out, depth + 1)
    }

    fn node(
        &mut self,
        el: &Element,
        file: &str,
        out: &mut Vec<u32>,
        depth: u32,
    ) -> Result<(), String> {
        if el.name == "include" {
            return self.include(el, file, out, depth);
        }
        // Android wants the fully qualified name for a view outside android.widget /
        // android.view (androidx's ViewPager2, say). Every framework view here lives in
        // picodroid.widget, so a layout may spell any of them either way. Any other dotted
        // name is a view class of the app's own: the layout carries the name, and a
        // LayoutInflater.Factory (the Activity) makes the view, since nothing can reflect on it.
        let simple = el
            .name
            .strip_prefix("picodroid.widget.")
            .unwrap_or(&el.name);
        let known = layout::class::ALL
            .iter()
            .find(|(n, _)| *n == simple)
            .map(|&(_, c)| c);
        let custom = known.is_none()
            && el.name.contains('.')
            && !el.name.starts_with("picodroid.widget.")
            && el
                .name
                .split('.')
                .all(|part| r_name(part, "").is_ok_and(|r| r == part));
        let class = match known {
            Some(code) => code,
            None if custom => layout::class::CUSTOM,
            None => {
                let hint = match el.name.as_str() {
                    "merge" => " (<merge> is not supported)",
                    n if n.starts_with("picodroid.widget.") => {
                        " (picodroid.widget has no such view)"
                    }
                    n if n.contains('.') => " (not a class name)",
                    _ => "",
                };
                let names: Vec<&str> = layout::class::ALL.iter().map(|(n, _)| *n).collect();
                return Err(format!(
                    "{file}: <{}> cannot be inflated{hint}. Supported: {}, <include>, or the \
                     fully qualified name of a view class of the app's own",
                    el.name,
                    names.join(", ")
                ));
            }
        };
        let mut words = Vec::new();
        for (name, text) in self.attributes(el, file)? {
            let from = format!("{file}: <{}> {name}=\"{text}\"", el.name);
            words.extend(self.attr(&name, &text, &from)?);
        }
        // Android reads `min`/`max` before `progress` whatever the XML order;
        // the inflater applies words in stream order, so put the range first.
        words.sort_by_key(|(code, _)| !matches!(*code, layout::attr::MIN | layout::attr::MAX));
        if custom {
            let from = format!("{file}: <{}>", el.name);
            let stored = (self.rename_class)(&el.name).unwrap_or_else(|| el.name.clone());
            // First, where the inflater expects it: it needs the name before anything else.
            words.insert(
                0,
                (layout::attr::CLASS_NAME, self.string_id(&stored, &from)?),
            );
        }
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
            // LVGL's flex layout has no per-item cross-axis alignment, so a
            // LinearLayout child's layout_gravity is carried in its
            // LayoutParams but never applied (LinearLayout.setGravity's note).
            if matches!(
                class,
                layout::class::LINEAR_LAYOUT | layout::class::RADIO_GROUP
            ) && child.attr("layout_gravity").is_some()
            {
                self.warnings.push(format!(
                    "{file}: <{}> ignores android:layout_gravity on a child of <{}> (no per-child \
                     cross-axis alignment); wrap the child in a FrameLayout and give that the \
                     gravity, or set the parent's android:gravity",
                    child.name, el.name
                ));
            }
            self.node(child, file, out, depth)?;
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
fn unsupported_dir(path: &Path) -> String {
    format!(
        "{}: not a supported resource directory (values, layout, drawable, or values-<q> / \
         layout-<q> for the configurations sw<N>dp, w<N>dp, h<N>dp, land, port, notouch, finger)",
        path.display()
    )
}

/// Why a `values-…` / `layout-…` directory's qualifiers were refused.
fn qualifier_error(spec: &str, e: QualifierError) -> String {
    match e {
        QualifierError::Unsupported => format!(
            "'{spec}' is not a supported configuration — there are no density, locale or night \
             configurations here; a directory may name sw<N>dp, w<N>dp, h<N>dp, land, port, \
             notouch, finger"
        ),
        QualifierError::Order => format!(
            "'{spec}': qualifiers go in Android's order, each once: sw<N>dp, w<N>dp, h<N>dp, \
             land|port, notouch|finger"
        ),
        QualifierError::BadNumber => {
            format!("'{spec}': a size qualifier is <N>dp with N from 1 to 65535")
        }
    }
}

pub fn compile(res_dir: &Path) -> Result<Compiled, String> {
    compile_with(res_dir, &|_| None)
}

/// [`compile`], with custom view class names in layouts spelled through
/// `rename_class` (the `--shrink-app` map; see `LayoutCompiler::rename_class`).
pub fn compile_with(
    res_dir: &Path,
    rename_class: &dyn Fn(&str) -> Option<String>,
) -> Result<Compiled, String> {
    let mut values_dir = None;
    let mut layout_dir = None;
    let mut drawable_dir = None;
    // Configuration variants (docs/designs/app-portability-2026-10.md D8):
    // `values-<q>` and `layout-<q>`, each one override block. Sorted by name
    // below so the block order, and with it `R`-free ids, is reproducible.
    let mut variant_values: Vec<(Qualifiers, PathBuf)> = Vec::new();
    let mut variant_layouts: Vec<(Qualifiers, PathBuf)> = Vec::new();
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
                if let Some((base, spec)) = name.split_once('-') {
                    let list = match base {
                        "values" => &mut variant_values,
                        "layout" => &mut variant_layouts,
                        "drawable" => {
                            return Err(format!(
                                "{}: drawables cannot vary by configuration; one image serves \
                                 every board",
                                path.display()
                            ))
                        }
                        _ => return Err(unsupported_dir(&path)),
                    };
                    let q = Qualifiers::parse(spec)
                        .map_err(|e| format!("{}: {}", path.display(), qualifier_error(spec, e)))?;
                    if !path.is_dir() {
                        return Err(format!("{}: expected a directory", path.display()));
                    }
                    list.push((q, path.clone()));
                    continue;
                }
                return Err(unsupported_dir(&path));
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
            styles: BTreeMap::new(),
        },
    };

    let mut drawables = Vec::new();
    let mut drawable_names = Vec::new();
    let mut shapes: BTreeMap<String, Shape> = BTreeMap::new();
    if let Some(dir) = &drawable_dir {
        let mut paths = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
            let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') {
                paths.push(path);
            }
        }
        paths.sort();
        for path in paths {
            let ext = path.extension().and_then(|e| e.to_str());
            if !path.is_file() || !matches!(ext, Some("png" | "xml")) {
                return Err(format!(
                    "{}: only *.png images and *.xml shapes belong in {}",
                    path.display(),
                    dir.display()
                ));
            }
            let name = file_res_name(&path)?;
            if drawable_names.contains(&name) || shapes.contains_key(&name) {
                return Err(format!(
                    "{}: @drawable/{name} is defined twice",
                    path.display()
                ));
            }
            if ext == Some("xml") {
                shapes.insert(name, parse_shape(&path, &values)?);
            } else {
                let file = path.file_name().unwrap().to_string_lossy();
                drawables.push((format!("{DRAWABLE_ASSET_PREFIX}{file}"), path.clone()));
                drawable_names.push(name);
            }
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
    // A variant layout redefines a base layout, under the same id; its ids
    // join the app's `R.id`.
    variant_layouts.sort_by(|a, b| a.1.cmp(&b.1));
    let mut variant_layout_files: Vec<(usize, String, String, Element)> = Vec::new();
    for (dir_index, (_, dir)) in variant_layouts.iter().enumerate() {
        for path in sorted_files(dir, "xml")? {
            let name = file_res_name(&path)?;
            if !layouts.iter().any(|(n, _, _)| *n == name) {
                return Err(format!(
                    "{}: @layout/{name} exists only in {}; a variant overrides a layout that \
                     res/layout/ defines",
                    path.display(),
                    dir.display()
                ));
            }
            let root = parse_xml(&path)?;
            let file = path.display().to_string();
            collect_ids(&root, &file, &mut id_names)?;
            variant_layout_files.push((dir_index, name, file, root));
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
    let mut variant_streams = Vec::new();
    {
        let by_name: BTreeMap<String, (String, Element)> = layouts
            .iter()
            .map(|(name, file, root)| (name.clone(), (file.clone(), root.clone())))
            .collect();
        let mut lc = LayoutCompiler {
            values: &values,
            string_index: &string_index,
            pooled: &mut pooled,
            named_strings: string_index.len() as u16,
            ids: &ids,
            drawable_index: &drawable_index,
            shapes: &shapes,
            layouts: &by_name,
            warnings: &mut warnings,
            rename_class,
        };
        for (_, file, root) in &layouts {
            let mut words = Vec::new();
            lc.node(root, file, &mut words, 0)?;
            streams.push(words);
        }
        // A variant layout's references resolve against the base values and
        // base layouts (`<include>`), like any layout's.
        for (_, _, file, root) in &variant_layout_files {
            let mut words = Vec::new();
            lc.node(root, file, &mut words, 0)?;
            variant_streams.push(words);
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
    // Styles, by R name. The table keeps only what the framework reads of
    // one: the theme colours its own widgets default to.
    let mut style_names: Vec<(String, &String)> = values
        .styles
        .keys()
        .map(|name| (name.replace('.', "_"), name))
        .collect();
    style_names.sort();
    for (r, name) in style_names {
        let from = &values.styles[name].at;
        let mut words = Vec::new();
        for (item, _, code) in theme::attr::ALL {
            if let Some(text) = values.style_item(name, item) {
                words.push(*code);
                words.push(values.word(
                    TYPE_COLOR,
                    text,
                    &format!("{from} <item name=\"{item}\">"),
                )?);
            }
        }
        let id = table.push_style(words).map_err(overflow)?;
        symbols.push((TYPE_STYLE, r, id));
    }

    // Configuration variants, one override block per directory: the ids a
    // `values-<q>` directory redefines, then the layouts a `layout-<q>` one
    // does. A variant may only redefine what the base defines; `R` is the
    // base's. Values resolve against the base with the variant laid over it.
    let id_of: BTreeMap<(u8, &str), u32> = symbols
        .iter()
        .map(|(ty, name, id)| ((*ty, name.as_str()), *id))
        .collect();
    variant_values.sort_by(|a, b| a.1.cmp(&b.1));
    for (q, dir) in &variant_values {
        let vv = Values::load(dir)?;
        if let Some((name, style)) = vv.styles.iter().next() {
            return Err(format!(
                "{}: <style name=\"{name}\"> — styles cannot vary by configuration",
                style.at
            ));
        }
        let merged = values.overlay(&vv);
        let mut pairs = Vec::new();
        for ty in [TYPE_STRING, TYPE_COLOR, TYPE_DIMEN, TYPE_INTEGER, TYPE_BOOL] {
            for name in vv.names(ty) {
                let (text, from) = &vv.raw[&ty][name];
                let id = *id_of.get(&(ty, name.as_str())).ok_or_else(|| {
                    format!(
                        "{from}: @{}/{name} exists only in {}; a variant overrides a value that \
                         res/values/ defines",
                        fmt::type_name(ty).unwrap_or("?"),
                        dir.display()
                    )
                })?;
                let value = if ty == TYPE_STRING {
                    let text = unescape_string(merged.resolve_text(TYPE_STRING, name, from)?)
                        .map_err(|e| format!("{from}: {e}"))?;
                    OverrideValue::Bytes(text.into_bytes())
                } else {
                    OverrideValue::Value(merged.word(ty, text, from)?)
                };
                pairs.push((id, value));
            }
        }
        table.push_overrides(*q, pairs).map_err(overflow)?;
    }
    let mut variant_streams = variant_streams.into_iter();
    let mut layout_pairs: Vec<Vec<(u32, OverrideValue)>> =
        (0..variant_layouts.len()).map(|_| Vec::new()).collect();
    for (dir_index, name, _, _) in &variant_layout_files {
        let words = variant_streams
            .next()
            .expect("one stream per variant layout");
        layout_pairs[*dir_index].push((
            id_of[&(TYPE_LAYOUT, name.as_str())],
            OverrideValue::Words(words),
        ));
    }
    for ((q, _), pairs) in variant_layouts.iter().zip(layout_pairs) {
        if !pairs.is_empty() {
            table.push_overrides(*q, pairs).map_err(overflow)?;
        }
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
        android:textSize="18sp"
        android:elevation="4dp" />
    <Button
        android:id="@+id/ok"
        android:layout_width="0dp"
        android:layout_height="40dp"
        android:layout_weight="1"
        android:gravity="center_vertical|right"
        android:text="OK"
        android:textSize="@dimen/gap" />
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
            node_header(k::TEXT_VIEW, 6, 0),
            a::ID, title,
            a::LAYOUT_WIDTH, -2i32 as u32,
            a::LAYOUT_HEIGHT, -2i32 as u32,
            a::TEXT, app_name,
            a::TEXT_COLOR, 0xFF33_66CC,
            a::TEXT_SIZE, 18.0f32.to_bits(),
            node_header(k::BUTTON, 7, 0),
            a::ID, ok,
            a::LAYOUT_WIDTH, 0,
            a::LAYOUT_HEIGHT, 40,
            a::LAYOUT_WEIGHT, 1.0f32.to_bits(),
            a::GRAVITY, 0x15,
            a::TEXT, ok_text,
            a::TEXT_SIZE, 12.0f32.to_bits(),
            node_header(k::BUTTON, 1, 0),
            a::TEXT, ok_text,
        ];
        assert_eq!(words, expected);

        // An attribute with no setter is reported, tools:context is not.
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.warnings[0].contains("elevation"));
    }

    #[test]
    fn configuration_variants_become_override_blocks() {
        let dir = tree(&[
            ("values/values.xml", VALUES),
            (
                "values-w320dp/strings.xml",
                r#"<resources><string name="app_name">Wide</string></resources>"#,
            ),
            ("layout/main.xml", LAYOUT),
            ("layout-land/main.xml", LAYOUT),
        ]);
        let c = compile(&dir).unwrap();
        let t = ResTable::parse(&c.table).unwrap();
        let blocks: Vec<fmt::Overrides> = t.overrides().collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].qualifiers.w_dp, 320);
        assert_eq!(
            blocks[1].qualifiers.orientation,
            fmt::config::ORIENTATION_LAND
        );
        let app_name = id_of(&c, TYPE_STRING, "app_name");
        let main = id_of(&c, TYPE_LAYOUT, "main");
        // The base reads as before; `R` gained nothing.
        assert_eq!(t.string(app_name), Some(&b"Res Demo"[..]));
        assert!(!c
            .symbols
            .iter()
            .any(|(ty, _, _)| *ty == fmt::TYPE_OVERRIDES));
        // A wide landscape window takes both.
        let sel = [0u8, 1];
        let r = t.with(&sel);
        assert_eq!(r.string(app_name), Some(&b"Wide"[..]));
        assert!(blocks[1].value(main).is_some());
        assert_eq!(r.layout(main).unwrap().len(), t.layout(main).unwrap().len());

        let err = |files: &[(&str, &str)]| compile(&tree(files)).unwrap_err();
        let wrap = |body: &str| format!("<resources>{body}</resources>");
        assert!(err(&[
            ("values/a.xml", &wrap(r#"<string name="a">x</string>"#)),
            ("values-land/a.xml", &wrap(r#"<string name="b">y</string>"#)),
        ])
        .contains("exists only in"));
        assert!(err(&[("values-land-sw320dp/a.xml", "<resources/>")]).contains("order"));
        assert!(err(&[("values-w320/a.xml", "<resources/>")]).contains("<N>dp"));
        assert!(err(&[("drawable-land/x.xml", "<shape/>")]).contains("cannot vary"));
        assert!(err(&[
            ("layout/m.xml", "<FrameLayout/>"),
            ("layout-port/other.xml", "<FrameLayout/>"),
        ])
        .contains("exists only in"));
    }

    #[test]
    fn an_element_may_be_fully_qualified() {
        let short = tree(&[(
            "layout/m.xml",
            r#"<FrameLayout><ViewPager2 layout_width="match_parent"/></FrameLayout>"#,
        )]);
        let long = tree(&[(
            "layout/m.xml",
            r#"<picodroid.widget.FrameLayout><picodroid.widget.ViewPager2 layout_width="match_parent"/></picodroid.widget.FrameLayout>"#,
        )]);
        assert_eq!(
            compile(&short).unwrap().table,
            compile(&long).unwrap().table
        );
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
        assert!(err(&[("values/a.xml", &wrap(r#"<plurals name="p"/>"#))]).contains("not supported"));
        assert!(err(&[("layout/Main.xml", "<TextView/>")]).contains("[a-z0-9_]"));
        assert!(err(&[("layout/m.xml", "<Dial/>")]).contains("cannot be inflated"));
        assert!(err(&[("layout/m.xml", "<picodroid.widget.Dial/>")]).contains("cannot be inflated"));
        assert!(err(&[(
            "layout/m.xml",
            "<com.example.Dial><TextView/></com.example.Dial>"
        )])
        .contains("not a ViewGroup"));
        assert!(err(&[(
            "layout/m.xml",
            r#"<TextView textColor="?attr/colorPrimary"/>"#
        )])
        .contains("AppTheme"));
        assert!(
            err(&[("layout/m.xml", r#"<TextView style="@style/Nope"/>"#)])
                .contains("@style/Nope is not defined")
        );
        assert!(err(&[("layout/m.xml", r#"<include layout="@layout/m"/>"#)]).contains("too deep"));
        assert!(
            err(&[("layout/m.xml", r#"<include layout="@layout/nope"/>"#)])
                .contains("@layout/nope is not defined")
        );
        assert!(err(&[
            (
                "layout/m.xml",
                r#"<FrameLayout background="@drawable/pic"/>"#
            ),
            ("drawable/pic.xml", r#"<vector/>"#)
        ])
        .contains("is a <shape>"));
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

    /// Collects a layout's words.
    fn words_of(c: &Compiled, name: &str) -> Vec<u32> {
        let t = ResTable::parse(&c.table).unwrap();
        let l = t.layout(id_of(c, TYPE_LAYOUT, name)).unwrap();
        (0..l.len()).map(|i| l.word(i).unwrap()).collect()
    }

    #[test]
    fn margins_compile_to_the_four_sides() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[
            ("values/values.xml", VALUES),
            (
                "layout/m.xml",
                r#"<FrameLayout layout_margin="@dimen/gap">
                     <TextView layout_marginLeft="3dp" layout_marginTop="4dp"
                               layout_marginEnd="5dp" layout_marginBottom="6dp"
                               includeFontPadding="false"/>
                     <View layout_marginHorizontal="7dp" layout_marginVertical="8dp"/>
                   </FrameLayout>"#,
            ),
        ]);
        let c = compile(&dir).unwrap();
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::FRAME_LAYOUT, 4, 2),
            a::LAYOUT_MARGIN_LEFT, 12, a::LAYOUT_MARGIN_TOP, 12,
            a::LAYOUT_MARGIN_RIGHT, 12, a::LAYOUT_MARGIN_BOTTOM, 12,
            node_header(k::TEXT_VIEW, 5, 0),
            a::LAYOUT_MARGIN_LEFT, 3, a::LAYOUT_MARGIN_TOP, 4,
            a::LAYOUT_MARGIN_RIGHT, 5, a::LAYOUT_MARGIN_BOTTOM, 6,
            a::INCLUDE_FONT_PADDING, 0,
            node_header(k::VIEW, 4, 0),
            a::LAYOUT_MARGIN_LEFT, 7, a::LAYOUT_MARGIN_RIGHT, 7,
            a::LAYOUT_MARGIN_TOP, 8, a::LAYOUT_MARGIN_BOTTOM, 8,
        ];
        assert_eq!(words_of(&c, "m"), expected);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    }

    #[test]
    fn a_shape_drawable_is_flattened_into_the_background() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[
            ("values/values.xml", VALUES),
            (
                "drawable/card.xml",
                r##"<shape shape="rectangle">
                     <solid color="@color/accent"/>
                     <corners radius="@dimen/gap"/>
                     <stroke width="2dp" color="#102030"/>
                   </shape>"##,
            ),
            (
                "drawable/flat.xml",
                r##"<shape><solid color="#000"/></shape>"##,
            ),
            (
                "layout/m.xml",
                r#"<FrameLayout background="@drawable/card">
                     <View background="@drawable/flat"/>
                   </FrameLayout>"#,
            ),
        ]);
        let c = compile(&dir).unwrap();
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::FRAME_LAYOUT, 4, 1),
            a::BACKGROUND, 0xFF33_66CC,
            a::BACKGROUND_RADIUS, 12,
            a::BACKGROUND_STROKE_WIDTH, 2,
            a::BACKGROUND_STROKE_COLOR, 0xFF10_2030,
            node_header(k::VIEW, 1, 0),
            a::BACKGROUND, 0xFF00_0000,
        ];
        assert_eq!(words_of(&c, "m"), expected);
        // A shape is a build-time thing: no R.drawable entry, nothing in ASSETS.
        assert!(c.drawables.is_empty());
        assert!(c.symbols.iter().all(|(ty, _, _)| *ty != TYPE_DRAWABLE));
    }

    #[test]
    fn an_include_is_the_other_layout_in_place() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[
            (
                "layout/row.xml",
                r#"<LinearLayout id="@+id/row" layout_width="match_parent" layout_height="20dp">
                     <TextView id="@+id/name"/>
                   </LinearLayout>"#,
            ),
            (
                "layout/m.xml",
                r#"<FrameLayout>
                     <include layout="@layout/row" id="@+id/first" layout_marginTop="5dp"/>
                     <include layout="@layout/row"/>
                   </FrameLayout>"#,
            ),
        ]);
        let c = compile(&dir).unwrap();
        let id = |n: &str| id_of(&c, TYPE_ID, n);
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::FRAME_LAYOUT, 0, 2),
            // The include's id replaces the root's; its margin is added.
            node_header(k::LINEAR_LAYOUT, 4, 1),
            a::LAYOUT_WIDTH, -1i32 as u32,
            a::LAYOUT_HEIGHT, 20,
            a::ID, id("first"),
            a::LAYOUT_MARGIN_TOP, 5,
            node_header(k::TEXT_VIEW, 1, 0),
            a::ID, id("name"),
            node_header(k::LINEAR_LAYOUT, 3, 1),
            a::ID, id("row"),
            a::LAYOUT_WIDTH, -1i32 as u32,
            a::LAYOUT_HEIGHT, 20,
            node_header(k::TEXT_VIEW, 1, 0),
            a::ID, id("name"),
        ];
        assert_eq!(words_of(&c, "m"), expected);
    }

    #[test]
    fn a_class_of_the_apps_own_carries_its_name() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[(
            "layout/m.xml",
            r#"<FrameLayout><com.example.ui.Dial id="@+id/dial" max="9" min="1"/></FrameLayout>"#,
        )]);
        let c = compile(&dir).unwrap();
        let t = ResTable::parse(&c.table).unwrap();
        let name = fmt::res_id(TYPE_STRING, 0);
        assert_eq!(t.string(name), Some(&b"com.example.ui.Dial"[..]));
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::FRAME_LAYOUT, 0, 1),
            node_header(k::CUSTOM, 4, 0),
            // The name first, then the range before the rest, as for any view.
            a::CLASS_NAME, name,
            a::MAX, 9,
            a::MIN, 1,
            a::ID, id_of(&c, TYPE_ID, "dial"),
        ];
        assert_eq!(words_of(&c, "m"), expected);
    }

    const STYLES: &str = r##"<resources>
    <color name="accent">#36C</color>
    <color name="ink">#111</color>
    <style name="AppTheme" parent="android:Theme.Material">
        <item name="colorPrimary">@color/accent</item>
        <item name="android:textColorPrimary">@color/ink</item>
        <item name="android:colorBackground">#000</item>
        <item name="gap">6dp</item>
    </style>
    <style name="Label">
        <item name="android:singleLine">true</item>
        <item name="android:textColor">?android:attr/textColorPrimary</item>
    </style>
    <style name="Label.Loud">
        <item name="android:textColor">?attr/colorPrimary</item>
        <item name="android:textSize">20sp</item>
    </style>
</resources>"##;

    #[test]
    fn a_style_is_expanded_and_the_theme_resolved_at_build_time() {
        use layout::{attr as a, class as k, node_header};
        let dir = tree(&[
            ("values/styles.xml", STYLES),
            (
                "layout/m.xml",
                r#"<LinearLayout padding="?attr/gap">
                     <TextView style="@style/Label"/>
                     <TextView style="@style/Label.Loud" textSize="14sp"/>
                   </LinearLayout>"#,
            ),
        ]);
        let c = compile(&dir).unwrap();
        #[rustfmt::skip]
        let expected = vec![
            node_header(k::LINEAR_LAYOUT, 4, 2),
            a::PADDING_LEFT, 6, a::PADDING_TOP, 6, a::PADDING_RIGHT, 6, a::PADDING_BOTTOM, 6,
            node_header(k::TEXT_VIEW, 2, 0),
            a::SINGLE_LINE, 1,
            a::TEXT_COLOR, 0xFF11_1111,
            // Label.Loud inherits Label by its name; its own colour wins, and the
            // element's own text size replaces the style's.
            node_header(k::TEXT_VIEW, 3, 0),
            a::SINGLE_LINE, 1,
            a::TEXT_COLOR, 0xFF33_66CC,
            a::TEXT_SIZE, 14.0f32.to_bits(),
        ];
        assert_eq!(words_of(&c, "m"), expected);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);

        // R.style, dots as underscores; the table keeps the framework's colours.
        assert_eq!(id_of(&c, TYPE_STYLE, "AppTheme"), 0x7f09_0000);
        assert_eq!(id_of(&c, TYPE_STYLE, "Label"), 0x7f09_0001);
        assert_eq!(id_of(&c, TYPE_STYLE, "Label_Loud"), 0x7f09_0002);
        let t = ResTable::parse(&c.table).unwrap();
        let theme_words = t.style(0x7f09_0000).unwrap();
        let got: Vec<u32> = (0..theme_words.len())
            .map(|i| theme_words.word(i).unwrap())
            .collect();
        #[rustfmt::skip]
        let want = vec![
            theme::attr::COLOR_PRIMARY, 0xFF33_66CC,
            theme::attr::COLOR_BACKGROUND, 0xFF00_0000,
            theme::attr::TEXT_COLOR_PRIMARY, 0xFF11_1111,
        ];
        assert_eq!(got, want);
        assert_eq!(t.style(0x7f09_0001).unwrap().len(), 0);
        assert!(r_java("p", &c.symbols).contains("public static final class style {"));
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
        // CUSTOM names no element of its own, so it is not in ALL.
        assert_eq!(
            inflater.get("CLASS_CUSTOM"),
            Some(&(layout::class::CUSTOM as i64)),
            "LayoutInflater CLASS_CUSTOM"
        );
        let class_consts = inflater.keys().filter(|k| k.starts_with("CLASS_")).count();
        assert_eq!(
            class_consts,
            layout::class::ALL.len() + 1,
            "stale CLASS_ label"
        );

        // Resources.applyTheme spells the theme attribute codes the same way.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk/java/picodroid/content/res/Resources.java");
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut themed = BTreeMap::new();
        for line in text.lines() {
            if let Some((case, name)) = line.trim().split_once(": // THEME_") {
                let code = case
                    .strip_prefix("case ")
                    .and_then(|n| n.parse::<u32>().ok());
                themed.insert(name.trim().to_string(), code.expect("a numeric case label"));
            }
        }
        for (_, name, code) in theme::attr::ALL {
            assert_eq!(themed.get(*name), Some(code), "Resources THEME_{name}");
        }
        assert_eq!(themed.len(), theme::attr::ALL.len(), "stale THEME_ label");

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
