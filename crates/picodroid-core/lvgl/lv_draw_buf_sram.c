// SPDX-License-Identifier: GPL-3.0-only
/*
 * Draw buffers from SRAM for a board whose LVGL pool lives in PSRAM
 * (board.toml `lv_mem_in_psram`; compiled in by build_support/lvgl.rs next
 * to -DLV_MEM_ADR).
 *
 * The pool is a fine home for what LVGL keeps between frames — objects,
 * styles, draw tasks — because the renderer reads those while walking the
 * tree, not per pixel. A layer buffer is the opposite: rendered into per
 * pixel and read back to composite, and through the QSPI bus that costs
 * more than the frame saves. So `lv_draw_buf_create`'s allocation callback
 * is swapped for one that serves from SRAM — the FreeRTOS arena, which the
 * JVM also draws on; layers are transient, allocated and freed inside one
 * refresh, so at rest they cost the arena nothing — and falls back to the
 * pool only when the arena is full. The free callback tells the two apart
 * by address: SRAM is 0x2xxxxxxx, the PSRAM window 0x1xxxxxxx.
 *
 * Installed once, right after lv_init, by graphics/lvgl/lifecycle.rs. The
 * font handlers get the same treatment (unused by the built-in bitmap fonts
 * in this LVGL, harmless to cover); the image-cache handlers stay on the
 * pool, where a decoded image is read once per row and lives for as long as
 * the widget does.
 */
#if PICODROID_LV_MEM_IN_PSRAM

#include "lvgl.h"
#include "draw/lv_draw_buf_private.h" /* via -I third_party/lvgl/src */
#include <stdint.h>
#include <stddef.h>

extern void *pvPortMalloc(size_t xWantedSize);
extern void vPortFree(void *pv);

static uint32_t pool_fallbacks;

static void *sram_buf_malloc(size_t size, lv_color_format_t color_format)
{
    LV_UNUSED(color_format);
    void *buf = pvPortMalloc(size);
    if(buf == NULL) {
        pool_fallbacks++;
        buf = lv_malloc(size);
    }
    return buf;
}

static void sram_buf_free(void *buf)
{
    if(((uintptr_t)buf >> 28) == 0x2u) vPortFree(buf);
    else lv_free(buf);
}

void picodroid_lv_draw_buf_use_sram(void)
{
    lv_draw_buf_handlers_t *handlers[] = {
        lv_draw_buf_get_handlers(),
        lv_draw_buf_get_font_handlers(),
    };
    for(size_t i = 0; i < sizeof(handlers) / sizeof(handlers[0]); i++) {
        handlers[i]->buf_malloc_cb = sram_buf_malloc;
        handlers[i]->buf_free_cb = sram_buf_free;
    }
}

/* How many draw buffers the arena could not serve, for a once-only warning. */
uint32_t picodroid_lv_draw_buf_pool_fallbacks(void)
{
    return pool_fallbacks;
}

#endif /* PICODROID_LV_MEM_IN_PSRAM */
