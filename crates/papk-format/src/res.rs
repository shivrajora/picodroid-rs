// SPDX-License-Identifier: GPL-3.0-only
//! The RESOURCES section (`"RESR"`, PAPK v1.2+): an app's compiled `res/`
//! tree — `res/values/*.xml`, `res/layout/*.xml` and the names of
//! `res/drawable/*.png` — addressed by the integer ids of the app's
//! generated `R` class.
//!
//! Nothing here is parsed on the device beyond offset arithmetic: XML is
//! compiled on the host (`tools/papk-pack`, module `res`), references
//! (`@color/accent`, `12dp`) are resolved at build time, and what ships is a
//! table of 32-bit values plus a few length-prefixed blobs read in place out
//! of XIP flash. This is the picodroid analogue of Android's
//! `resources.arsc` + binary XML, minus configurations — there is one
//! display, one density and one locale.
//!
//! # Resource ids
//!
//! `0x7fTTEEEE`, as on Android: package byte `0x7f`, one byte of type, a
//! 16-bit entry index. [`res_id`] composes one, [`res_type`] / [`res_entry`]
//! take it apart. Ids are assigned by the compiler in sorted-name order per
//! type, so the `R.java` generator and the packer agree by construction.
//!
//! # Section data
//!
//! All integers little-endian; every offset is relative to the start of the
//! section data and 4-byte aligned where it names `u32`s.
//!
//! ```text
//!   [u8 table_version = 1][u8 type_count][u16 reserved0]
//!   type directory — type_count entries, ascending type:
//!     [u8 type][u8 reserved0][u16 entry_count][u32 values_offset]
//!   per type, at values_offset: entry_count × [u32 value]
//!     string   (1): offset of [u16 len][UTF-8 bytes]
//!     color    (2): 0xAARRGGBB
//!     dimen    (3): f32 bits, pixels (dp = sp = px: one density)
//!     integer  (4): i32
//!     bool     (5): 0 / 1
//!     layout   (6): offset (4-aligned) of [u32 word_count][u32 words…]
//!     drawable (7): offset of [u16 len][ASSETS entry name]
//!   blobs, in any order
//! ```
//!
//! `id` resources (`@+id/title`, [`TYPE_ID`]) exist only as `R.id`
//! constants; they have no table.
//!
//! The string table may hold more entries than `R.string` names: a literal
//! `android:text="Hello"` in a layout is pooled as an anonymous string entry
//! after the named ones, so a layout word stream carries nothing but ints.
//!
//! # Layout word stream
//!
//! One node, pre-order, children following their parent:
//!
//! ```text
//!   [u32 header]  = class (bits 0..8) | attr_count (8..16) | child_count (16..32)
//!   attr_count × ([u32 attr][u32 value])
//!   child_count × node
//! ```
//!
//! The `class` codes are [`layout::class`], the `attr` codes
//! [`layout::attr`]. `sdk/java/picodroid/view/LayoutInflater.java` carries
//! the same numbers as `static final int`s; a papk-pack test holds the two
//! together.

use crate::PapkError;

/// `table_version` this reader understands and the writer emits.
pub const TABLE_VERSION: u8 = 1;

/// The package byte of every app resource id.
pub const PACKAGE_ID: u32 = 0x7f;

pub const TYPE_STRING: u8 = 1;
pub const TYPE_COLOR: u8 = 2;
pub const TYPE_DIMEN: u8 = 3;
pub const TYPE_INTEGER: u8 = 4;
pub const TYPE_BOOL: u8 = 5;
pub const TYPE_LAYOUT: u8 = 6;
pub const TYPE_DRAWABLE: u8 = 7;
/// `R.id` — constants only, never present in the table.
pub const TYPE_ID: u8 = 8;

/// The `R` inner-class name of a type, or `None` for an unknown one.
pub const fn type_name(ty: u8) -> Option<&'static str> {
    Some(match ty {
        TYPE_STRING => "string",
        TYPE_COLOR => "color",
        TYPE_DIMEN => "dimen",
        TYPE_INTEGER => "integer",
        TYPE_BOOL => "bool",
        TYPE_LAYOUT => "layout",
        TYPE_DRAWABLE => "drawable",
        TYPE_ID => "id",
        _ => return None,
    })
}

