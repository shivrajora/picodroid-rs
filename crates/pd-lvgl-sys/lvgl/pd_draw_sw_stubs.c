// SPDX-License-Identifier: GPL-3.0-only
/*
 * Stands in for two software-renderer files build_support (lvgl.rs) leaves
 * out of the compile: third_party/lvgl/src/draw/sw/lv_draw_sw_box_shadow.c
 * and lv_draw_sw_blur.c. Both sit under switches that cannot go off on their
 * own (LV_DRAW_SW_COMPLEX also guards arcs, rounded borders and masked
 * images; blur has no switch at all in 9.6) and both are reached only from
 * the task dispatch in lv_draw_sw.c, which names them, so they must link.
 *
 * Neither task is ever created here. A box-shadow task needs a style with
 * shadow_width != 0 and shadow_opa > LV_OPA_MIN (lv_draw_rect.c): the
 * default theme sets one only in light mode (lv_theme_default.c, under
 * `!(flags & MODE_DARK)`) and lv_conf.h builds the dark theme; the LED
 * widget, the other shadow user, is off; and picodroid binds no
 * lv_obj_set_style_shadow_* / lv_style_set_shadow_*, so no app can ask. A
 * blur task needs blur_radius > 0 (lv_draw_blur.c) and nothing sets one.
 * pd-lvgl-sys's tests pin all of that (dark theme on, no shadow or blur
 * setter bound). 4.6 KB on the RP2350 at -Os for the two files; measured
 * on the pico_display2_w debug image, docs/designs/flash-budget-2026-09.md
 * §6.5 (2026-10-14 amendment).
 *
 * Like pd_blend_to_rgb888_stub.c, these stop hard if ever reached: a shadow
 * or blur task means a setter was bound without the stub going away, and
 * drawing nothing is the failure this file's reason for being fixes.
 */

#include "lvgl.h"
#include "draw/sw/lv_draw_sw.h"

#if LV_USE_DRAW_SW

#if LV_DRAW_SW_COMPLEX
void lv_draw_sw_box_shadow(lv_draw_task_t * t, const lv_draw_box_shadow_dsc_t * dsc, const lv_area_t * coords)
{
    LV_UNUSED(t);
    LV_UNUSED(dsc);
    LV_UNUSED(coords);
    LV_ASSERT_MSG(0, "box shadow task: picodroid never creates one (pd_draw_sw_stubs.c)");
}
#endif

void lv_draw_sw_blur(lv_draw_task_t * t, const lv_draw_blur_dsc_t * dsc, const lv_area_t * coords)
{
    LV_UNUSED(t);
    LV_UNUSED(dsc);
    LV_UNUSED(coords);
    LV_ASSERT_MSG(0, "blur task: picodroid never creates one (pd_draw_sw_stubs.c)");
}

#endif /* LV_USE_DRAW_SW */
