/**
 * LVGL configuration for picodroid.
 * Based on lv_conf_template.h for LVGL v9.2.2, tracked forward to v9.6.0.
 *
 * Board-specific values (LV_DPI_DEF, LV_MEM_SIZE) may be overridden
 * by -D flags injected from build.rs; #ifndef guards preserve defaults.
 */

#ifndef LV_CONF_H
#define LV_CONF_H

/*====================
   COLOR SETTINGS
 *====================*/
/* The panels (ST7789, ST7796) take RGB565 big-endian over SPI, so LVGL renders
 * straight into the byte-swapped format and flush_cb hands the band on as is.
 *
 * This used to be LV_COLOR_DEPTH 16 + LV_COLOR_16_SWAP 1: render little-endian,
 * then lv_refr.c byte-swapped every band in place before flush_cb. v9.6.0
 * deprecated that and v10 removes it. The bytes flush_cb receives are the same
 * either way -- the fbhash sequences of graphicsbench are identical before and
 * after on three boards (docs/designs/rgb565-swapped-render-2026-09.md §7).
 * This is not a speed change worth mentioning: the old swap measured ~3 ms of a
 * ~109 ms frame, and RGB565 images now pay a per-pixel swap as they blend. */
#define LV_COLOR_FORMAT_DEFAULT LV_COLOR_FORMAT_RGB565_SWAPPED

/*=========================
   STDLIB WRAPPER SETTINGS
 *=========================*/

/*
 * Use LVGL's built-in allocator backed by a static pool.
 * The pool calls our custom alloc/free (FreeRTOS pvPortMalloc/vPortFree
 * on hardware, standard malloc/free in sim).
 */
#define LV_USE_STDLIB_MALLOC    LV_STDLIB_BUILTIN
#define LV_USE_STDLIB_STRING    LV_STDLIB_BUILTIN
#define LV_USE_STDLIB_SPRINTF   LV_STDLIB_BUILTIN

#define LV_STDINT_INCLUDE       <stdint.h>
#define LV_STDDEF_INCLUDE       <stddef.h>
#define LV_STDBOOL_INCLUDE      <stdbool.h>
#define LV_INTTYPES_INCLUDE     <inttypes.h>
#define LV_LIMITS_INCLUDE       <limits.h>
#define LV_STDARG_INCLUDE       <stdarg.h>

/* Built-in memory pool — default 64 KB for RP2350's 520 KB SRAM.
 * Override via build.rs -D for boards with different RAM budgets.
 *
 * Where the pool lives is the build's call too: a board with
 * `lv_mem_in_psram` gets -DLV_MEM_ADR=<psram_origin>, and
 * lv_mem_core_builtin.c then creates the TLSF pool at that address instead
 * of in a .bss array, with lv_draw_buf_sram.c (beside this file) keeping the
 * render targets in SRAM. Deliberately not defined here: lv_conf_internal.h
 * defaults it to 0, the .bss array. */
#ifndef LV_MEM_SIZE
#define LV_MEM_SIZE (64 * 1024U)
#endif

/*====================
   HAL SETTINGS
 *====================*/
/* LVGL's refresh period, in the same milliseconds `lv_tick_inc` is fed.
 * Must equal `executors::tick_source::TICK_PERIOD_MS`; a guard test in that
 * file enforces it.
 *
 * `lv_timer` stamps `last_run = lv_tick_get()` and carries no credit, so a
 * period that is not a whole multiple of the tick quantises up to the next one.
 * This was 33 against a 16 ms tick, so every paint waited three ticks instead
 * of one — measured, exactly three, never two or four. On the touch board that
 * showed up as 19-30 ms of idle per frame, and it becomes the dominant term the
 * moment the render gets cheaper
 * (docs/designs/scroll-performance-2026-09.md S3). */
#define LV_DEF_REFR_PERIOD  16
#ifndef LV_DPI_DEF
#define LV_DPI_DEF          130
#endif

/*=================
 * OPERATING SYSTEM
 *=================*/
#define LV_USE_OS   LV_OS_NONE

/*========================
 * RENDERING CONFIGURATION
 *========================*/
#define LV_DRAW_BUF_STRIDE_ALIGN      1
#define LV_DRAW_BUF_ALIGN             4
#define LV_DRAW_TRANSFORM_USE_MATRIX  0

/* Smaller layer buffer for memory-constrained target */
#define LV_DRAW_LAYER_SIMPLE_BUF_SIZE (8 * 1024)
#define LV_DRAW_THREAD_STACK_SIZE     (8 * 1024)