/// Compose a resource id from a type and an entry index.
pub const fn res_id(ty: u8, entry: u16) -> u32 {
    (PACKAGE_ID << 24) | ((ty as u32) << 16) | entry as u32
}

/// The type byte of `id`, or `None` when `id` is not an app resource id.
pub const fn res_type(id: u32) -> Option<u8> {
    if id >> 24 == PACKAGE_ID {
        Some((id >> 16) as u8)
    } else {
        None
    }
}

/// The entry index of `id`.
pub const fn res_entry(id: u32) -> u16 {
    id as u16
}

/// Codes of the layout word stream. Append only: a code, once shipped in a
/// PAPK, keeps its meaning.
pub mod layout {
    /// View classes an XML element may name.
    pub mod class {
        pub const LINEAR_LAYOUT: u8 = 1;
        pub const FRAME_LAYOUT: u8 = 2;
        pub const SCROLL_VIEW: u8 = 3;
        pub const TEXT_VIEW: u8 = 4;
        pub const BUTTON: u8 = 5;
        pub const IMAGE_VIEW: u8 = 6;
        pub const EDIT_TEXT: u8 = 7;
        pub const CHECK_BOX: u8 = 8;
        pub const SWITCH: u8 = 9;
        pub const PROGRESS_BAR: u8 = 10;
        pub const SEEK_BAR: u8 = 11;
        pub const RADIO_GROUP: u8 = 12;
        pub const RADIO_BUTTON: u8 = 13;
        pub const TOGGLE_BUTTON: u8 = 14;
        pub const SPINNER: u8 = 15;
        pub const LIST_VIEW: u8 = 16;

        /// `(XML element name, code)`.
        pub const ALL: &[(&str, u8)] = &[
            ("LinearLayout", LINEAR_LAYOUT),
            ("FrameLayout", FRAME_LAYOUT),
            ("ScrollView", SCROLL_VIEW),
            ("TextView", TEXT_VIEW),
            ("Button", BUTTON),
            ("ImageView", IMAGE_VIEW),
            ("EditText", EDIT_TEXT),
            ("CheckBox", CHECK_BOX),
            ("Switch", SWITCH),
            ("ProgressBar", PROGRESS_BAR),
            ("SeekBar", SEEK_BAR),
            ("RadioGroup", RADIO_GROUP),
            ("RadioButton", RADIO_BUTTON),
            ("ToggleButton", TOGGLE_BUTTON),
            ("Spinner", SPINNER),
            ("ListView", LIST_VIEW),
        ];
    }

    /// Attributes. The comment on each is the encoding of its value word.
    pub mod attr {
        /// `R.id` value.
        pub const ID: u32 = 1;
        /// Pixels, or -1 `match_parent`, -2 `wrap_content`.
        pub const LAYOUT_WIDTH: u32 = 2;
        pub const LAYOUT_HEIGHT: u32 = 3;
        /// f32 bits.
        pub const LAYOUT_WEIGHT: u32 = 4;
        /// `Gravity` bits.
        pub const LAYOUT_GRAVITY: u32 = 5;
        /// Pixels. `padding` is expanded to the four sides by the compiler.
        pub const PADDING_LEFT: u32 = 6;
        pub const PADDING_TOP: u32 = 7;
        pub const PADDING_RIGHT: u32 = 8;
        pub const PADDING_BOTTOM: u32 = 9;
        /// ARGB. Colour backgrounds only.
        pub const BACKGROUND: u32 = 10;
        /// `View.VISIBLE` / `INVISIBLE` / `GONE`.
        pub const VISIBILITY: u32 = 11;
        /// 0 / 1.
        pub const ENABLED: u32 = 12;
        pub const FOCUSABLE: u32 = 13;
        /// f32 bits.
        pub const ALPHA: u32 = 14;
        /// String resource id (named or pooled).
        pub const TEXT: u32 = 15;
        /// ARGB.
        pub const TEXT_COLOR: u32 = 16;
        /// String resource id.
        pub const HINT: u32 = 17;
        /// 0 / 1.
        pub const SINGLE_LINE: u32 = 18;
        pub const MAX_LINES: u32 = 19;
        /// 0 none, 1 start, 2 middle, 3 end, 4 marquee.
        pub const ELLIPSIZE: u32 = 20;
        /// `LinearLayout.HORIZONTAL` (0) / `VERTICAL` (1).
        pub const ORIENTATION: u32 = 21;
        /// `Gravity` bits.
        pub const GRAVITY: u32 = 22;
        /// Drawable resource id.
        pub const SRC: u32 = 23;
        /// `ImageView.SCALE_*`.
        pub const SCALE_TYPE: u32 = 24;
        /// ARGB.
        pub const TINT: u32 = 25;
        /// 0 / 1.
        pub const CHECKED: u32 = 26;
        pub const PROGRESS: u32 = 27;
        pub const MAX: u32 = 28;
        /// `InputType` bits.
        pub const INPUT_TYPE: u32 = 29;
        /// String resource ids.
        pub const TEXT_ON: u32 = 30;
        pub const TEXT_OFF: u32 = 31;

