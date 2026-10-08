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
//! `resources.arsc` + binary XML, with one density and one locale and a
//! closed subset of configuration qualifiers ([`config`]).
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
//!     style    (9): offset (4-aligned) of [u32 word_count][u32 words…]
//!     overrides (10): offset (4-aligned) of a block — the values a
//!       `values-<q>` / `layout-<q>` directory overrides ([`config`]):
//!       [u16 sw_dp][u16 w_dp][u16 h_dp][u8 orientation][u8 touch]
//!       [u32 pair_count] pair_count × ([u32 id][u32 value])
//!       where `value` is what the base entry of `id` would hold
//!   blobs, in any order
//! ```
//!
//! `id` resources (`@+id/title`, [`TYPE_ID`]) exist only as `R.id`
//! constants; they have no table.
//!
//! A style's words are `(attr, value)` pairs from [`theme::attr`]: the
//! colours a theme gives the framework's own widgets, which is all of a
//! `<style>` the device needs. Everything else a style or a theme says is
//! folded into the layouts that use it, at build time.
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
//! A node of class [`layout::class::CUSTOM`] is a view class of the app's
//! own; its first attribute is [`layout::attr::CLASS_NAME`], the class name
//! as a pooled string, which the inflater hands to a factory.
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
pub const TYPE_STYLE: u8 = 9;
/// Configuration override blocks (docs/designs/app-portability-2026-10.md
/// D8): never an `R` type, read through [`ResTable::with`]. A reader that
/// predates it skips the type and sees the base values.
pub const TYPE_OVERRIDES: u8 = 10;

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
        TYPE_STYLE => "style",
        TYPE_OVERRIDES => "overrides",
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