#define LV_USE_DRAW_SW 1
#if LV_USE_DRAW_SW == 1
    /* The render target (LV_COLOR_FORMAT_DEFAULT above). */
    #define LV_DRAW_SW_SUPPORT_RGB565_SWAPPED 1
    /* Nothing renders into plain RGB565 any more, but every papk image is RGB565,
     * and the swapped blender only takes an RGB565 source under this switch
     * (lv_draw_sw_blend_to_rgb565_swapped.c). It also links the RGB565 target
     * blender; the two cannot be split. RGB565A8 is the transform path's
     * intermediate (e.g. a scaled ImageView), blended as RGB565 + mask. */
    #define LV_DRAW_SW_SUPPORT_RGB565       1
    #define LV_DRAW_SW_SUPPORT_RGB565A8     1
    #define LV_DRAW_SW_SUPPORT_RGB888       0
    #define LV_DRAW_SW_SUPPORT_XRGB8888    0
    #define LV_DRAW_SW_SUPPORT_ARGB8888    1  /* needed internally for blending */
    /* Premultiplied ARGB8888 is unreachable here, and LVGL defaults it ON when
     * Kconfig is absent, so it has been linked all along without being asked
     * for. It can only be hit two ways, and both are closed: an asset in that
     * format (papk-pack bakes LV_COLOR_FORMAT_RGB565 into every image, with a
     * guard test in tools/papk-pack), or an intermediate layer requesting it
     * (every lv_draw_layer_create call site in third_party/lvgl passes ARGB8888,
     * A8 or NATIVE -- none passes PREMULTIPLIED). Worth 11,292 bytes across
     * blend_image_to_ and blend_color_to_. */
    #define LV_DRAW_SW_SUPPORT_ARGB8888_PREMULTIPLIED 0
    #define LV_DRAW_SW_SUPPORT_L8          0
    #define LV_DRAW_SW_SUPPORT_AL88        0
    #define LV_DRAW_SW_SUPPORT_A8          1  /* needed for font rendering */
    #define LV_DRAW_SW_SUPPORT_I1          0

    #define LV_DRAW_SW_DRAW_UNIT_CNT    1
    #define LV_USE_DRAW_ARM2D_SYNC      0
    #define LV_USE_NATIVE_HELIUM_ASM    0
    #define LV_DRAW_SW_COMPLEX          1

    #if LV_DRAW_SW_COMPLEX == 1
        #define LV_DRAW_SW_SHADOW_CACHE_SIZE 0
        #define LV_DRAW_SW_CIRCLE_CACHE_SIZE 4
    #endif

    #define LV_USE_DRAW_SW_ASM  LV_DRAW_SW_ASM_NONE
    #define LV_USE_DRAW_SW_COMPLEX_GRADIENTS 0
#endif

/* Disable GPU backends */
#define LV_USE_DRAW_VGLITE    0
/* v9.6.0 turned LV_USE_PXP into a deprecated no-op — LV_USE_DRAW_PXP is what
 * actually gates the backend. */
#define LV_USE_DRAW_PXP       0
#define LV_USE_DRAW_G2D       0
#define LV_USE_DRAW_DAVE2D    0
#define LV_USE_DRAW_SDL       0
#define LV_USE_DRAW_VG_LITE   0

/*=======================
 * FEATURE CONFIGURATION
 *=======================*/

/* Logging — disabled to save code size */
#define LV_USE_LOG 0

/* Asserts — keep null/malloc checks, disable expensive ones.
 *
 * v9.6.0 added LV_USE_ASSERT as a master switch, defaulting to 0. It gates
 * only the generic LV_ASSERT / LV_ASSERT_MSG that LVGL sprinkles through its
 * own code; LV_ASSERT_NULL and LV_ASSERT_MALLOC expand through
 * LV_ASSERT_INTERNAL, which is not gated, so the two checks we actually want
 * survive it. Left at the new default, stated here so it is a decision
 * rather than an accident. */
#define LV_USE_ASSERT               0
#define LV_USE_ASSERT_NULL          1
#define LV_USE_ASSERT_MALLOC        1
#define LV_USE_ASSERT_STYLE         0
#define LV_USE_ASSERT_MEM_INTEGRITY 0
#define LV_USE_ASSERT_OBJ           0

/* v9.6.0 replaced LV_ASSERT_OBJ with the LV_CHECK_ARG family. Without Kconfig
 * LV_USE_CHECK_ARG defaults to 1, which puts a NULL check at the head of every
 * public LVGL call; the class and validity layers on top of it walk the widget
 * tree. LV_USE_ASSERT_OBJ was 0 here, so this is new work nothing asked for —
 * off, to keep the size ratchet and the paint path where they were. */
#define LV_USE_CHECK_ARG            0
#define LV_USE_CHECK_OBJ_CLASSTYPE  0
#define LV_USE_CHECK_OBJ_VALIDITY   0

/* v9.6.0 renamed LV_ASSERT_HANDLER_INCLUDE; the old name warns and is removed
 * in v10.0. The handler macro itself still belongs in lv_conf.h. */