        /// `(constant name in LayoutInflater.java, code)`.
        pub const ALL: &[(&str, u32)] = &[
            ("ID", ID),
            ("LAYOUT_WIDTH", LAYOUT_WIDTH),
            ("LAYOUT_HEIGHT", LAYOUT_HEIGHT),
            ("LAYOUT_WEIGHT", LAYOUT_WEIGHT),
            ("LAYOUT_GRAVITY", LAYOUT_GRAVITY),
            ("PADDING_LEFT", PADDING_LEFT),
            ("PADDING_TOP", PADDING_TOP),
            ("PADDING_RIGHT", PADDING_RIGHT),
            ("PADDING_BOTTOM", PADDING_BOTTOM),
            ("BACKGROUND", BACKGROUND),
            ("VISIBILITY", VISIBILITY),
            ("ENABLED", ENABLED),
            ("FOCUSABLE", FOCUSABLE),
            ("ALPHA", ALPHA),
            ("TEXT", TEXT),
            ("TEXT_COLOR", TEXT_COLOR),
            ("HINT", HINT),
            ("SINGLE_LINE", SINGLE_LINE),
            ("MAX_LINES", MAX_LINES),
            ("ELLIPSIZE", ELLIPSIZE),
            ("ORIENTATION", ORIENTATION),
            ("GRAVITY", GRAVITY),
            ("SRC", SRC),
            ("SCALE_TYPE", SCALE_TYPE),
            ("TINT", TINT),
            ("CHECKED", CHECKED),
            ("PROGRESS", PROGRESS),
            ("MAX", MAX),
            ("INPUT_TYPE", INPUT_TYPE),
            ("TEXT_ON", TEXT_ON),
            ("TEXT_OFF", TEXT_OFF),
        ];
    }

    /// Pack a node header word.
    pub const fn node_header(class: u8, attr_count: u8, child_count: u16) -> u32 {
        class as u32 | (attr_count as u32) << 8 | (child_count as u32) << 16
    }
}

const TABLE_HEADER_LEN: usize = 4;
const DIR_ENTRY_LEN: usize = 8;

/// A compiled layout, read in place.
#[derive(Debug, Clone, Copy)]
pub struct Layout<'a> {
    /// `word_count × 4` bytes.
    words: &'a [u8],
}

