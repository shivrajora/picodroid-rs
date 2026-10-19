// SPDX-License-Identifier: GPL-3.0-only
//! The LVGL half of `LayoutInflater.nativeInflateNode`: one node of a
//! compiled layout made under its final parent, every attribute applied
//! under one style refresh.
//!
//! The Java inflater used to make a view with a dozen natives — one per
//! word of the layout, the create, one per setter, the size and the
//! re-parent — and a few hundred bytecodes around them, which is what made a
//! view cost the RP2350 some 7 ms (docs/designs/ui-build-budget-2026-10.md).
//! Here the node (`graphics::inflate_scan`) arrives already read; this
//! module creates the widget where it belongs and applies what the node
//! said through the same `view_ops` and widget shims the setters use, so
//! nothing is spelled twice.

use crate::graphics::gfx::{Handle, Visibility};
use crate::graphics::inflate_scan::{self as scan, Node};
use crate::lvgl_ffi::*;
use papk_format::res::layout::class;

use super::{events, handle_table, keep_on, lifecycle, style_batch, view_ops, widgets};

/// How the parent lays its children out: what `ViewGroup.addView` does with
/// the `LayoutParams` type, done here instead.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::graphics) enum ParentKind {
    /// Unattached (the root of an `inflate` with no root, or one not
    /// attached to it): Java sizes it later, as before.
    None,
    /// A `LinearLayout`: size, weight and margins.
    Linear,
    /// A `FrameLayout`: size, and the place its gravity and margins name.
    Frame,
    /// Any other framework group: size and margins.
    Other,
    /// A group class of the app's own: made under it, but Java's
    /// `addView` applies the params so an override of it runs.
    Custom,
    /// A `GridLayout`: made under it, and `GridLayout.addView` places it in
    /// its cell from the params Java builds out of the deferred words.
    Grid,
}

impl ParentKind {
    pub(in crate::graphics) fn from_java(kind: i32) -> Self {
        match kind {
            1 => Self::Linear,
            2 => Self::Frame,
            3 => Self::Other,
            4 => Self::Custom,
            5 => Self::Grid,
            _ => Self::None,
        }
    }
}

/// The parent to make the node under: the screen for 0, else the live
/// widget `handle` names. `None` for a handle the table no longer resolves
/// (its Activity finished), which the shim reports as the exception
/// `addView` throws for it.
pub(in crate::graphics) fn parent_of(handle: i32) -> Option<Obj> {
    if handle == 0 {
        lifecycle::screen_ptr()
    } else if handle_table::is_live(handle) {
        handle_table::get(handle)
    } else {
        None
    }
}

/// What the widget was told, for the Java wrapper's cached fields.
#[derive(Clone, Copy, Default)]
pub(in crate::graphics) struct Applied {
    pub handle: i32,
    /// `TextView.mLineMode` as the setters would have left it.
    pub line_mode: i32,
    /// `View.focusable` as the constructor and the attribute leave it.
    pub focusable: bool,
}