#define LV_ASSERT_USE_CUSTOM_INCLUDE 1
#define LV_ASSERT_CUSTOM_INCLUDE <stdint.h>
#define LV_ASSERT_HANDLER while(1);

/* Debug overlays off */
#define LV_USE_REFR_DEBUG         0
#define LV_USE_LAYER_DEBUG        0
#define LV_USE_PARALLEL_DRAW_DEBUG 0

/* Other features */
#define LV_ENABLE_GLOBAL_CUSTOM   0
#define LV_CACHE_DEF_SIZE         0
#define LV_IMAGE_HEADER_CACHE_DEF_CNT 0
#define LV_GRADIENT_MAX_STOPS     2
#define LV_COLOR_MIX_ROUND_OFS    0
#define LV_OBJ_STYLE_CACHE        0
#define LV_USE_OBJ_ID             0
#define LV_OBJ_ID_AUTO_ASSIGN     0
#define LV_USE_OBJ_ID_BUILTIN     1
#define LV_USE_OBJ_PROPERTY       0
#define LV_USE_OBJ_PROPERTY_NAME  0
#define LV_USE_VG_LITE_THORVG     0

/*=====================
 *  COMPILER SETTINGS
 *====================*/
#define LV_BIG_ENDIAN_SYSTEM  0
#define LV_ATTRIBUTE_TICK_INC
#define LV_ATTRIBUTE_TIMER_HANDLER
#define LV_ATTRIBUTE_FLUSH_READY
#define LV_ATTRIBUTE_MEM_ALIGN_SIZE 4
#define LV_ATTRIBUTE_MEM_ALIGN
#define LV_ATTRIBUTE_LARGE_CONST
#define LV_ATTRIBUTE_LARGE_RAM_ARRAY
#define LV_ATTRIBUTE_FAST_MEM
#define LV_EXPORT_CONST_INT(int_value) struct _silence_gcc_warning
#define LV_ATTRIBUTE_EXTERN_DATA
#define LV_USE_FLOAT            0
#define LV_USE_MATRIX           0
#define LV_USE_PRIVATE_API      0

/*==================
 *   FONT USAGE
 *===================*/
#define LV_FONT_MONTSERRAT_8  0
#define LV_FONT_MONTSERRAT_10 0
#define LV_FONT_MONTSERRAT_12 0
#define LV_FONT_MONTSERRAT_14 1  /* default font */
#define LV_FONT_MONTSERRAT_16 0
#define LV_FONT_MONTSERRAT_18 0
#define LV_FONT_MONTSERRAT_20 0
#define LV_FONT_MONTSERRAT_22 0
#define LV_FONT_MONTSERRAT_24 0
#define LV_FONT_MONTSERRAT_26 0
#define LV_FONT_MONTSERRAT_28 0
#define LV_FONT_MONTSERRAT_30 0
#define LV_FONT_MONTSERRAT_32 0
#define LV_FONT_MONTSERRAT_34 0
#define LV_FONT_MONTSERRAT_36 0
#define LV_FONT_MONTSERRAT_38 0
#define LV_FONT_MONTSERRAT_40 0
#define LV_FONT_MONTSERRAT_42 0
#define LV_FONT_MONTSERRAT_44 0
#define LV_FONT_MONTSERRAT_46 0
#define LV_FONT_MONTSERRAT_48 0

#define LV_FONT_MONTSERRAT_28_COMPRESSED 0
#define LV_FONT_DEJAVU_16_PERSIAN_HEBREW 0
#define LV_FONT_SIMSUN_14_CJK            0
#define LV_FONT_SIMSUN_16_CJK            0
#define LV_FONT_UNSCII_8  0
#define LV_FONT_UNSCII_16 0

#define LV_FONT_CUSTOM_DECLARE
#define LV_FONT_DEFAULT &lv_font_montserrat_14
#define LV_FONT_FMT_TXT_LARGE 0
#define LV_USE_FONT_COMPRESSED 0
#define LV_USE_FONT_PLACEHOLDER 1

/*=================
 *  TEXT SETTINGS
 *=================*/
#define LV_TXT_ENC LV_TXT_ENC_UTF8
#define LV_TXT_BREAK_CHARS " ,.;:-_)]}"
#define LV_TXT_LINE_BREAK_LONG_LEN  0
#define LV_TXT_LINE_BREAK_LONG_PRE_MIN_LEN  3
#define LV_TXT_LINE_BREAK_LONG_POST_MIN_LEN 3
#define LV_USE_BIDI 0
#define LV_USE_ARABIC_PERSIAN_CHARS 0

/*==================
 * WIDGETS
 *================*/
#define LV_WIDGETS_HAS_DEFAULT_VALUE 1