impl Layout<'_> {
    /// Number of 32-bit words in the stream.
    pub fn len(&self) -> usize {
        self.words.len() / 4
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Word `index`, or `None` past the end.
    pub fn word(&self, index: usize) -> Option<u32> {
        let at = index.checked_mul(4)?;
        let b = self.words.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Zero-copy reader over RESOURCES section data.
#[derive(Debug, Clone, Copy)]
pub struct ResTable<'a> {
    data: &'a [u8],
    type_count: usize,
}

impl<'a> ResTable<'a> {
    /// Validate the table header and the type directory. Blobs are
    /// bounds-checked as they are read.
    pub fn parse(data: &'a [u8]) -> Result<Self, PapkError> {
        if data.len() < TABLE_HEADER_LEN {
            return Err(PapkError::Truncated);
        }
        if data[0] != TABLE_VERSION {
            return Err(PapkError::UnsupportedVersion);
        }
        let type_count = data[1] as usize;
        let dir_end = TABLE_HEADER_LEN + type_count * DIR_ENTRY_LEN;
        if dir_end > data.len() {
            return Err(PapkError::Truncated);
        }
        let table = Self { data, type_count };
        for i in 0..type_count {
            let (_, count, offset) = table.dir_entry(i);
            let end = (count as usize)
                .checked_mul(4)
                .and_then(|n| n.checked_add(offset))
                .ok_or(PapkError::Truncated)?;
            if end > data.len() {
                return Err(PapkError::Truncated);
            }
        }
        Ok(table)
    }

    fn dir_entry(&self, i: usize) -> (u8, u16, usize) {
        let at = TABLE_HEADER_LEN + i * DIR_ENTRY_LEN;
        let d = self.data;
        (
            d[at],
            u16::from_le_bytes([d[at + 2], d[at + 3]]),
            u32::from_le_bytes([d[at + 4], d[at + 5], d[at + 6], d[at + 7]]) as usize,
        )
    }

    /// `(type, entry_count)` for every type in the table.
    pub fn types(&self) -> impl Iterator<Item = (u8, u16)> + '_ {
        (0..self.type_count).map(|i| {
            let (ty, count, _) = self.dir_entry(i);
            (ty, count)
        })
    }

    /// Entry count of `ty`; 0 when the table has none.
    pub fn entry_count(&self, ty: u8) -> u16 {
        self.types().find(|&(t, _)| t == ty).map_or(0, |(_, n)| n)
    }

    /// The raw value word of `id`, whatever its type.
    pub fn value(&self, id: u32) -> Option<u32> {
        let ty = res_type(id)?;
        let entry = res_entry(id) as usize;
        let (_, count, offset) = (0..self.type_count)
            .map(|i| self.dir_entry(i))
            .find(|&(t, _, _)| t == ty)?;
        if entry >= count as usize {
            return None;
        }
        // `parse` proved the whole value array in bounds.
        let at = offset + entry * 4;
        let d = self.data;
        Some(u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]))
    }

    /// The value of `id`, only when `id` is of type `ty`.
    pub fn value_of(&self, ty: u8, id: u32) -> Option<u32> {
        if res_type(id)? != ty {
            return None;
        }
        self.value(id)
    }

    /// `[u16 len][bytes]` at `offset`.
    fn blob_u16(&self, offset: usize) -> Option<&'a [u8]> {
        let len_end = offset.checked_add(2)?;
        let l = self.data.get(offset..len_end)?;
        let len = u16::from_le_bytes([l[0], l[1]]) as usize;
        self.data.get(len_end..len_end.checked_add(len)?)
    }

    /// The UTF-8 bytes of a string resource.
    pub fn string(&self, id: u32) -> Option<&'a [u8]> {
        self.blob_u16(self.value_of(TYPE_STRING, id)? as usize)
    }

    /// The ASSETS entry name behind a drawable resource.
    pub fn drawable_name(&self, id: u32) -> Option<&'a [u8]> {
        self.blob_u16(self.value_of(TYPE_DRAWABLE, id)? as usize)
    }

    /// The word stream of a layout resource.
    pub fn layout(&self, id: u32) -> Option<Layout<'a>> {
        let offset = self.value_of(TYPE_LAYOUT, id)? as usize;
        let count_end = offset.checked_add(4)?;
        let c = self.data.get(offset..count_end)?;
        let count = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
        let words = self
            .data
            .get(count_end..count_end.checked_add(count.checked_mul(4)?)?)?;
        Some(Layout { words })
    }
}

// ── Writer ────────────────────────────────────────────────────────────────────

#[cfg(feature = "write")]
pub use write::ResTableBuilder;

#[cfg(feature = "write")]
mod write {
    use super::*;
    use crate::BuildError;
    use alloc::vec::Vec;

    enum Entry {
        Value(u32),
        /// `[u16 len][bytes]` blob.
        Bytes(Vec<u8>),
        /// `[u32 count][words]` blob.
        Words(Vec<u32>),
    }

    /// Builds RESOURCES section data. Entries of a type get consecutive
    /// indices in insertion order; each `push_*` returns the new id.
    #[derive(Default)]
    pub struct ResTableBuilder {
        /// Indexed by type; `types[0]` is unused.
        types: [Vec<Entry>; 8],
    }