/// Resource configuration qualifiers — the closed subset of Android's that
/// a `values-…` or `layout-…` directory may carry, in Android's precedence
/// order: `sw<N>dp`, `w<N>dp`, `h<N>dp`, `land` / `port`, `notouch` /
/// `finger`. The packer parses directory names into [`Qualifiers`] and the
/// runtime ranks the blocks that match its [`Config`] with [`best_first`],
/// once, when the app's resources open; both sides share this code so they
/// cannot disagree.
pub mod config {
    /// Width and height are in dp, which is a pixel of the app's window.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Qualifiers {
        /// `sw<N>dp`: the window's shorter side is at least N. 0 = unspecified.
        pub sw_dp: u16,
        /// `w<N>dp`: the window is at least N wide. 0 = unspecified.
        pub w_dp: u16,
        /// `h<N>dp`: the window is at least N tall. 0 = unspecified.
        pub h_dp: u16,
        /// [`ORIENTATION_PORT`] / [`ORIENTATION_LAND`], 0 = unspecified.
        pub orientation: u8,
        /// [`TOUCH_NOTOUCH`] / [`TOUCH_FINGER`], 0 = unspecified.
        pub touch: u8,
    }

    /// `port`: the window is at least as tall as it is wide.
    pub const ORIENTATION_PORT: u8 = 1;
    /// `land`: the window is wider than it is tall.
    pub const ORIENTATION_LAND: u8 = 2;
    /// `notouch`: the board has no touch panel.
    pub const TOUCH_NOTOUCH: u8 = 1;
    /// `finger`: the board has one.
    pub const TOUCH_FINGER: u8 = 2;

    /// What the app runs with: its window and the board's input.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Config {
        pub w_dp: u16,
        pub h_dp: u16,
        pub touch: bool,
    }

    impl Config {
        pub fn sw_dp(&self) -> u16 {
            self.w_dp.min(self.h_dp)
        }
        pub fn orientation(&self) -> u8 {
            if self.w_dp > self.h_dp {
                ORIENTATION_LAND
            } else {
                ORIENTATION_PORT
            }
        }
        fn touch_kind(&self) -> u8 {
            if self.touch {
                TOUCH_FINGER
            } else {
                TOUCH_NOTOUCH
            }
        }
    }

    /// Why a directory's qualifier text was refused.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum QualifierError {
        /// Not one of the subset (`values-night`, `layout-xlarge`, …).
        Unsupported,
        /// Out of Android's order (`land-sw320dp`), or the same one twice.
        Order,
        /// `sw0dp`, `w99999dp`, `wdp`.
        BadNumber,
    }

    impl Qualifiers {
        pub const fn none() -> Self {
            Qualifiers {
                sw_dp: 0,
                w_dp: 0,
                h_dp: 0,
                orientation: 0,
                touch: 0,
            }
        }

        pub fn is_empty(&self) -> bool {
            *self == Self::none()
        }

        /// Parse what follows `values-` or `layout-` in a directory name:
        /// `sw320dp`, `w320dp-land`, `land-finger`. Qualifiers must come in
        /// precedence order, each at most once, as Android's `aapt` wants.
        pub fn parse(spec: &str) -> Result<Qualifiers, QualifierError> {
            let mut q = Qualifiers::none();
            // Position in the precedence order of the last qualifier seen.
            let mut rank = 0u8;
            for part in spec.split('-') {
                let (this, value): (u8, Option<u16>) = if let Some(n) = part.strip_prefix("sw") {
                    (1, Some(dp(n)?))
                } else if let Some(n) = part.strip_prefix('w') {
                    (2, Some(dp(n)?))
                } else if let Some(n) = part.strip_prefix('h') {
                    (3, Some(dp(n)?))
                } else {
                    match part {
                        "port" | "land" => (4, None),
                        "notouch" | "finger" => (5, None),
                        _ => return Err(QualifierError::Unsupported),
                    }
                };
                if this <= rank {
                    return Err(QualifierError::Order);
                }
                rank = this;
                match this {
                    1 => q.sw_dp = value.unwrap_or(0),
                    2 => q.w_dp = value.unwrap_or(0),
                    3 => q.h_dp = value.unwrap_or(0),
                    4 => {
                        q.orientation = if part == "land" {
                            ORIENTATION_LAND
                        } else {
                            ORIENTATION_PORT
                        }
                    }
                    _ => {
                        q.touch = if part == "finger" {
                            TOUCH_FINGER
                        } else {
                            TOUCH_NOTOUCH
                        }
                    }
                }
            }
            Ok(q)
        }

        /// Whether every qualifier this directory names is met by `c`.
        pub fn matches(&self, c: &Config) -> bool {
            (self.sw_dp == 0 || c.sw_dp() >= self.sw_dp)
                && (self.w_dp == 0 || c.w_dp >= self.w_dp)
                && (self.h_dp == 0 || c.h_dp >= self.h_dp)
                && (self.orientation == 0 || self.orientation == c.orientation())
                && (self.touch == 0 || self.touch == c.touch_kind())
        }

        /// Android's `isBetterThan` for two directories that both match: the
        /// first qualifier, in precedence order, on which they differ decides —
        /// a larger size bound wins, and a named qualifier beats an unnamed one.
        pub fn is_better_than(&self, other: &Qualifiers) -> bool {
            if self.sw_dp != other.sw_dp {
                return self.sw_dp > other.sw_dp;
            }
            if self.w_dp != other.w_dp {
                return self.w_dp > other.w_dp;
            }
            if self.h_dp != other.h_dp {
                return self.h_dp > other.h_dp;
            }
            if self.orientation != other.orientation {
                return self.orientation != 0;
            }
            if self.touch != other.touch {
                return self.touch != 0;
            }
            false
        }
    }

    /// `<N>dp` → N, 1..=u16::MAX.
    fn dp(text: &str) -> Result<u16, QualifierError> {
        let n = text.strip_suffix("dp").ok_or(QualifierError::BadNumber)?;
        if n.is_empty() || n.len() > 5 || !n.bytes().all(|b| b.is_ascii_digit()) {
            return Err(QualifierError::BadNumber);
        }
        let v: u32 = n.parse().map_err(|_| QualifierError::BadNumber)?;
        u16::try_from(v)
            .ok()
            .filter(|v| *v >= 1)
            .ok_or(QualifierError::BadNumber)
    }

    /// Rank the blocks that match `c`, best first, into `out` as indices into
    /// `blocks`; returns how many were written (at most `out.len()`, the
    /// rest dropped — the runtime keeps a handful). No allocation.
    pub fn best_first<I>(blocks: I, c: &Config, out: &mut [u8]) -> usize
    where
        I: IntoIterator<Item = Qualifiers>,
    {
        let mut n = 0usize;
        let mut kept: [Qualifiers; 16] = [Qualifiers::none(); 16];
        for (i, q) in blocks.into_iter().enumerate() {
            if i > u8::MAX as usize || !q.matches(c) {
                continue;
            }
            // Insertion sort by precedence; the array of qualifiers rides
            // along so comparisons need no second look at the table.
            let mut at = n.min(out.len());
            while at > 0 && q.is_better_than(&kept[at - 1]) {
                at -= 1;
            }
            if at >= out.len() {
                continue;
            }
            let last = n.min(out.len() - 1);
            let mut j = last;
            while j > at {
                out[j] = out[j - 1];
                kept[j] = kept[j - 1];
                j -= 1;
            }
            out[at] = i as u8;
            kept[at] = q;
            if n < out.len() {
                n += 1;
            }
        }
        n
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn directory_names_parse_in_androids_order() {
            assert_eq!(
                Qualifiers::parse("sw320dp-land-finger"),
                Ok(Qualifiers {
                    sw_dp: 320,
                    w_dp: 0,
                    h_dp: 0,
                    orientation: ORIENTATION_LAND,
                    touch: TOUCH_FINGER
                })
            );
            assert_eq!(Qualifiers::parse("w320dp").unwrap().w_dp, 320);
            assert_eq!(Qualifiers::parse("h480dp").unwrap().h_dp, 480);
            assert_eq!(Qualifiers::parse("notouch").unwrap().touch, TOUCH_NOTOUCH);
            assert_eq!(
                Qualifiers::parse("land-sw320dp"),
                Err(QualifierError::Order)
            );
            assert_eq!(Qualifiers::parse("land-port"), Err(QualifierError::Order));
            assert_eq!(Qualifiers::parse("night"), Err(QualifierError::Unsupported));
            assert_eq!(
                Qualifiers::parse("xlarge"),
                Err(QualifierError::Unsupported)
            );
            assert_eq!(Qualifiers::parse("sw0dp"), Err(QualifierError::BadNumber));
            assert_eq!(Qualifiers::parse("w320"), Err(QualifierError::BadNumber));
            assert_eq!(
                Qualifiers::parse("sw99999dp"),
                Err(QualifierError::BadNumber)
            );
        }

        #[test]
        fn matching_and_precedence_follow_android() {
            let wide_touch = Config {
                w_dp: 320,
                h_dp: 240,
                touch: true,
            };
            let sw320 = Qualifiers::parse("sw320dp").unwrap();
            let w320 = Qualifiers::parse("w320dp").unwrap();
            let land = Qualifiers::parse("land").unwrap();
            let finger = Qualifiers::parse("finger").unwrap();
            let w240_land = Qualifiers::parse("w240dp-land").unwrap();
            assert!(!sw320.matches(&wide_touch), "sw is the shorter side");
            assert!(w320.matches(&wide_touch));
            assert!(land.matches(&wide_touch));
            assert!(!Qualifiers::parse("port").unwrap().matches(&wide_touch));
            assert!(!Qualifiers::parse("notouch").unwrap().matches(&wide_touch));
            // A larger width bound beats a smaller one; width beats orientation.
            assert!(w320.is_better_than(&w240_land));
            assert!(w240_land.is_better_than(&land));
            assert!(land.is_better_than(&finger));
            assert!(!finger.is_better_than(&land));

            let blocks = [finger, land, w320, sw320, w240_land];
            let mut out = [0u8; 8];
            let n = best_first(blocks.iter().copied(), &wide_touch, &mut out);
            assert_eq!(&out[..n], &[2, 4, 1, 0]);

            // A small output keeps the best.
            let mut two = [0u8; 2];
            let n = best_first(blocks.iter().copied(), &wide_touch, &mut two);
            assert_eq!(&two[..n], &[2, 4]);
        }
    }
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
        pub const CIRCULAR_PROGRESS_INDICATOR: u8 = 17;
        pub const VIEW_PAGER2: u8 = 18;
        /// A view class of the app's own, named by the node's first
        /// attribute ([`super::attr::CLASS_NAME`]). No element name of its
        /// own, so not in [`ALL`].
        pub const CUSTOM: u8 = 19;
        /// A plain `android.view.View`.
        pub const VIEW: u8 = 20;
        /// `android.widget.Space`: an invisible view that takes room, weighted
        /// in a `LinearLayout` to push its neighbours apart.
        pub const SPACE: u8 = 21;

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
            ("CircularProgressIndicator", CIRCULAR_PROGRESS_INDICATOR),
            ("ViewPager2", VIEW_PAGER2),
            ("View", VIEW),
            ("Space", SPACE),
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
        /// Integer. Applied before `PROGRESS` whatever the XML order (with `MAX`).
        pub const MIN: u32 = 32;
        /// ARGB.
        pub const PROGRESS_TINT: u32 = 33;
        pub const PROGRESS_BACKGROUND_TINT: u32 = 34;
        pub const INDETERMINATE_TINT: u32 = 35;
        /// ARGB.
        pub const INDICATOR_COLOR: u32 = 36;
        pub const TRACK_COLOR: u32 = 37;
        /// Pixels.
        pub const TRACK_THICKNESS: u32 = 38;
        pub const INDICATOR_SIZE: u32 = 39;
        /// f32 bits, degrees.
        pub const START_ANGLE: u32 = 40;
        pub const SWEEP_ANGLE: u32 = 41;
        /// f32 bits, pixels (`TextView.setTextSize(COMPLEX_UNIT_PX, ..)`).
        pub const TEXT_SIZE: u32 = 42;
        /// Pixels. `layout_margin` is expanded to the four sides by the compiler.
        pub const LAYOUT_MARGIN_LEFT: u32 = 43;
        pub const LAYOUT_MARGIN_TOP: u32 = 44;
        pub const LAYOUT_MARGIN_RIGHT: u32 = 45;
        pub const LAYOUT_MARGIN_BOTTOM: u32 = 46;
        /// 0 / 1.
        pub const INCLUDE_FONT_PADDING: u32 = 47;
        /// A `<shape>` drawable background, flattened: its `<solid>` colour
        /// is `BACKGROUND`; these carry its corner radius (pixels) and its
        /// stroke (pixels, ARGB).
        pub const BACKGROUND_RADIUS: u32 = 48;
        pub const BACKGROUND_STROKE_WIDTH: u32 = 49;
        pub const BACKGROUND_STROKE_COLOR: u32 = 50;
        /// String resource id (pooled): the class a `CUSTOM` node names.
        /// Always the node's first attribute.
        pub const CLASS_NAME: u32 = 51;
        /// 0 / 1: `View.setKeepScreenOn`.
        pub const KEEP_SCREEN_ON: u32 = 52;
        /// Pixels: `View.setMinimumWidth` / `setMinimumHeight`, `TextView.setMaxWidth`.
        pub const MIN_WIDTH: u32 = 53;
        pub const MIN_HEIGHT: u32 = 54;
        pub const MAX_WIDTH: u32 = 55;

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
            ("MIN", MIN),
            ("PROGRESS_TINT", PROGRESS_TINT),
            ("PROGRESS_BACKGROUND_TINT", PROGRESS_BACKGROUND_TINT),
            ("INDETERMINATE_TINT", INDETERMINATE_TINT),
            ("INDICATOR_COLOR", INDICATOR_COLOR),
            ("TRACK_COLOR", TRACK_COLOR),
            ("TRACK_THICKNESS", TRACK_THICKNESS),
            ("INDICATOR_SIZE", INDICATOR_SIZE),
            ("START_ANGLE", START_ANGLE),
            ("SWEEP_ANGLE", SWEEP_ANGLE),
            ("TEXT_SIZE", TEXT_SIZE),
            ("LAYOUT_MARGIN_LEFT", LAYOUT_MARGIN_LEFT),
            ("LAYOUT_MARGIN_TOP", LAYOUT_MARGIN_TOP),
            ("LAYOUT_MARGIN_RIGHT", LAYOUT_MARGIN_RIGHT),
            ("LAYOUT_MARGIN_BOTTOM", LAYOUT_MARGIN_BOTTOM),
            ("INCLUDE_FONT_PADDING", INCLUDE_FONT_PADDING),
            ("BACKGROUND_RADIUS", BACKGROUND_RADIUS),
            ("BACKGROUND_STROKE_WIDTH", BACKGROUND_STROKE_WIDTH),
            ("BACKGROUND_STROKE_COLOR", BACKGROUND_STROKE_COLOR),
            ("CLASS_NAME", CLASS_NAME),
            ("KEEP_SCREEN_ON", KEEP_SCREEN_ON),
            ("MIN_WIDTH", MIN_WIDTH),
            ("MIN_HEIGHT", MIN_HEIGHT),
            ("MAX_WIDTH", MAX_WIDTH),
        ];
    }

    /// Pack a node header word.
    pub const fn node_header(class: u8, attr_count: u8, child_count: u16) -> u32 {
        class as u32 | (attr_count as u32) << 8 | (child_count as u32) << 16
    }
}

