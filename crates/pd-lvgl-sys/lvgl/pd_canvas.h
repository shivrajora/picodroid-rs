// SPDX-License-Identifier: GPL-3.0-only
/*
 * The retained display list behind picodroid.graphics.Canvas. See pd_canvas.c.
 *
 * Every entry point takes plain integers so the Rust side (src/lib.rs) never
 * mirrors an LVGL draw descriptor. Coordinates are view-local pixels, LVGL
 * style: a rectangle's x2/y2 are inclusive. Colours are 0xAARRGGBB; the alpha
 * byte becomes the op's opacity.
 */
#ifndef PD_CANVAS_H
#define PD_CANVAS_H

#include <stddef.h>
#include <stdint.h>

#include "lvgl.h"

/* Bytes one view's list may hold: 64 ops of 32 bytes, a little less with text.
 * Past it an op is dropped and counted. The list shares LVGL's pool (48 KB on
 * most boards) with every widget, and LVGL's malloc assert spins forever when
 * that pool runs dry, so the cap is deliberately small. */
#define PD_CANVAS_MAX_BYTES 2048

/* Give `obj` a display list and the draw and delete hooks that serve it.
 * Call exactly once per object: LVGL does not dedupe event callbacks.
 * 0 on success, -1 when the list header could not be allocated (the object
 * then draws nothing but is otherwise a normal view). */
int pd_canvas_attach(lv_obj_t *obj);

/* Empty the list before a new onDraw. */
void pd_canvas_begin(lv_obj_t *obj);

/* Close the list and schedule a redraw. Returns how many ops this pass
 * dropped for want of room (0 normally). */
int pd_canvas_end(lv_obj_t *obj);

/* The ops. Each returns 0, or -1 when the op was dropped (no list, or the
 * list is full). */
int pd_canvas_fill(lv_obj_t *obj, uint32_t argb);
int pd_canvas_rect(lv_obj_t *obj, int32_t x1, int32_t y1, int32_t x2, int32_t y2, int32_t radius,
                   uint32_t fill_argb, uint32_t stroke_argb, int32_t stroke_width);
int pd_canvas_line(lv_obj_t *obj, int32_t x1, int32_t y1, int32_t x2, int32_t y2, int32_t width,
                   uint32_t argb, int round_caps);
int pd_canvas_arc(lv_obj_t *obj, int32_t cx, int32_t cy, int32_t radius, int32_t start_deg,
                  int32_t end_deg, int32_t width, uint32_t argb, int rounded);
/* `x` is where the alignment anchors (left edge, centre or right edge, per
 * `align`: LV_TEXT_ALIGN_LEFT / CENTER / RIGHT) and `baseline` the text's
 * baseline, as Android's Canvas.drawText takes them; `font_index` is a row of
 * pd_font_table (pd_fonts.h). `utf8` need not be NUL-terminated; it is
 * copied. */
int pd_canvas_text(lv_obj_t *obj, int32_t x, int32_t baseline, int32_t font_index,
                   uint32_t argb, int align, const char *utf8, size_t len);

/* Paint.ascent() / descent() for the face at `font_index`: the line's top
 * above the baseline (negative) and its bottom below it, which is where
 * pd_canvas_text puts a line. 0 for a missing face. */
int32_t pd_canvas_ascent(int32_t font_index);
int32_t pd_canvas_descent(int32_t font_index);

/* Paint.measureText(): the width of one line of `utf8` in that face. */
int32_t pd_canvas_text_width(int32_t font_index, const char *utf8, size_t len);

#endif
