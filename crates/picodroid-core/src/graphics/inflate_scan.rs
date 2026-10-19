// SPDX-License-Identifier: GPL-3.0-only
//! One node of a compiled layout, read for the inflater's native path.
//!
//! `LayoutInflater.nativeInflateNode` makes a framework view in one call:
//! this module reads the node's words — the header, then `(attr, value)`
//! pairs (`papk_format::res::layout`) — and sorts the attributes into what
//! the LVGL side applies itself, what the Java side needs for the
//! `LayoutParams` object, and what is handed back for Java's own setters
//! (the attributes of widgets this path does not cover, and the `<shape>`
//! background, which builds a `GradientDrawable`). Pure, so it is tested on
//! the host; `lvgl/inflate.rs` applies the result.

use papk_format::res::layout::{attr, class};

/// `View.WRAP_CONTENT`, the layout size of an attribute left out.
pub const WRAP_CONTENT: i32 = -2;

/// Attributes handed back to Java per node, at most. A node with more takes
/// the Java path whole.
pub const MAX_DEFERRED: usize = 8;

/// The framework classes the native path makes. The rest — the compound
/// buttons, whose constructors reach their widget, the bars and pickers,
/// `EditText`, `Spinner`, `ListView`, `ViewPager2` — keep the Java path.
pub fn is_fast_class(cls: u8) -> bool {
    matches!(
        cls,
        class::LINEAR_LAYOUT
            | class::FRAME_LAYOUT
            | class::SCROLL_VIEW
            | class::TEXT_VIEW
            | class::BUTTON
            | class::IMAGE_VIEW
            | class::RADIO_GROUP
            | class::VIEW
            | class::SPACE
            | class::GRID_LAYOUT
    )
}

/// A class whose widget is a label, or holds one: the text attributes apply.
pub fn is_text_class(cls: u8) -> bool {
    matches!(cls, class::TEXT_VIEW | class::BUTTON)
}

/// A class whose widget is a flex layout: orientation and gravity apply.
pub fn is_linear_class(cls: u8) -> bool {
    matches!(cls, class::LINEAR_LAYOUT | class::RADIO_GROUP)
}

/// A plain `View` or `Space`: a drawing object that Java measures itself
/// when a dimension is `wrap_content`.
pub fn is_drawing_class(cls: u8) -> bool {
    matches!(cls, class::VIEW | class::SPACE)
}

/// What the node said. Every `Option` is `Some` only for an attribute the
/// node carried, so the LVGL side touches nothing the layout left out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Node {
    pub cls: u8,
    /// Index of the word after this node's attributes: its first child's
    /// header, or the next sibling's.
    pub next_pos: usize,
    pub child_count: u32,
    pub id: Option<i32>,
    // LayoutParams.
    pub width: i32,
    pub height: i32,
    pub weight_bits: i32,
    pub layout_gravity: i32,
    pub margin: [i32; 4],
    // View.
    pub padding: Option<[i32; 4]>,
    /// A plain colour background. A `<shape>` (radius or stroke present) is
    /// handed back whole instead.
    pub background: Option<u32>,
    pub visibility: Option<i32>,
    pub enabled: Option<bool>,
    pub focusable: Option<bool>,
    pub alpha: Option<f32>,
    pub keep_screen_on: Option<bool>,
    pub min_width: Option<i32>,
    pub min_height: Option<i32>,
    // TextView.
    pub text: Option<i32>,
    pub text_color: Option<u32>,
    pub text_size: Option<f32>,
    pub include_font_padding: Option<bool>,
    pub single_line: Option<bool>,
    pub max_lines: Option<i32>,
    pub ellipsize: Option<i32>,
    pub gravity: Option<i32>,
    pub max_width: Option<i32>,
    // LinearLayout.
    pub orientation: Option<i32>,
    /// `(attr, value)` pairs for Java's `apply`.
    pub deferred: [(i32, i32); MAX_DEFERRED],
    pub deferred_count: usize,
    /// More than [`MAX_DEFERRED`] pairs: the node takes the Java path.
    pub too_many_deferred: bool,
}