/// Codes of a style's word stream ([`TYPE_STYLE`]). Append only.
pub mod theme {
    /// The theme attributes the framework reads: the colours its own widgets
    /// default to. Every value is ARGB.
    pub mod attr {
        pub const COLOR_PRIMARY: u32 = 1;
        pub const COLOR_ON_PRIMARY: u32 = 2;
        pub const COLOR_BACKGROUND: u32 = 3;
        pub const COLOR_SURFACE: u32 = 4;
        pub const TEXT_COLOR_PRIMARY: u32 = 5;
        pub const TEXT_COLOR_SECONDARY: u32 = 6;
        pub const COLOR_OUTLINE: u32 = 7;

        /// `(item name in a <style>, without any `android:` prefix; constant
        /// name in Resources.java; code)`.
        pub const ALL: &[(&str, &str, u32)] = &[
            ("colorPrimary", "COLOR_PRIMARY", COLOR_PRIMARY),
            ("colorOnPrimary", "COLOR_ON_PRIMARY", COLOR_ON_PRIMARY),
            ("colorBackground", "COLOR_BACKGROUND", COLOR_BACKGROUND),
            ("colorSurface", "COLOR_SURFACE", COLOR_SURFACE),
            ("textColorPrimary", "TEXT_COLOR_PRIMARY", TEXT_COLOR_PRIMARY),
            (
                "textColorSecondary",
                "TEXT_COLOR_SECONDARY",
                TEXT_COLOR_SECONDARY,
            ),
            ("colorOutline", "COLOR_OUTLINE", COLOR_OUTLINE),
        ];
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
        self.words(TYPE_LAYOUT, id)
    }