    impl ResTableBuilder {
        pub fn new() -> Self {
            Self::default()
        }

        /// True when nothing was added — the caller then omits the section.
        pub fn is_empty(&self) -> bool {
            self.types.iter().all(Vec::is_empty)
        }

        fn push(&mut self, ty: u8, entry: Entry) -> Result<u32, BuildError> {
            let list = &mut self.types[ty as usize];
            let index = u16::try_from(list.len()).map_err(|_| BuildError::TooManyEntries)?;
            list.push(entry);
            Ok(res_id(ty, index))
        }

        /// A color, dimen (f32 bits), integer or bool.
        pub fn push_value(&mut self, ty: u8, value: u32) -> Result<u32, BuildError> {
            debug_assert!(matches!(
                ty,
                TYPE_COLOR | TYPE_DIMEN | TYPE_INTEGER | TYPE_BOOL
            ));
            self.push(ty, Entry::Value(value))
        }

        pub fn push_string(&mut self, text: &str) -> Result<u32, BuildError> {
            if text.len() > u16::MAX as usize {
                return Err(BuildError::ValueTooLong);
            }
            self.push(TYPE_STRING, Entry::Bytes(text.as_bytes().to_vec()))
        }

        /// `asset_name` is the ASSETS entry holding the pixels.
        pub fn push_drawable(&mut self, asset_name: &str) -> Result<u32, BuildError> {
            if asset_name.len() > u16::MAX as usize {
                return Err(BuildError::NameTooLong);
            }
            self.push(TYPE_DRAWABLE, Entry::Bytes(asset_name.as_bytes().to_vec()))
        }

        pub fn push_layout(&mut self, words: Vec<u32>) -> Result<u32, BuildError> {
            self.push(TYPE_LAYOUT, Entry::Words(words))
        }

        pub fn build(&self) -> Result<Vec<u8>, BuildError> {
            let present: Vec<u8> = (1..self.types.len() as u8)
                .filter(|&t| !self.types[t as usize].is_empty())
                .collect();
            let mut out = Vec::new();
            out.push(TABLE_VERSION);
            out.push(present.len() as u8);
            out.extend_from_slice(&[0, 0]);

            // Value arrays sit right after the directory, in type order.
            let mut values_offset = TABLE_HEADER_LEN + present.len() * DIR_ENTRY_LEN;
            for &ty in &present {
                let count = self.types[ty as usize].len();
                out.push(ty);
                out.push(0);
                out.extend_from_slice(&(count as u16).to_le_bytes());
                let off = u32::try_from(values_offset).map_err(|_| BuildError::TooLarge)?;
                out.extend_from_slice(&off.to_le_bytes());
                values_offset += count * 4;
            }

            // Blobs follow the last value array; lay them out first so the
            // value words can name their offsets.
            let mut blobs: Vec<u8> = Vec::new();
            let blob_base = values_offset;
            let mut values: Vec<u32> = Vec::new();
            for &ty in &present {
                for entry in &self.types[ty as usize] {
                    let at = |blobs: &Vec<u8>| {
                        u32::try_from(blob_base + blobs.len()).map_err(|_| BuildError::TooLarge)
                    };
                    match entry {
                        Entry::Value(v) => values.push(*v),
                        Entry::Bytes(b) => {
                            values.push(at(&blobs)?);
                            blobs.extend_from_slice(&(b.len() as u16).to_le_bytes());
                            blobs.extend_from_slice(b);
                        }
                        Entry::Words(w) => {
                            while !(blob_base + blobs.len()).is_multiple_of(4) {
                                blobs.push(0);
                            }
                            values.push(at(&blobs)?);
                            let count = u32::try_from(w.len()).map_err(|_| BuildError::TooLarge)?;
                            blobs.extend_from_slice(&count.to_le_bytes());
                            for word in w {
                                blobs.extend_from_slice(&word.to_le_bytes());
                            }
                        }
                    }
                }
            }
            for v in values {
                out.extend_from_slice(&v.to_le_bytes());
            }
            debug_assert_eq!(out.len(), blob_base);
            out.extend_from_slice(&blobs);
            Ok(out)
        }
    }
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn ids_compose_and_decompose() {
        let id = res_id(TYPE_COLOR, 3);
        assert_eq!(id, 0x7f02_0003);
        assert_eq!(res_type(id), Some(TYPE_COLOR));
        assert_eq!(res_entry(id), 3);
        assert_eq!(res_type(0x0102_0003), None);
    }