/// Why a node could not be read: the stream ended inside it (a layout id
/// that is not a layout, or one compiled by a newer packer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Truncated;

impl Node {
    fn defer(&mut self, attr: u32, value: i32) {
        if self.deferred_count < MAX_DEFERRED {
            self.deferred[self.deferred_count] = (attr as i32, value);
            self.deferred_count += 1;
        } else {
            self.too_many_deferred = true;
        }
    }
}

/// Read the node at `pos` through `word`, which answers a word index or
/// `None` past the end of the stream.
pub fn scan(word: impl Fn(usize) -> Option<u32>, pos: usize) -> Result<Node, Truncated> {
    let header = word(pos).ok_or(Truncated)?;
    let attrs = ((header >> 8) & 0xFF) as usize;
    let mut node = Node {
        cls: (header & 0xFF) as u8,
        child_count: header >> 16,
        width: WRAP_CONTENT,
        height: WRAP_CONTENT,
        ..Node::default()
    };
    let mut padding = [0i32; 4];
    let mut padded = false;
    let mut shape: [Option<i32>; 3] = [None; 3];
    let mut background: Option<i32> = None;
    let mut at = pos + 1;
    for _ in 0..attrs {
        let a = word(at).ok_or(Truncated)?;
        let v = word(at + 1).ok_or(Truncated)? as i32;
        at += 2;
        let text = is_text_class(node.cls);
        match a {
            attr::ID => node.id = Some(v),
            attr::LAYOUT_WIDTH => node.width = v,
            attr::LAYOUT_HEIGHT => node.height = v,
            attr::LAYOUT_WEIGHT => node.weight_bits = v,
            attr::LAYOUT_GRAVITY => node.layout_gravity = v,
            attr::PADDING_LEFT => (padding[0], padded) = (v, true),
            attr::PADDING_TOP => (padding[1], padded) = (v, true),
            attr::PADDING_RIGHT => (padding[2], padded) = (v, true),
            attr::PADDING_BOTTOM => (padding[3], padded) = (v, true),
            attr::LAYOUT_MARGIN_LEFT => node.margin[0] = v,
            attr::LAYOUT_MARGIN_TOP => node.margin[1] = v,
            attr::LAYOUT_MARGIN_RIGHT => node.margin[2] = v,
            attr::LAYOUT_MARGIN_BOTTOM => node.margin[3] = v,
            attr::BACKGROUND => background = Some(v),
            attr::BACKGROUND_RADIUS => shape[0] = Some(v),
            attr::BACKGROUND_STROKE_WIDTH => shape[1] = Some(v),
            attr::BACKGROUND_STROKE_COLOR => shape[2] = Some(v),
            attr::VISIBILITY => node.visibility = Some(v),
            attr::ENABLED => node.enabled = Some(v != 0),
            attr::FOCUSABLE => node.focusable = Some(v != 0),
            attr::ALPHA => node.alpha = Some(f32::from_bits(v as u32)),
            attr::KEEP_SCREEN_ON => node.keep_screen_on = Some(v != 0),
            attr::MIN_WIDTH => node.min_width = Some(v),
            attr::MIN_HEIGHT => node.min_height = Some(v),
            // The text attributes of a class that is not a label are what
            // Java's `apply` drops: its `instanceof` finds no setter.
            attr::TEXT if text => node.text = Some(v),
            attr::TEXT_COLOR if text => node.text_color = Some(v as u32),
            attr::TEXT_SIZE if text => node.text_size = Some(f32::from_bits(v as u32)),
            attr::INCLUDE_FONT_PADDING if text => node.include_font_padding = Some(v != 0),
            attr::SINGLE_LINE if text => node.single_line = Some(v != 0),
            attr::MAX_LINES if text => node.max_lines = Some(v),
            attr::ELLIPSIZE if text => node.ellipsize = Some(v),
            attr::MAX_WIDTH if text => node.max_width = Some(v),
            attr::GRAVITY if text || is_linear_class(node.cls) => node.gravity = Some(v),
            attr::ORIENTATION if is_linear_class(node.cls) => node.orientation = Some(v),
            // A grid's orientation is Java's to apply, with its counts.
            attr::ORIENTATION if node.cls == class::GRID_LAYOUT => node.defer(a, v),
            attr::TEXT
            | attr::TEXT_COLOR
            | attr::TEXT_SIZE
            | attr::INCLUDE_FONT_PADDING
            | attr::SINGLE_LINE
            | attr::MAX_LINES
            | attr::ELLIPSIZE
            | attr::MAX_WIDTH
            | attr::GRAVITY
            | attr::ORIENTATION => {}
            other => node.defer(other, v),
        }
    }
    if padded {
        node.padding = Some(padding);
    }
    // A `<shape>`: the four words go back to Java, which builds the
    // GradientDrawable so `getBackground()` can return it.
    if shape.iter().any(Option::is_some) {
        node.defer(attr::BACKGROUND, background.unwrap_or(0));
        for (i, code) in [
            attr::BACKGROUND_RADIUS,
            attr::BACKGROUND_STROKE_WIDTH,
            attr::BACKGROUND_STROKE_COLOR,
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(v) = shape[i] {
                node.defer(code, v);
            }
        }
    } else if let Some(fill) = background {
        node.background = Some(fill as u32);
    }
    node.next_pos = at;
    Ok(node)
}