    /// The `(attr, value)` words of a style resource ([`theme::attr`]).
    pub fn style(&self, id: u32) -> Option<Layout<'a>> {
        self.words(TYPE_STYLE, id)
    }

    /// The `[u32 word_count][words]` blob behind `id`, when it is of type `ty`.
    fn words(&self, ty: u8, id: u32) -> Option<Layout<'a>> {
        self.words_at(self.value_of(ty, id)? as usize)
    }

    /// The `[u32 word_count][words]` blob at `offset`.
    fn words_at(&self, offset: usize) -> Option<Layout<'a>> {
        let count_end = offset.checked_add(4)?;
        let c = self.data.get(offset..count_end)?;
        let count = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
        let words = self
            .data
            .get(count_end..count_end.checked_add(count.checked_mul(4)?)?)?;
        Some(Layout { words })
    }

    /// The configuration override blocks, in table order (the index is
    /// what [`config::best_first`] ranks and [`ResTable::with`] takes).
    pub fn overrides(&self) -> impl Iterator<Item = Overrides<'a>> + '_ {
        (0..self.entry_count(TYPE_OVERRIDES)).filter_map(move |i| self.override_block(i))
    }

    fn override_block(&self, index: u16) -> Option<Overrides<'a>> {
        let offset = self.value(res_id(TYPE_OVERRIDES, index))? as usize;
        let head = self.data.get(offset..offset.checked_add(12)?)?;
        let u16_at = |i: usize| u16::from_le_bytes([head[i], head[i + 1]]);
        let qualifiers = config::Qualifiers {
            sw_dp: u16_at(0),
            w_dp: u16_at(2),
            h_dp: u16_at(4),
            orientation: head[6],
            touch: head[7],
        };
        let count = u32::from_le_bytes([head[8], head[9], head[10], head[11]]) as usize;
        let start = offset + 12;
        let pairs = self
            .data
            .get(start..start.checked_add(count.checked_mul(8)?)?)?;
        Some(Overrides { qualifiers, pairs })
    }

    /// This table read through the override blocks `selected` — indices
    /// into [`overrides`](Self::overrides), best match first, as
    /// [`config::best_first`] ranks them: every accessor takes an
    /// overriding value before the base one. An empty selection is the
    /// table itself.
    pub fn with<'s>(&'s self, selected: &'s [u8]) -> Resolved<'a, 's> {
        Resolved {
            table: self,
            selected,
        }
    }
}

