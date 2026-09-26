// SPDX-License-Identifier: GPL-3.0-only
/*
 * Stands in for third_party/lvgl/src/draw/sw/blend/lv_draw_sw_blend_to_rgb888.c,
 * which build_support (lvgl.rs) leaves out of the compile.
 *
 * LV_DRAW_SW_SUPPORT_RGB888 is on for the *source* side only: the software
 * renderer blends a horizontal gradient as an RGB888 source image
 * (lv_draw_sw_fill.c and lv_draw_sw_triangle.c set src_color_format =
 * LV_COLOR_FORMAT_RGB888 and hand over the lv_color_t gradient map), so the
 * RGB888 arm of the RGB565_SWAPPED and ARGB8888 blenders has to exist or a
 * LEFT_RIGHT GradientDrawable draws nothing. The same switch links the RGB888
 * *destination* blender -- 2.8 KB on the RP2040 -- and nothing here renders
 * into RGB888: the display is RGB565_SWAPPED and every lv_draw_layer_create
 * call site in third_party/lvgl asks for ARGB8888, A8 or NATIVE. The
 * dispatcher in lv_draw_sw_blend.c still names both entry points, so they are
 * defined here and stop hard if ever reached: a layer in that format is a bug,
 * and drawing nothing is the failure this file's reason for being fixes.
 */

#include "lvgl.h"
#include "draw/sw/blend/lv_draw_sw_blend_to_rgb888.h"

#if LV_USE_DRAW_SW && (LV_DRAW_SW_SUPPORT_RGB888 || LV_DRAW_SW_SUPPORT_XRGB8888)

void lv_draw_sw_blend_color_to_rgb888(lv_draw_sw_blend_fill_dsc_t * dsc, uint32_t dest_px_size)
{
    LV_UNUSED(dsc);
    LV_UNUSED(dest_px_size);
    LV_ASSERT_MSG(0, "RGB888 render target: picodroid never creates one (pd_blend_to_rgb888_stub.c)");
}

void lv_draw_sw_blend_image_to_rgb888(lv_draw_sw_blend_image_dsc_t * dsc, uint32_t dest_px_size)
{
    LV_UNUSED(dsc);
    LV_UNUSED(dest_px_size);
    LV_ASSERT_MSG(0, "RGB888 render target: picodroid never creates one (pd_blend_to_rgb888_stub.c)");
}

#endif /* LV_USE_DRAW_SW && (RGB888 || XRGB8888) */