// ── TextView's packed line mode ─────────────────────────────────────────────
//
// `TextView.mLineMode` packs the ellipsize kind, the single-line flag, the
// max-lines count and two unrelated flags into one int (the constants are
// `TextView.java`'s; `line_mode_packs_as_text_view_does` pins them).

pub const ELLIPSIZE_MASK: i32 = 0x7;
pub const SINGLE_LINE: i32 = 0x8;
pub const FONT_PAD_OFF: i32 = 0x10;
pub const TEXT_COLOR_SET: i32 = 0x20;
pub const MAX_LINES_SHIFT: i32 = 8;
pub const MAX_LINES_LIMIT: i32 = 0xFFFF;

/// `mLineMode` for a label that got `ellipsize`, `single_line`, `max_lines`
/// (0 for none), with `font_pad_off` and `color_set` as the setters leave
/// them. `max_lines` is clamped as `setMaxLines` clamps it.
pub fn pack_line_mode(
    ellipsize: i32,
    single_line: bool,
    max_lines: i32,
    font_pad_off: bool,
    color_set: bool,
) -> i32 {
    let lines = max_lines.clamp(0, MAX_LINES_LIMIT);
    (ellipsize & ELLIPSIZE_MASK)
        | if single_line { SINGLE_LINE } else { 0 }
        | if font_pad_off { FONT_PAD_OFF } else { 0 }
        | if color_set { TEXT_COLOR_SET } else { 0 }
        | (lines << MAX_LINES_SHIFT)
}

// ── Gravity ─────────────────────────────────────────────────────────────────

pub const GRAVITY_HORIZONTAL_MASK: i32 = 0x07;
pub const GRAVITY_VERTICAL_MASK: i32 = 0x70;
pub const GRAVITY_RIGHT: i32 = 0x05;
pub const GRAVITY_CENTER_HORIZONTAL: i32 = 0x01;
pub const GRAVITY_BOTTOM: i32 = 0x50;
pub const GRAVITY_CENTER_VERTICAL: i32 = 0x10;
pub const GRAVITY_START: i32 = 0x0080_0003;
pub const GRAVITY_TOP: i32 = 0x30;

/// `TextView.setGravity`'s normalisation: an axis the gravity leaves out is
/// `START` / `TOP`, Android's default.
pub fn text_gravity(gravity: i32) -> i32 {
    let mut g = gravity;
    if g & GRAVITY_HORIZONTAL_MASK == 0 {
        g |= GRAVITY_START;
    }
    if g & GRAVITY_VERTICAL_MASK == 0 {
        g |= GRAVITY_TOP;
    }
    g
}