    #[test]
    fn round_trip_every_type() {
        let mut b = ResTableBuilder::new();
        assert!(b.is_empty());
        let hello = b.push_string("Hello").unwrap();
        let odd = b.push_string("odd").unwrap();
        let accent = b.push_value(TYPE_COLOR, 0xFF33_66CC).unwrap();
        let gap = b.push_value(TYPE_DIMEN, 12.5f32.to_bits()).unwrap();
        let answer = b.push_value(TYPE_INTEGER, (-42i32) as u32).unwrap();
        let yes = b.push_value(TYPE_BOOL, 1).unwrap();
        let main = b.push_layout(vec![1, 2, 3]).unwrap();
        let empty = b.push_layout(vec![]).unwrap();
        let logo = b.push_drawable("res/drawable/logo.png").unwrap();
        assert!(!b.is_empty());

        let data = b.build().unwrap();
        let t = ResTable::parse(&data).unwrap();
        assert_eq!(t.string(hello), Some(&b"Hello"[..]));
        assert_eq!(t.string(odd), Some(&b"odd"[..]));
        assert_eq!(t.value_of(TYPE_COLOR, accent), Some(0xFF33_66CC));
        assert_eq!(t.value_of(TYPE_DIMEN, gap).map(f32::from_bits), Some(12.5));
        assert_eq!(t.value_of(TYPE_INTEGER, answer), Some((-42i32) as u32));
        assert_eq!(t.value_of(TYPE_BOOL, yes), Some(1));
        let l = t.layout(main).unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!((l.word(0), l.word(2), l.word(3)), (Some(1), Some(3), None));
        assert!(t.layout(empty).unwrap().is_empty());
        assert_eq!(t.drawable_name(logo), Some(&b"res/drawable/logo.png"[..]));
        assert_eq!(t.entry_count(TYPE_STRING), 2);
        assert_eq!(t.entry_count(TYPE_ID), 0);

        // Layout blobs are 4-byte aligned within the section.
        assert_eq!(t.value(main).unwrap() % 4, 0);
        assert_eq!(t.value(empty).unwrap() % 4, 0);
    }

    #[test]
    fn wrong_type_and_out_of_range_miss() {
        let mut b = ResTableBuilder::new();
        let s = b.push_string("x").unwrap();
        let c = b.push_value(TYPE_COLOR, 1).unwrap();
        let data = b.build().unwrap();
        let t = ResTable::parse(&data).unwrap();
        assert_eq!(t.string(c), None);
        assert_eq!(t.value_of(TYPE_COLOR, s), None);
        assert_eq!(t.string(res_id(TYPE_STRING, 1)), None);
        assert_eq!(t.value(res_id(TYPE_LAYOUT, 0)), None);
        assert_eq!(t.value(0), None);
    }

    #[test]
    fn hostile_tables_are_refused_not_indexed() {
        assert_eq!(ResTable::parse(&[]).unwrap_err(), PapkError::Truncated);
        assert_eq!(
            ResTable::parse(&[9, 0, 0, 0]).unwrap_err(),
            PapkError::UnsupportedVersion
        );
        // One type whose directory entry is missing.
        assert_eq!(
            ResTable::parse(&[1, 1, 0, 0]).unwrap_err(),
            PapkError::Truncated
        );
        // A value array that runs past the section.
        let mut d = vec![1, 1, 0, 0, TYPE_COLOR, 0, 2, 0];
        d.extend_from_slice(&12u32.to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        assert_eq!(ResTable::parse(&d).unwrap_err(), PapkError::Truncated);
        // A string whose offset points outside.
        let mut d = vec![1, 1, 0, 0, TYPE_STRING, 0, 1, 0];
        d.extend_from_slice(&12u32.to_le_bytes());
        d.extend_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let t = ResTable::parse(&d).unwrap();
        assert_eq!(t.string(res_id(TYPE_STRING, 0)), None);
    }
}