/// One `values-<q>` / `layout-<q>` directory's overrides: the ids it
/// redefines, with the value word each base entry would hold instead.
#[derive(Clone, Copy)]
pub struct Overrides<'a> {
    pub qualifiers: config::Qualifiers,
    pairs: &'a [u8],
}

impl<'a> Overrides<'a> {
    /// How many ids the block redefines.
    pub fn len(&self) -> usize {
        self.pairs.len() / 8
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The value word this block gives `id`, if it redefines it.
    pub fn value(&self, id: u32) -> Option<u32> {
        self.pairs().find_map(|(pid, v)| (pid == id).then_some(v))
    }

    /// `(id, value)` pairs, in block order.
    pub fn pairs(&self) -> impl Iterator<Item = (u32, u32)> + 'a {
        self.pairs.as_chunks::<8>().0.iter().map(|p| {
            (
                u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                u32::from_le_bytes([p[4], p[5], p[6], p[7]]),
            )
        })
    }
}

/// A [`ResTable`] with a configuration applied; see [`ResTable::with`].
#[derive(Clone, Copy)]
pub struct Resolved<'a, 's> {
    table: &'s ResTable<'a>,
    selected: &'s [u8],
}

impl<'a> Resolved<'a, '_> {
    /// The value word of `id`, only when `id` is of type `ty`: the best
    /// matching block's, else the base entry's.
    pub fn value_of(&self, ty: u8, id: u32) -> Option<u32> {
        if res_type(id)? != ty {
            return None;
        }
        for &index in self.selected {
            if let Some(v) = self
                .table
                .override_block(index as u16)
                .and_then(|b| b.value(id))
            {
                return Some(v);
            }
        }
        self.table.value(id)
    }

    pub fn string(&self, id: u32) -> Option<&'a [u8]> {
        self.table
            .blob_u16(self.value_of(TYPE_STRING, id)? as usize)
    }

    pub fn drawable_name(&self, id: u32) -> Option<&'a [u8]> {
        self.table
            .blob_u16(self.value_of(TYPE_DRAWABLE, id)? as usize)
    }

    pub fn layout(&self, id: u32) -> Option<Layout<'a>> {
        self.table
            .words_at(self.value_of(TYPE_LAYOUT, id)? as usize)
    }

    pub fn style(&self, id: u32) -> Option<Layout<'a>> {
        self.table.words_at(self.value_of(TYPE_STYLE, id)? as usize)
    }
}