/// A `FrameLayout` child's offset from the edge or centre its gravity
/// names, as `ViewGroup.addView` computes it from the margins: moved in by
/// the margin on the side the gravity pulls toward, and by the difference
/// of the two when centred. `margin` is `[left, top, right, bottom]`.
pub fn frame_offset(gravity: i32, margin: [i32; 4]) -> (i32, i32) {
    let [left, top, right, bottom] = margin;
    let dx = match gravity & GRAVITY_HORIZONTAL_MASK {
        GRAVITY_RIGHT => -right,
        GRAVITY_CENTER_HORIZONTAL => left - right,
        _ => left,
    };
    let dy = match gravity & GRAVITY_VERTICAL_MASK {
        GRAVITY_BOTTOM => -bottom,
        GRAVITY_CENTER_VERTICAL => top - bottom,
        _ => top,
    };
    (dx, dy)
}

/// `LinearLayout.LayoutParams.weight` as the flex-grow `ViewGroup.addView`
/// sets: ten times the weight, at least one, so fractional ratios survive
/// the integer; 0 for no weight.
pub fn flex_grow(weight_bits: i32) -> i32 {
    let weight = f32::from_bits(weight_bits as u32);
    if weight > 0.0 {
        // `Math.round`: the floor of x + 0.5, which `as` is for a positive x.
        ((weight * 10.0 + 0.5) as i32).max(1)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(ws: &[u32]) -> impl Fn(usize) -> Option<u32> + '_ {
        move |i| ws.get(i).copied()
    }

    fn header(cls: u8, attrs: u32, children: u32) -> u32 {
        cls as u32 | (attrs << 8) | (children << 16)
    }

    #[test]
    fn a_label_with_the_usual_attributes() {
        let ws = [
            header(class::TEXT_VIEW, 6, 0),
            attr::LAYOUT_WIDTH,
            -1i32 as u32,
            attr::TEXT,
            0x7f03_0001,
            attr::TEXT_COLOR,
            0xFF11_2233,
            attr::SINGLE_LINE,
            1,
            attr::PADDING_LEFT,
            6,
            attr::LAYOUT_MARGIN_TOP,
            2,
            // The next sibling's header.
            header(class::FRAME_LAYOUT, 0, 0),
        ];
        let n = scan(words(&ws), 0).unwrap();
        assert_eq!(n.cls, class::TEXT_VIEW);
        assert_eq!(n.next_pos, 13);
        assert_eq!((n.width, n.height), (-1, WRAP_CONTENT));
        assert_eq!(n.text, Some(0x7f03_0001));
        assert_eq!(n.text_color, Some(0xFF11_2233));
        assert_eq!(n.single_line, Some(true));
        assert_eq!(n.padding, Some([6, 0, 0, 0]));
        assert_eq!(n.margin, [0, 2, 0, 0]);
        assert_eq!(n.deferred_count, 0);
        assert!(!n.too_many_deferred);
    }

    #[test]
    fn a_shape_background_goes_back_to_java_whole() {
        let ws = [
            header(class::FRAME_LAYOUT, 3, 2),
            attr::BACKGROUND,
            0xFF00_0000,
            attr::BACKGROUND_RADIUS,
            4,
            attr::BACKGROUND_STROKE_WIDTH,
            1,
        ];
        let n = scan(words(&ws), 0).unwrap();
        assert_eq!(n.child_count, 2);
        assert_eq!(n.background, None);
        assert_eq!(
            &n.deferred[..n.deferred_count],
            &[
                (attr::BACKGROUND as i32, 0xFF00_0000u32 as i32),
                (attr::BACKGROUND_RADIUS as i32, 4),
                (attr::BACKGROUND_STROKE_WIDTH as i32, 1)
            ]
        );
    }

    #[test]
    fn text_attributes_on_a_layout_are_dropped_as_java_drops_them() {
        let ws = [
            header(class::LINEAR_LAYOUT, 2, 0),
            attr::TEXT,
            7,
            attr::ORIENTATION,
            0,
        ];
        let n = scan(words(&ws), 0).unwrap();
        assert_eq!(n.text, None);
        assert_eq!(n.orientation, Some(0));
        assert_eq!(n.deferred_count, 0);
    }

    #[test]
    fn a_grid_defers_its_orientation_and_a_cell_its_six_words() {
        let ws = [
            header(class::GRID_LAYOUT, 2, 1),
            attr::COLUMN_COUNT,
            3,
            attr::ORIENTATION,
            1,
            header(class::TEXT_VIEW, 6, 0),
            attr::LAYOUT_COLUMN,
            1,
            attr::LAYOUT_ROW,
            2,
            attr::LAYOUT_COLUMN_SPAN,
            2,
            attr::LAYOUT_ROW_SPAN,
            1,
            attr::LAYOUT_COLUMN_WEIGHT,
            1.0f32.to_bits(),
            attr::LAYOUT_ROW_WEIGHT,
            0,
        ];
        let grid = scan(words(&ws), 0).unwrap();
        assert!(is_fast_class(grid.cls));
        assert_eq!(grid.orientation, None);
        assert_eq!(
            &grid.deferred[..grid.deferred_count],
            &[
                (attr::COLUMN_COUNT as i32, 3),
                (attr::ORIENTATION as i32, 1)
            ]
        );
        let cell = scan(words(&ws), grid.next_pos).unwrap();
        assert_eq!(cell.deferred_count, 6);
        assert!(!cell.too_many_deferred);
        assert_eq!(cell.deferred[0], (attr::LAYOUT_COLUMN as i32, 1));
        assert_eq!(cell.deferred[5], (attr::LAYOUT_ROW_WEIGHT as i32, 0));
    }

    #[test]
    fn unknown_attributes_are_deferred_and_too_many_is_flagged() {
        let mut ws = vec![header(class::IMAGE_VIEW, 9, 0)];
        for i in 0..9 {
            ws.push(attr::SRC);
            ws.push(i);
        }
        let n = scan(words(&ws), 0).unwrap();
        assert_eq!(n.deferred_count, MAX_DEFERRED);
        assert!(n.too_many_deferred);
        assert_eq!(n.next_pos, 19);
    }

    #[test]
    fn a_truncated_stream_is_an_error() {
        assert_eq!(scan(words(&[]), 0), Err(Truncated));
        assert_eq!(
            scan(words(&[header(class::TEXT_VIEW, 1, 0), attr::TEXT]), 0),
            Err(Truncated)
        );
    }

    #[test]
    fn line_mode_packs_as_text_view_does() {
        assert_eq!(pack_line_mode(0, false, 0, false, false), 0);
        assert_eq!(pack_line_mode(3, true, 0, false, true), 0x3 | 0x8 | 0x20);
        assert_eq!(pack_line_mode(0, false, 2, true, false), 0x10 | (2 << 8));
        assert_eq!(
            pack_line_mode(0, false, 0x1_0000, false, false),
            0xFFFF << 8
        );
        assert_eq!(pack_line_mode(0, false, -1, false, false), 0);
    }

    #[test]
    fn text_gravity_fills_the_axes_left_out() {
        assert_eq!(text_gravity(0), GRAVITY_START | GRAVITY_TOP);
        assert_eq!(text_gravity(GRAVITY_RIGHT), GRAVITY_RIGHT | GRAVITY_TOP);
        assert_eq!(text_gravity(GRAVITY_BOTTOM), GRAVITY_BOTTOM | GRAVITY_START);
    }

    #[test]
    fn frame_offsets_follow_add_view() {
        assert_eq!(frame_offset(0, [7, 9, 0, 0]), (7, 9));
        assert_eq!(
            frame_offset(GRAVITY_RIGHT | GRAVITY_BOTTOM, [0, 0, 3, 4]),
            (-3, -4)
        );
        assert_eq!(
            frame_offset(
                GRAVITY_CENTER_HORIZONTAL | GRAVITY_CENTER_VERTICAL,
                [5, 1, 2, 7]
            ),
            (3, -6)
        );
    }

    #[test]
    fn flex_grow_scales_weights_by_ten() {
        assert_eq!(flex_grow(0), 0);
        assert_eq!(flex_grow(1.5f32.to_bits() as i32), 15);
        assert_eq!(flex_grow(0.01f32.to_bits() as i32), 1);
    }
}
