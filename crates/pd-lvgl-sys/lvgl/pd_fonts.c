// SPDX-License-Identifier: GPL-3.0-only
/*
 * The faces TextView.setTextSize can snap to: one table, assembled by the
 * preprocessor from what build_support/lvgl.rs switched on for this board
 * (its `text_sizes` key). The C build is thus the only place that knows
 * which faces exist, and the Rust side asks at runtime
 * (graphics/lvgl/widgets/text_view.rs::set_text_size) instead of keeping a
 * list of its own that could drift.
 *
 * Montserrat 14 is LVGL's stock face and LV_FONT_DEFAULT, always present.
 * Every other row is an ASCII-only face scripts/gen-fonts.sh generated into
 * fonts/, compiled in only when its size is listed, so a board pays for
 * nothing it does not use. Adding a size: generate the face, add its row
 * here in ascending order, list it in the MCU or board toml. A pd-lvgl-sys
 * test holds the rows here to the files in fonts/.
 */
#include "pd_fonts.h"

#if PD_FONT_MONTSERRAT_20
LV_FONT_DECLARE(pd_font_montserrat_20)
#define PD_ROW_20 { &pd_font_montserrat_20, 20 },
#else
#define PD_ROW_20
#endif

#if PD_FONT_MONTSERRAT_28
LV_FONT_DECLARE(pd_font_montserrat_28)
#define PD_ROW_28 { &pd_font_montserrat_28, 28 },
#else
#define PD_ROW_28
#endif

#if PD_FONT_MONTSERRAT_64
LV_FONT_DECLARE(pd_font_montserrat_64)
#define PD_ROW_64 { &pd_font_montserrat_64, 64 },
#else
#define PD_ROW_64
#endif

static const pd_font_t PD_FONTS[] = {
    { &lv_font_montserrat_14, 14 },
    PD_ROW_20
    PD_ROW_28
    PD_ROW_64
};

const pd_font_t *pd_font_table(size_t *count)
{
    *count = sizeof(PD_FONTS) / sizeof(PD_FONTS[0]);
    return PD_FONTS;
}

int pd_font_top_leading(const lv_font_t *font)
{
    lv_font_glyph_dsc_t g;
    if (font == NULL || !lv_font_get_glyph_dsc(font, &g, '0', 0)) {
        return 0;
    }
    /* Line top to glyph top: the line minus the descent below the baseline,
     * the glyph's offset above it, and the glyph itself. */
    int leading = (int)lv_font_get_line_height(font) - (int)font->base_line
                  - (int)g.ofs_y - (int)g.box_h;
    return leading > 0 ? leading : 0;
}