#define LV_USE_ANIMIMG    0
#define LV_USE_ARC        1
#define LV_USE_BAR        1
#define LV_USE_BUTTON     1
#define LV_USE_BUTTONMATRIX 1
#define LV_USE_CALENDAR   1
#define LV_USE_CANVAS     0
#define LV_USE_CHART      0
#define LV_USE_CHECKBOX   1
#define LV_USE_DROPDOWN   1
#define LV_USE_IMAGE      1
#define LV_USE_IMAGEBUTTON 0
#define LV_USE_KEYBOARD   1
#define LV_USE_LABEL      1
#if LV_USE_LABEL
    #define LV_LABEL_TEXT_SELECTION 0
    #define LV_LABEL_LONG_TXT_HINT 0
    #define LV_LABEL_WAIT_CHAR_COUNT 3
#endif
#define LV_USE_LED        0
#define LV_USE_LINE       1
#define LV_USE_LIST       1
#define LV_USE_LOTTIE     0
#define LV_USE_MENU       0
#define LV_USE_MSGBOX     0
#define LV_USE_ROLLER     1
#define LV_USE_SCALE      0
#define LV_USE_SLIDER     1
#define LV_USE_SPAN       0
#define LV_USE_SPINBOX    0
#define LV_USE_SPINNER    1
#define LV_USE_SWITCH     1
#define LV_USE_TEXTAREA   1
#define LV_USE_TABLE      0
#define LV_USE_TABVIEW    0
#define LV_USE_TILEVIEW   0
#define LV_USE_WIN        0

/*==================
 * THEMES
 *==================*/
#define LV_USE_THEME_DEFAULT 1
#if LV_USE_THEME_DEFAULT
    #define LV_THEME_DEFAULT_DARK 1   /* dark theme for OLED/TFT */
    #define LV_THEME_DEFAULT_GROW 0   /* disable grow animation to save CPU */
    #define LV_THEME_DEFAULT_TRANSITION_TIME 80
#endif
#define LV_USE_THEME_SIMPLE 0
#define LV_USE_THEME_MONO   0

/*==================
 * LAYOUTS
 *==================*/
#define LV_USE_FLEX 1
#define LV_USE_GRID 0

/*==================
 * 3RD PARTY LIBS
 *==================*/
#define LV_USE_FS_STDIO  0
#define LV_USE_FS_POSIX  0
#define LV_USE_FS_WIN32  0
#define LV_USE_FS_FATFS  0
#define LV_USE_FS_MEMFS  0
#define LV_USE_FS_LITTLEFS 0
#define LV_USE_FS_ARDUINO_ESP_LITTLEFS 0
#define LV_USE_FS_ARDUINO_SD 0

#define LV_USE_LODEPNG   0
#define LV_USE_LIBPNG    0
#define LV_USE_BMP       0
#define LV_USE_RLE       0
#define LV_USE_QRCODE    0
#define LV_USE_BARCODE   0
#define LV_USE_TJPGD     0
#define LV_USE_LIBJPEG_TURBO 0
#define LV_USE_FREETYPE  0
#define LV_USE_TINY_TTF  0
#define LV_USE_RLOTTIE   0
#define LV_USE_FFMPEG    0
/* v9.6.0 dropped the _INTERNAL / _EXTERNAL split on both of these. */
#define LV_USE_THORVG        0
#define LV_USE_LZ4           0

/*==================
 * DEVICES
 *==================*/
#define LV_USE_SDL              0
#define LV_USE_X11              0
#define LV_USE_LINUX_FBDEV      0
#define LV_USE_NUTTX            0
#define LV_USE_LINUX_DRM        0
#define LV_USE_TFT_ESPI         0
#define LV_USE_EVDEV            0
#define LV_USE_LIBINPUT         0
#define LV_USE_ST7735           0
#define LV_USE_ST7789           0
#define LV_USE_ST7796           0
#define LV_USE_ILI9341          0
#define LV_USE_GENERIC_MIPI     0
#define LV_USE_RENESAS_GLCDC    0
#define LV_USE_OPENGLES         0
#define LV_USE_WINDOWS          0

/*==================
 * EXAMPLES & DEMOS
 *==================*/
#define LV_BUILD_EXAMPLES 0
#define LV_USE_DEMO_WIDGETS 0
#define LV_USE_DEMO_KEYPAD_AND_ENCODER 0
#define LV_USE_DEMO_BENCHMARK 0
#define LV_USE_DEMO_RENDER 0
#define LV_USE_DEMO_STRESS 0
#define LV_USE_DEMO_MUSIC 0
#define LV_USE_DEMO_FLEX_LAYOUT 0
#define LV_USE_DEMO_MULTILANG 0
#define LV_USE_DEMO_TRANSFORM 0
#define LV_USE_DEMO_SCROLL 0
#define LV_USE_DEMO_VECTOR_GRAPHIC 0
#define LV_USE_DEMO_EBIKE 0

#endif /* LV_CONF_H */
