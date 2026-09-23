// SPDX-License-Identifier: GPL-3.0-only
/*
 * The faces this build compiled, for TextView.setTextSize. See pd_fonts.c.
 */
#ifndef PD_FONTS_H
#define PD_FONTS_H

#include <stddef.h>
#include <stdint.h>

#include "lvgl.h"

/* One compiled face and its pixel size. The pointer comes first so the
 * layout is the same on ARM32 and x86_64; `pd_font_t` in src/lib.rs mirrors it. */
typedef struct {
    const lv_font_t *font;
    uint8_t px;
} pd_font_t;

/* The table, ascending by size; `*count` receives the row count. Never
 * empty: Montserrat 14 (LV_FONT_DEFAULT) is always the first row. */
const pd_font_t *pd_font_table(size_t *count);

/* The blank rows between a face's line top and the top of its digits: what
 * TextView.setIncludeFontPadding(false) trims. Measured on the face's '0',
 * so it is exact whatever the size (3 for Montserrat 14). 0 for no font. */
int pd_font_top_leading(const lv_font_t *font);

#endif