/// Make `node` under `parent` and apply its attributes. `text` is the
/// resolved `android:text`, for a label or a button. Handle 0 when LVGL's
/// pool refused the widget, as the plain creates return it.
pub(in crate::graphics) fn make(node: &Node, parent: Obj, kind: ParentKind, text: &str) -> Applied {
    let handle = match node.cls {
        class::LINEAR_LAYOUT | class::RADIO_GROUP => widgets::linear_layout::create_under(parent),
        class::FRAME_LAYOUT => widgets::frame_layout::create_under(parent),
        class::GRID_LAYOUT => widgets::grid_layout::create_under(parent),
        class::SCROLL_VIEW => widgets::scroll_view::create_under(parent),
        class::TEXT_VIEW => widgets::text_view::create_under(parent),
        class::BUTTON => widgets::button::create_under(parent, text),
        class::IMAGE_VIEW => widgets::image_view::create_under(parent),
        #[cfg(has_canvas)]
        class::VIEW | class::SPACE => widgets::canvas::create_under(parent),
        _ => 0,
    };
    let Some(obj) = handle_table::get(handle) else {
        return Applied::default();
    };
    let h = Handle::from_java(handle);
    let text_class = scan::is_text_class(node.cls);
    let mut applied = Applied {
        handle,
        line_mode: 0,
        focusable: false,
    };
    // Every style set below folds into one refresh (style_batch.rs); the
    // padding, margin and shape sets nest in it. PAD_TOP's flags — a layout
    // update — cover the batch, and the node has no children yet to walk.
    style_batch::with_one_refresh(obj, LV_STYLE_PAD_TOP, || {
        if let Some([l, t, r, b]) = node.padding {
            view_ops::set_padding(h, l, t, r, b);
        }
        apply_layout(h, node, kind);
        if let Some(w) = node.min_width.filter(|w| *w != 0) {
            view_ops::set_min_size(h, w, -1);
        }
        if let Some(hgt) = node.min_height.filter(|hgt| *hgt != 0) {
            view_ops::set_min_size(h, -1, hgt);
        }
        if let Some(argb) = node.background {
            view_ops::set_bg_color(h, argb);
        }
        if let Some(v) = node.visibility.filter(|v| *v != 0) {
            view_ops::set_visibility(
                h,
                if v == 4 {
                    Visibility::Invisible
                } else {
                    Visibility::Gone
                },
            );
        }
        if node.enabled == Some(false) {
            view_ops::set_enabled(h, false);
        }
        // `setAlpha` leaves an INVISIBLE view's opacity alone (that is 0).
        if let Some(a) = node
            .alpha
            .filter(|a| *a != 1.0 && node.visibility != Some(4))
        {
            view_ops::set_alpha(h, (a * 255.0) as u8);
        }
        // A Button is focusable from its constructor; an attribute decides
        // for every class.
        let focusable = node.focusable.unwrap_or(node.cls == class::BUTTON);
        if node.focusable.is_some() || node.cls == class::BUTTON {
            events::set_view_focusable(handle, focusable);
        }
        applied.focusable = focusable;
        if node.keep_screen_on == Some(true) {
            keep_on::set(handle, true);
        }
        if text_class {
            applied.line_mode = apply_text(handle, node, text);
        } else if scan::is_linear_class(node.cls) {
            if let Some(o) = node.orientation {
                widgets::linear_layout::set_orientation(handle, o);
            }
            if let Some(g) = node.gravity {
                widgets::linear_layout::set_gravity(handle, g);
            }
        }
    });
    applied
}

/// The `LayoutParams` half of `ViewGroup.addView`, by the parent's kind.
fn apply_layout(h: Handle, node: &Node, kind: ParentKind) {
    if matches!(
        kind,
        ParentKind::None | ParentKind::Custom | ParentKind::Grid
    ) {
        return;
    }
    view_ops::set_size(h, node.width, node.height);
    let margined = node.margin.iter().any(|m| *m != 0);
    match kind {
        ParentKind::Linear => {
            let grow = scan::flex_grow(node.weight_bits);
            if grow > 0 {
                view_ops::set_flex_grow(h, grow);
            }
            if margined {
                let [l, t, r, b] = node.margin;
                view_ops::set_margins(h, l, t, r, b);
            }
        }
        ParentKind::Frame => {
            if node.layout_gravity != 0 || margined {
                let (dx, dy) = scan::frame_offset(node.layout_gravity, node.margin);
                view_ops::set_frame_gravity(h, node.layout_gravity, dx, dy);
            }
        }
        _ => {
            if margined {
                let [l, t, r, b] = node.margin;
                view_ops::set_margins(h, l, t, r, b);
            }
        }
    }
}

/// `TextView`'s setters in the order `apply` would reach them, and the
/// packed `mLineMode` they leave behind. The size comes first, as the font
/// padding trim and the line cap are measured from the face.
fn apply_text(handle: i32, node: &Node, text: &str) -> i32 {
    const DEFAULT_TEXT_SIZE: f32 = 14.0;
    if node.cls == class::TEXT_VIEW && node.text.is_some() {
        widgets::text_view::set_text(handle, text);
    }
    if let Some(argb) = node.text_color {
        widgets::text_view::set_text_color(handle, argb);
    }
    if let Some(px) = node.text_size.filter(|px| *px != DEFAULT_TEXT_SIZE) {
        widgets::text_view::set_text_size(handle, px);
    }
    let font_pad_off = node.include_font_padding == Some(false);
    if let Some(include) = node.include_font_padding {
        widgets::text_view::set_include_font_padding(handle, include);
    }
    let ellipsize = node.ellipsize.unwrap_or(0);
    let single = node.single_line.unwrap_or(false);
    let max_lines = node.max_lines.map_or(0, |l| l.max(0));
    if node.ellipsize.is_some() || node.single_line.is_some() || node.max_lines.is_some() {
        widgets::text_view::set_line_mode(handle, ellipsize, max_lines, single);
    }
    if let Some(g) = node.gravity {
        widgets::text_view::set_gravity(handle, scan::text_gravity(g));
    }
    if let Some(w) = node.max_width.filter(|w| *w != i32::MAX) {
        view_ops::set_max_width(Handle::from_java(handle), w);
    }
    scan::pack_line_mode(
        ellipsize,
        single,
        max_lines,
        font_pad_off,
        node.text_color.is_some(),
    )
}
