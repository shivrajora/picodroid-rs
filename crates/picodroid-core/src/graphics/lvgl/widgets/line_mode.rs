// SPDX-License-Identifier: GPL-3.0-only
//! The `TextView` line mode — `setSingleLine` / `setEllipsize` / `setMaxLines` — as LVGL sees
//! it: a label long mode and a cap on the lines the box may hold. Pure, so it is unit-tested
//! through the shim in `lib.rs` although the graphics tree is `cfg(not(test))`; `text_view.rs`
//! applies the result.

use crate::lvgl_ffi::*;

/// `TextUtils.TruncateAt.MARQUEE.ordinal() + 1`, as `TextView.setEllipsize` packs it.
const MARQUEE: i32 = 4;

/// The LVGL long mode and the line cap (0 = none) for the Java state: `kind` is
/// `TruncateAt.ordinal() + 1`, or 0 for no ellipsize; `max_lines` is 0 for no limit.
///
/// `DOTS` puts LVGL's three dots on the last line that fits the box, which is why a single-line
/// or max-lines label needs the cap. `CLIP` and the scroll modes never wrap (LVGL sets the
/// label's `expand`), so a single line without an ellipsis is a clip. Android's `START` and
/// `MIDDLE` have no LVGL counterpart and render as `END`.
pub(crate) fn line_mode(kind: i32, max_lines: i32, single: bool) -> (lv_label_long_mode_t, i32) {
    let cap = if single {
        1
    } else if max_lines > 0 {
        max_lines
    } else {
        0
    };
    let mode = match kind {
        0 if single => LV_LABEL_LONG_MODE_CLIP,
        0 => LV_LABEL_LONG_MODE_WRAP,
        MARQUEE => LV_LABEL_LONG_MODE_SCROLL_CIRCULAR,
        _ => LV_LABEL_LONG_MODE_DOTS,
    };
    (mode, cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_wraps_without_a_cap() {
        assert_eq!(line_mode(0, 0, false), (LV_LABEL_LONG_MODE_WRAP, 0));
    }

    #[test]
    fn single_line_clips_and_holds_one_line() {
        assert_eq!(line_mode(0, 0, true), (LV_LABEL_LONG_MODE_CLIP, 1));
        // setSingleLine wins over a max-lines limit, as on Android.
        assert_eq!(line_mode(0, 3, true), (LV_LABEL_LONG_MODE_CLIP, 1));
    }

    #[test]
    fn max_lines_wraps_under_a_cap() {
        assert_eq!(line_mode(0, 3, false), (LV_LABEL_LONG_MODE_WRAP, 3));
    }

    #[test]
    fn start_middle_end_all_render_as_dots() {
        for kind in 1..=3 {
            assert_eq!(line_mode(kind, 0, true), (LV_LABEL_LONG_MODE_DOTS, 1));
            assert_eq!(line_mode(kind, 2, false), (LV_LABEL_LONG_MODE_DOTS, 2));
            // No limit and no single line: the dots only show on a fixed-height label.
            assert_eq!(line_mode(kind, 0, false), (LV_LABEL_LONG_MODE_DOTS, 0));
        }
    }

    #[test]
    fn marquee_scrolls_circularly() {
        assert_eq!(
            line_mode(MARQUEE, 0, true),
            (LV_LABEL_LONG_MODE_SCROLL_CIRCULAR, 1)
        );
        assert_eq!(
            line_mode(MARQUEE, 0, false),
            (LV_LABEL_LONG_MODE_SCROLL_CIRCULAR, 0)
        );
    }
}
