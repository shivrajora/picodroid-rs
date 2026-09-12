// SPDX-License-Identifier: GPL-3.0-only
/*
 * Hardware vertical scroll: the two questions graphics/lvgl/hw_scroll.rs has
 * to ask through LVGL's private headers. Compiled in by build_support/lvgl.rs
 * for a board whose panel can rotate a band of its own frame memory
 * (board_cfg::hw_vscroll — a portrait ST7796 — and its simulator, which
 * emulates the registers).
 *
 * 1. picodroid_hw_vscroll_region: may this scroller's visible rows be moved
 *    by the panel, and if so which rows, and what static things are drawn on
 *    top of them that need repainting after every step? The style getters
 *    that answer it are header-only inlines, and the walk needs obj->coords.
 *
 * 2. picodroid_hw_vscroll_shift_pending: once the panel has moved a band's
 *    content by dy rows, every area already queued for redraw inside that
 *    band describes pixels that moved with it. The queue is
 *    lv_display_t::inv_areas, which has no public accessor.
 *
 * The rule the panel imposes, and what the eligibility check enforces: every
 * pixel inside the band that does not move with the scroller's children must
 * look the same on every row, or the rotation shows. A flat fill, a
 * horizontal gradient and a side border pass; a top or bottom border line, a
 * rounded corner, an image and a vertical gradient do not. Objects drawn on
 * top of the band — a scrollbar, a toast, an FPS label, a floating child —
 * are reported back so Rust repaints them after each step, up to a limit past
 * which repainting the band is cheaper.
 */
#if PICODROID_HW_VSCROLL

#include "lvgl.h"
#include "core/lv_obj_private.h"
#include "display/lv_display_private.h"
#include "misc/lv_area_private.h"

/* Why a scroller was refused, for the once-only log on the Rust side. */
#define REFUSED_SHAPE      (-1) /* hidden, not scrollable, not full width, off the active screen */
#define REFUSED_DECORATION (-2) /* a border line, corner, image or gradient would move */
#define REFUSED_OVERLAYS   (-3) /* too many, or too much, drawn on top of the band */
#define REFUSED_TRANSFORM  (-4) /* scaled or rotated: rendered through a layer */

static bool rows_intersect(int32_t a1, int32_t a2, int32_t b1, int32_t b2)
{
    return a1 <= b2 && b1 <= a2;
}

static bool is_transformed(const lv_obj_t *obj)
{
    return lv_obj_get_style_transform_scale_x(obj, LV_PART_MAIN) != LV_SCALE_NONE ||
           lv_obj_get_style_transform_scale_y(obj, LV_PART_MAIN) != LV_SCALE_NONE ||
           lv_obj_get_style_transform_rotation(obj, LV_PART_MAIN) != 0;
}

static bool border_visible(const lv_obj_t *obj)
{
    return lv_obj_get_style_border_width(obj, LV_PART_MAIN) > 0 &&
           lv_obj_get_style_border_opa(obj, LV_PART_MAIN) > LV_OPA_MIN;
}

/* The fill of `obj` looks the same on every row of any band it covers. */
static bool fill_is_row_invariant(const lv_obj_t *obj)
{
    if(lv_obj_get_style_bg_opa(obj, LV_PART_MAIN) <= LV_OPA_MIN) return true;
    if(lv_obj_get_style_bg_image_src(obj, LV_PART_MAIN) != NULL) return false;
    lv_grad_dir_t dir = lv_obj_get_style_bg_grad_dir(obj, LV_PART_MAIN);
    return dir == LV_GRAD_DIR_NONE || dir == LV_GRAD_DIR_HOR;
}

/* A horizontal border line or a rounded corner of `obj` lands inside the band. */
static bool decoration_crosses(const lv_obj_t *obj, const lv_area_t *band)
{
    const lv_area_t *c = &obj->coords;
    if(border_visible(obj)) {
        lv_border_side_t side = lv_obj_get_style_border_side(obj, LV_PART_MAIN);
        int32_t bw = lv_obj_get_style_border_width(obj, LV_PART_MAIN);
        if((side & LV_BORDER_SIDE_TOP) && rows_intersect(c->y1, c->y1 + bw - 1, band->y1, band->y2)) return true;
        if((side & LV_BORDER_SIDE_BOTTOM) && rows_intersect(c->y2 - bw + 1, c->y2, band->y1, band->y2)) return true;
    }
    int32_t r = lv_obj_get_style_radius(obj, LV_PART_MAIN);
    bool painted = border_visible(obj) || lv_obj_get_style_bg_opa(obj, LV_PART_MAIN) > LV_OPA_MIN;
    if(r > 0 && painted) {
        /* LV_RADIUS_CIRCLE is a large sentinel; the sums stay well inside int32. */
        int32_t half = (lv_area_get_height(c) + 1) / 2;
        if(r > half) r = half;
        if(rows_intersect(c->y1, c->y1 + r - 1, band->y1, band->y2)) return true;
        if(rows_intersect(c->y2 - r + 1, c->y2, band->y1, band->y2)) return true;
    }
    return false;
}