// ── Writer ────────────────────────────────────────────────────────────────────

#[cfg(feature = "write")]
pub use write::{OverrideValue, ResTableBuilder};

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
        /// An override block ([`TYPE_OVERRIDES`]): the pairs' blobs are
        /// laid out first, then the block naming their offsets.
        Block(config::Qualifiers, Vec<(u32, OverrideValue)>),
    }

    /// What an override block gives an id: the shape of the base entry's
    /// value (a word, a `[u16 len][bytes]` blob, a `[u32 count][words]`
    /// blob).
    pub enum OverrideValue {
        Value(u32),
        Bytes(Vec<u8>),
        Words(Vec<u32>),
    }

    /// Builds RESOURCES section data. Entries of a type get consecutive
    /// indices in insertion order; each `push_*` returns the new id.
    #[derive(Default)]
    pub struct ResTableBuilder {
        /// Indexed by type; `types[0]` is unused, and so is `TYPE_ID`'s.
        types: [Vec<Entry>; 11],
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

        /// `words` are `(theme::attr code, value)` pairs.
        pub fn push_style(&mut self, words: Vec<u32>) -> Result<u32, BuildError> {
            self.push(TYPE_STYLE, Entry::Words(words))
        }

        /// One directory's overrides: `pairs` redefine ids the base
        /// table already holds, each with a value of its type's shape.
        /// Returns the block's index (what the runtime's selection names).
        pub fn push_overrides(
            &mut self,
            qualifiers: config::Qualifiers,
            pairs: Vec<(u32, OverrideValue)>,
        ) -> Result<u16, BuildError> {
            let id = self.push(TYPE_OVERRIDES, Entry::Block(qualifiers, pairs))?;
            Ok(res_entry(id))
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
                        Entry::Block(q, pairs) => {
                            // The pairs' own blobs first, so the block can
                            // name their offsets.
                            let mut words: Vec<u32> = Vec::with_capacity(pairs.len() * 2);
                            for (id, v) in pairs {
                                words.push(*id);
                                match v {
                                    OverrideValue::Value(x) => words.push(*x),
                                    OverrideValue::Bytes(b) => {
                                        words.push(at(&blobs)?);
                                        blobs.extend_from_slice(&(b.len() as u16).to_le_bytes());
                                        blobs.extend_from_slice(b);
                                    }
                                    OverrideValue::Words(w) => {
                                        while !(blob_base + blobs.len()).is_multiple_of(4) {
                                            blobs.push(0);
                                        }
                                        words.push(at(&blobs)?);
                                        let count = u32::try_from(w.len())
                                            .map_err(|_| BuildError::TooLarge)?;
                                        blobs.extend_from_slice(&count.to_le_bytes());
                                        for word in w {
                                            blobs.extend_from_slice(&word.to_le_bytes());
                                        }
                                    }
                                }
                            }
                            while !(blob_base + blobs.len()).is_multiple_of(4) {
                                blobs.push(0);
                            }
                            values.push(at(&blobs)?);
                            blobs.extend_from_slice(&q.sw_dp.to_le_bytes());
                            blobs.extend_from_slice(&q.w_dp.to_le_bytes());
                            blobs.extend_from_slice(&q.h_dp.to_le_bytes());
                            blobs.push(q.orientation);
                            blobs.push(q.touch);
                            let count =
                                u32::try_from(pairs.len()).map_err(|_| BuildError::TooLarge)?;
                            blobs.extend_from_slice(&count.to_le_bytes());
                            for word in words {
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
    fn overrides_resolve_by_configuration_and_hide_from_old_readers() {
        use config::{Config, Qualifiers};
        let mut b = ResTableBuilder::new();
        let name = b.push_string("narrow").unwrap();
        let gap = b.push_value(TYPE_DIMEN, 8f32.to_bits()).unwrap();
        let row = b.push_layout(vec![1, 2]).unwrap();
        let wide = b
            .push_overrides(
                Qualifiers::parse("w320dp").unwrap(),
                vec![(name, OverrideValue::Bytes(b"wide".to_vec()))],
            )
            .unwrap();
        let land = b
            .push_overrides(
                Qualifiers::parse("land").unwrap(),
                vec![
                    (gap, OverrideValue::Value(12f32.to_bits())),
                    (row, OverrideValue::Words(vec![3, 4, 5])),
                    (name, OverrideValue::Bytes(b"landscape".to_vec())),
                ],
            )
            .unwrap();
        assert_eq!((wide, land), (0, 1));
        let data = b.build().unwrap();
        let t = ResTable::parse(&data).unwrap();

        // The base accessors never see a block: an old firmware reads these.
        assert_eq!(t.string(name), Some(&b"narrow"[..]));
        assert_eq!(t.value_of(TYPE_DIMEN, gap), Some(8f32.to_bits()));
        assert_eq!(t.layout(row).unwrap().len(), 2);
        assert_eq!(t.entry_count(TYPE_OVERRIDES), 2);
        let blocks: Vec<Overrides> = t.overrides().collect();
        assert_eq!(blocks[0].qualifiers.w_dp, 320);
        assert_eq!(blocks[1].len(), 3);

        // 320x240: both apply, the width bound outranks the orientation.
        let c = Config {
            w_dp: 320,
            h_dp: 240,
            touch: false,
        };
        let mut sel = [0u8; 4];
        let n = config::best_first(t.overrides().map(|o| o.qualifiers), &c, &mut sel);
        assert_eq!(&sel[..n], &[0, 1]);
        let r = t.with(&sel[..n]);
        assert_eq!(r.string(name), Some(&b"wide"[..]));
        assert_eq!(r.value_of(TYPE_DIMEN, gap), Some(12f32.to_bits()));
        assert_eq!(r.layout(row).unwrap().len(), 3);

        // 240x240: neither applies.
        let c = Config {
            w_dp: 240,
            h_dp: 240,
            touch: true,
        };
        let n = config::best_first(t.overrides().map(|o| o.qualifiers), &c, &mut sel);
        assert_eq!(n, 0);
        let r = t.with(&sel[..n]);
        assert_eq!(r.string(name), Some(&b"narrow"[..]));
        assert_eq!(r.layout(row).unwrap().len(), 2);
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