/* Record `obj`'s footprint inside the band as an overlay. Returns the new
 * count, or REFUSED_OVERLAYS when the table is full. Hidden objects and ones
 * outside the band cost nothing. */
static int32_t add_overlay(const lv_obj_t *obj, const lv_area_t *band, lv_area_t *overlays, int32_t max,
                           int32_t n)
{
    if(n < 0) return n;
    if(lv_obj_has_flag(obj, LV_OBJ_FLAG_HIDDEN)) return n;
    lv_area_t a = obj->coords;
    int32_t ext = lv_obj_get_ext_draw_size(obj);
    lv_area_increase(&a, ext, ext);
    if(!lv_area_intersect(&a, &a, band)) return n;
    if(n >= max) return REFUSED_OVERLAYS;
    overlays[n] = a;
    return n + 1;
}

int32_t picodroid_hw_vscroll_region(lv_obj_t *scroller, lv_area_t *region, lv_area_t *overlays,
                                    int32_t max_overlays)
{
    if(lv_obj_has_flag(scroller, LV_OBJ_FLAG_HIDDEN)) return REFUSED_SHAPE;
    if(!lv_obj_has_flag(scroller, LV_OBJ_FLAG_SCROLLABLE)) return REFUSED_SHAPE;
    if(lv_obj_has_flag(scroller, LV_OBJ_FLAG_OVERFLOW_VISIBLE)) return REFUSED_SHAPE;

    lv_display_t *disp = lv_obj_get_display(scroller);
    if(disp == NULL) return REFUSED_SHAPE;
    /* Mid screen transition both screens paint; leave that to the renderer. */
    if(lv_display_get_screen_prev(disp) != NULL) return REFUSED_SHAPE;
    if(lv_obj_get_screen(scroller) != lv_display_get_screen_active(disp)) return REFUSED_SHAPE;
    if(is_transformed(scroller)) return REFUSED_TRANSFORM;

    /* The rows the panel would rotate: the scroller clipped by every ancestor
     * and the screen, exactly as the renderer clips its children. */
    lv_area_t band = scroller->coords;
    for(lv_obj_t *p = lv_obj_get_parent(scroller); p != NULL; p = lv_obj_get_parent(p)) {
        if(lv_obj_has_flag(p, LV_OBJ_FLAG_HIDDEN)) return REFUSED_SHAPE;
        if(lv_obj_has_flag(p, LV_OBJ_FLAG_OVERFLOW_VISIBLE)) return REFUSED_SHAPE;
        if(is_transformed(p)) return REFUSED_TRANSFORM;
        if(!lv_area_intersect(&band, &band, &p->coords)) return REFUSED_SHAPE;
    }
    lv_area_t screen;
    lv_area_set(&screen, 0, 0, lv_display_get_horizontal_resolution(disp) - 1,
                lv_display_get_vertical_resolution(disp) - 1);
    if(!lv_area_intersect(&band, &band, &screen)) return REFUSED_SHAPE;
    /* The panel rotates whole lines, so the band must be the whole width. */
    if(band.x1 != screen.x1 || band.x2 != screen.x2) return REFUSED_SHAPE;
    if(lv_area_get_height(&band) < 2) return REFUSED_SHAPE;

    /* The scroller's own paint, and what shows through it. */
    if(!fill_is_row_invariant(scroller)) return REFUSED_DECORATION;
    if(decoration_crosses(scroller, &band)) return REFUSED_DECORATION;
    bool sees_through = lv_obj_get_style_bg_opa(scroller, LV_PART_MAIN) < LV_OPA_COVER ||
                        lv_obj_get_style_opa(scroller, LV_PART_MAIN) < LV_OPA_COVER;

    int32_t n = 0;
    int32_t overlay_rows = 0;

    /* Floating children stay put while their siblings scroll. */
    uint32_t child_cnt = lv_obj_get_child_count(scroller);
    for(uint32_t i = 0; i < child_cnt; i++) {
        lv_obj_t *child = lv_obj_get_child(scroller, (int32_t)i);
        if(lv_obj_has_flag(child, LV_OBJ_FLAG_FLOATING)) n = add_overlay(child, &band, overlays, max_overlays, n);
    }

    /* Up the tree: later siblings are drawn over the band and stay put;
     * earlier ones show through a transparent scroller; an ancestor's fill
     * shows through until one is opaque. */
    lv_obj_t *c = scroller;
    for(lv_obj_t *p = lv_obj_get_parent(c); p != NULL; c = p, p = lv_obj_get_parent(p)) {
        int32_t idx = lv_obj_get_index(c);
        uint32_t cnt = lv_obj_get_child_count(p);
        for(uint32_t i = (uint32_t)(idx + 1); i < cnt; i++) {
            n = add_overlay(lv_obj_get_child(p, (int32_t)i), &band, overlays, max_overlays, n);
        }
        if(sees_through) {
            for(int32_t i = 0; i < idx; i++) {
                lv_obj_t *sib = lv_obj_get_child(p, i);
                if(lv_obj_has_flag(sib, LV_OBJ_FLAG_HIDDEN)) continue;
                lv_area_t a = sib->coords;
                if(lv_area_intersect(&a, &a, &band)) return REFUSED_DECORATION;
            }
            if(!fill_is_row_invariant(p)) return REFUSED_DECORATION;
            if(decoration_crosses(p, &band)) return REFUSED_DECORATION;
            if(lv_obj_get_style_bg_opa(p, LV_PART_MAIN) >= LV_OPA_COVER) sees_through = false;
        }
    }

    /* The layers above every screen: dialogs, toasts, the cursor. */
    lv_obj_t *layers[2] = { lv_display_get_layer_top(disp), lv_display_get_layer_sys(disp) };
    for(size_t l = 0; l < 2; l++) {
        if(layers[l] == NULL) continue;
        uint32_t cnt = lv_obj_get_child_count(layers[l]);
        for(uint32_t i = 0; i < cnt; i++) {
            n = add_overlay(lv_obj_get_child(layers[l], (int32_t)i), &band, overlays, max_overlays, n);
        }
    }
    if(n < 0) return n;

    /* Past this much cover, repainting the overlays costs what the band does. */
    for(int32_t i = 0; i < n; i++) overlay_rows += lv_area_get_height(&overlays[i]);
    if(overlay_rows * 2 > lv_area_get_height(&band)) return REFUSED_OVERLAYS;

    *region = band;
    return n;
}

int32_t picodroid_hw_vscroll_thumb_flat_radius(lv_obj_t *obj)
{
    if(lv_obj_get_style_bg_opa(obj, LV_PART_SCROLLBAR) < LV_OPA_COVER) return -1;
    if(lv_obj_get_style_opa(obj, LV_PART_SCROLLBAR) < LV_OPA_COVER) return -1;
    if(lv_obj_get_style_bg_grad_dir(obj, LV_PART_SCROLLBAR) != LV_GRAD_DIR_NONE) return -1;
    if(lv_obj_get_style_bg_image_src(obj, LV_PART_SCROLLBAR) != NULL) return -1;
    if(lv_obj_get_style_border_width(obj, LV_PART_SCROLLBAR) > 0 &&
       lv_obj_get_style_border_opa(obj, LV_PART_SCROLLBAR) > LV_OPA_MIN) return -1;
    if(lv_obj_get_style_shadow_width(obj, LV_PART_SCROLLBAR) > 0 &&
       lv_obj_get_style_shadow_opa(obj, LV_PART_SCROLLBAR) > LV_OPA_MIN) return -1;
    int32_t r = lv_obj_get_style_radius(obj, LV_PART_SCROLLBAR);
    return r < 0 ? 0 : r;
}

void picodroid_hw_vscroll_shift_pending(lv_display_t *disp, const lv_area_t *region, int32_t dy)
{
    for(uint32_t i = 0; i < disp->inv_p; i++) {
        lv_area_t *a = &disp->inv_areas[i];
        lv_area_t moved;
        if(!lv_area_intersect(&moved, a, region)) continue;
        moved.y1 += dy;
        moved.y2 += dy;
        if(!lv_area_intersect(&moved, &moved, region)) continue;
        /* The bounding box of where the area was and where its pixels went:
         * a few rows over-repainted rather than any left stale. */
        lv_area_join(a, a, &moved);
    }
}

#endif /* PICODROID_HW_VSCROLL */
