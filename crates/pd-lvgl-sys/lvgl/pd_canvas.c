// SPDX-License-Identifier: GPL-3.0-only
/*
 * pd_canvas: the retained display list behind picodroid.graphics.Canvas.
 *
 * Android runs View.onDraw(Canvas) on the UI thread and the canvas rasterises
 * as it goes. Picodroid cannot do either literally. A pixel-backed lv_canvas
 * needs width x height x 2 bytes (150 KB for a 320x240 panel, more than the
 * display board has free), and LVGL renders from the main task's tick, where
 * no Java can run, one 20-row band at a time. So View.invalidate() posts a
 * Runnable, the Runnable runs onDraw, each Canvas.drawX lands here as one
 * packed op in a per-object list, and this file's LV_EVENT_DRAW_MAIN hook
 * replays the list with lv_draw_rect / line / arc / label every time LVGL
 * paints a band that crosses the view. The hook runs after the object's own
 * background (lv_obj's class handler draws first), so setBackgroundColor
 * sits under what onDraw drew, as on Android, and the layer's clip area is
 * already the object's coordinates, so nothing spills outside the view.
 *
 * The replay only reads the list. Java rewrites it between ticks on the same
 * task (pd_canvas_begin), never during a render, so a label's text pointer,
 * which LVGL keeps until the band is dispatched, stays valid. A band that
 * misses an op's bounds skips it before any draw task is made: a full-height
 * view is visited once per band.
 *
 * The list lives in LVGL's pool, grows in PD_CANVAS_STEP steps to
 * PD_CANVAS_MAX_BYTES and is freed with the object (LV_EVENT_DELETE).
 */
#include "pd_canvas.h"
#include "pd_fonts.h"

#define PD_CANVAS_STEP 256

enum {
    PD_OP_FILL = 1,
    PD_OP_RECT,
    PD_OP_LINE,
    PD_OP_ARC,
    PD_OP_TEXT,
};

#define PD_FLAG_ROUND 0x01

/* One op, 32 bytes on every target: no pointers, so the simulator's list is
 * the device's size. Text follows the record, NUL-terminated, and `size`
 * covers both, rounded up to 4 so the next record stays aligned. */
typedef struct {
    uint8_t kind;
    uint8_t flags;
    uint16_t size;
    int16_t bx1, by1, bx2, by2; /* view-local bounds, for the band test */
    int16_t a, b, c, d, e;      /* kind-specific, see the pd_canvas_* below */
    int16_t w;
    uint32_t color;
    uint32_t color2;
} pd_op_t;

_Static_assert(sizeof(pd_op_t) == 32, "pd_op_t is 32 bytes on every target");

typedef struct {
    uint8_t *ops;
    uint16_t len;
    uint16_t cap;
    uint16_t dropped;
} pd_canvas_t;

static int16_t clamp16(int32_t v)
{
    if(v < INT16_MIN) return INT16_MIN;
    if(v > INT16_MAX) return INT16_MAX;
    return (int16_t)v;
}

static lv_color_t rgb_of(uint32_t argb)
{
    return lv_color_hex(argb & 0xFFFFFFu);
}

/* The op's alpha scaled by the view's own (setAlpha, a fade on an ancestor). */
static lv_opa_t opa_of(uint32_t argb, lv_opa_t view_opa)
{
    return (lv_opa_t)LV_OPA_MIX2(argb >> 24, view_opa);
}

static pd_canvas_t *canvas_of(lv_obj_t *obj)
{
    return obj == NULL ? NULL : (pd_canvas_t *)lv_obj_get_user_data(obj);
}

static pd_op_t *push(lv_obj_t *obj, uint8_t kind, size_t text_len)
{
    pd_canvas_t *cv = canvas_of(obj);
    if(cv == NULL) return NULL;
    size_t need = sizeof(pd_op_t);
    if(kind == PD_OP_TEXT) need += text_len + 1;
    need = (need + 3u) & ~(size_t)3u;
    if((size_t)cv->len + need > (size_t)cv->cap) {
        size_t cap = cv->cap;
        while(cap < (size_t)cv->len + need) cap += PD_CANVAS_STEP;
        if(cap > PD_CANVAS_MAX_BYTES) {
            cv->dropped++;
            return NULL;
        }
        uint8_t *grown = lv_realloc(cv->ops, cap);
        if(grown == NULL) {
            cv->dropped++;
            return NULL;
        }
        cv->ops = grown;
        cv->cap = (uint16_t)cap;
    }
    pd_op_t *op = (pd_op_t *)(cv->ops + cv->len);
    lv_memzero(op, sizeof(*op));
    op->kind = kind;
    op->size = (uint16_t)need;
    cv->len = (uint16_t)(cv->len + need);
    return op;
}

static void bounds(pd_op_t *op, int32_t x1, int32_t y1, int32_t x2, int32_t y2)
{
    op->bx1 = clamp16(x1);
    op->by1 = clamp16(y1);
    op->bx2 = clamp16(x2);
    op->by2 = clamp16(y2);
}

/* Row `index` of the face table, or NULL. */
static const lv_font_t *face_at(int32_t index)
{
    size_t count = 0;
    const pd_font_t *faces = pd_font_table(&count);
    if(faces == NULL || index < 0 || (size_t)index >= count) return NULL;
    return faces[index].font;
}

static void replay(const pd_op_t *op, lv_layer_t *layer, const lv_area_t *origin, lv_opa_t view_opa)
{
    int32_t ox = origin->x1;
    int32_t oy = origin->y1;
    switch(op->kind) {
        case PD_OP_FILL: {
            lv_draw_rect_dsc_t d;
            lv_draw_rect_dsc_init(&d);
            d.bg_color = rgb_of(op->color);
            d.bg_opa = opa_of(op->color, view_opa);
            lv_draw_rect(layer, &d, origin);
            break;
        }
        case PD_OP_RECT: {
            lv_draw_rect_dsc_t d;
            lv_draw_rect_dsc_init(&d);
            d.radius = op->a;
            d.bg_color = rgb_of(op->color);
            d.bg_opa = opa_of(op->color, view_opa);
            if(op->w > 0) {
                d.border_color = rgb_of(op->color2);
                d.border_opa = opa_of(op->color2, view_opa);
                d.border_width = op->w;
            }
            lv_area_t area = { ox + op->bx1, oy + op->by1, ox + op->bx2, oy + op->by2 };
            lv_draw_rect(layer, &d, &area);
            break;
        }
        case PD_OP_LINE: {
            lv_draw_line_dsc_t d;
            lv_draw_line_dsc_init(&d);
            d.p1.x = ox + op->a;
            d.p1.y = oy + op->b;
            d.p2.x = ox + op->c;
            d.p2.y = oy + op->d;
            d.width = op->w;
            d.color = rgb_of(op->color);
            d.opa = opa_of(op->color, view_opa);
            d.round_start = (op->flags & PD_FLAG_ROUND) ? 1 : 0;
            d.round_end = d.round_start;
            lv_draw_line(layer, &d);
            break;
        }
        case PD_OP_ARC: {
            lv_draw_arc_dsc_t d;
            lv_draw_arc_dsc_init(&d);
            d.center.x = ox + op->a;
            d.center.y = oy + op->b;
            d.radius = (uint16_t)op->c;
            d.start_angle = op->d;
            d.end_angle = op->e;
            d.width = op->w;
            d.color = rgb_of(op->color);
            d.opa = opa_of(op->color, view_opa);
            d.rounded = (op->flags & PD_FLAG_ROUND) ? 1 : 0;
            lv_draw_arc(layer, &d);
            break;
        }
        case PD_OP_TEXT: {
            lv_draw_label_dsc_t d;
            lv_draw_label_dsc_init(&d);
            d.text = (const char *)(op + 1);
            d.font = face_at(op->a);
            if(d.font == NULL) break;
            d.color = rgb_of(op->color);
            d.opa = opa_of(op->color, view_opa);
            lv_area_t area = { ox + op->bx1, oy + op->by1, ox + op->bx2, oy + op->by2 };
            lv_draw_label(layer, &d, &area);
            break;
        }
        default:
            break;
    }
}

static void draw_cb(lv_event_t *e)
{
    lv_obj_t *obj = lv_event_get_current_target_obj(e);
    pd_canvas_t *cv = canvas_of(obj);
    if(cv == NULL || cv->len == 0) return;
    lv_opa_t view_opa = lv_obj_get_style_opa_recursive(obj, LV_PART_MAIN);
    if(view_opa <= LV_OPA_MIN) return;
    lv_layer_t *layer = lv_event_get_layer(e);
    lv_area_t origin;
    lv_obj_get_coords(obj, &origin);
    const lv_area_t *clip = &layer->_clip_area;
    for(uint16_t off = 0; off < cv->len;) {
        const pd_op_t *op = (const pd_op_t *)(cv->ops + off);
        off = (uint16_t)(off + op->size);
        if(op->kind != PD_OP_FILL &&
           (origin.x1 + op->bx2 < clip->x1 || origin.x1 + op->bx1 > clip->x2 ||
            origin.y1 + op->by2 < clip->y1 || origin.y1 + op->by1 > clip->y2)) {
            continue;
        }
        replay(op, layer, &origin, view_opa);
    }
}

static void delete_cb(lv_event_t *e)
{
    lv_obj_t *obj = lv_event_get_current_target_obj(e);
    pd_canvas_t *cv = canvas_of(obj);
    if(cv == NULL) return;
    lv_obj_set_user_data(obj, NULL);
    lv_free(cv->ops);
    lv_free(cv);
}

int pd_canvas_attach(lv_obj_t *obj)
{
    if(obj == NULL) return -1;
    pd_canvas_t *cv = lv_malloc(sizeof(*cv));
    if(cv == NULL) return -1;
    lv_memzero(cv, sizeof(*cv));
    lv_obj_set_user_data(obj, cv);
    lv_obj_add_event_cb(obj, draw_cb, LV_EVENT_DRAW_MAIN, NULL);
    lv_obj_add_event_cb(obj, delete_cb, LV_EVENT_DELETE, NULL);
    return 0;
}

void pd_canvas_begin(lv_obj_t *obj)
{
    pd_canvas_t *cv = canvas_of(obj);
    if(cv == NULL) return;
    cv->len = 0;
    cv->dropped = 0;
}

int pd_canvas_end(lv_obj_t *obj)
{
    if(obj == NULL) return 0;
    lv_obj_invalidate(obj);
    pd_canvas_t *cv = canvas_of(obj);
    return cv == NULL ? 0 : cv->dropped;
}

int pd_canvas_fill(lv_obj_t *obj, uint32_t argb)
{
    pd_op_t *op = push(obj, PD_OP_FILL, 0);
    if(op == NULL) return -1;
    op->color = argb;
    return 0;
}

/* a = corner radius, w = border width; color = fill, color2 = border. */
int pd_canvas_rect(lv_obj_t *obj, int32_t x1, int32_t y1, int32_t x2, int32_t y2, int32_t radius,
                   uint32_t fill_argb, uint32_t stroke_argb, int32_t stroke_width)
{
    if(x2 < x1 || y2 < y1) return 0; /* empty: Android draws nothing */
    pd_op_t *op = push(obj, PD_OP_RECT, 0);
    if(op == NULL) return -1;
    bounds(op, x1, y1, x2, y2);
    op->a = clamp16(radius < 0 ? 0 : radius);
    op->w = clamp16(stroke_width < 0 ? 0 : stroke_width);
    op->color = fill_argb;
    op->color2 = stroke_argb;
    return 0;
}

/* a, b = first point; c, d = second point; w = width. */
int pd_canvas_line(lv_obj_t *obj, int32_t x1, int32_t y1, int32_t x2, int32_t y2, int32_t width,
                   uint32_t argb, int round_caps)
{
    pd_op_t *op = push(obj, PD_OP_LINE, 0);
    if(op == NULL) return -1;
    int32_t half = width / 2 + 1;
    bounds(op, LV_MIN(x1, x2) - half, LV_MIN(y1, y2) - half, LV_MAX(x1, x2) + half,
           LV_MAX(y1, y2) + half);
    op->a = clamp16(x1);
    op->b = clamp16(y1);
    op->c = clamp16(x2);
    op->d = clamp16(y2);
    op->w = clamp16(width < 1 ? 1 : width);
    op->color = argb;
    op->flags = round_caps ? PD_FLAG_ROUND : 0;
    return 0;
}

/* a, b = centre; c = outer radius; d, e = start and end angle; w = width. */
int pd_canvas_arc(lv_obj_t *obj, int32_t cx, int32_t cy, int32_t radius, int32_t start_deg,
                  int32_t end_deg, int32_t width, uint32_t argb, int rounded)
{
    if(radius <= 0 || start_deg == end_deg) return 0;
    pd_op_t *op = push(obj, PD_OP_ARC, 0);
    if(op == NULL) return -1;
    bounds(op, cx - radius, cy - radius, cx + radius, cy + radius);
    op->a = clamp16(cx);
    op->b = clamp16(cy);
    op->c = clamp16(radius);
    op->d = clamp16(start_deg);
    op->e = clamp16(end_deg);
    op->w = clamp16(width < 1 ? 1 : width);
    op->color = argb;
    op->flags = rounded ? PD_FLAG_ROUND : 0;
    return 0;
}

/* Bounds = the laid-out text box; a = face index; the text follows the record. */
int pd_canvas_text(lv_obj_t *obj, int32_t x, int32_t baseline, int32_t font_index,
                   uint32_t argb, int align, const char *utf8, size_t len)
{
    const lv_font_t *font = face_at(font_index);
    if(font == NULL || utf8 == NULL || len == 0) return 0;
    if(len > PD_CANVAS_MAX_BYTES) len = PD_CANVAS_MAX_BYTES;
    pd_op_t *op = push(obj, PD_OP_TEXT, len);
    if(op == NULL) return -1;
    char *text = (char *)(op + 1);
    lv_memcpy(text, utf8, len);
    text[len] = '\0';
    lv_point_t size;
    lv_text_get_size(&size, text, font, 0, 0, LV_COORD_MAX, LV_TEXT_FLAG_NONE);
    int32_t left = x;
    /* Centred on x the way a centre-aligned label of even width centres it
     * ((width - text) / 2 from its left edge), so a column of drawText labels
     * lands where a column of TextViews did. */
    if(align == LV_TEXT_ALIGN_CENTER) left = x - (size.x + 1) / 2;
    else if(align == LV_TEXT_ALIGN_RIGHT) left = x - size.x;
    /* LVGL lays a line out from its top; Android's y is the baseline. */
    int32_t top = baseline + pd_canvas_ascent(font_index);
    bounds(op, left, top, left + size.x - 1, top + size.y - 1);
    op->a = (int16_t)font_index;
    op->color = argb;
    return 0;
}

int32_t pd_canvas_ascent(int32_t font_index)
{
    const lv_font_t *font = face_at(font_index);
    /* base_line is measured up from the bottom of the line. */
    return font == NULL ? 0 : -(lv_font_get_line_height(font) - font->base_line);
}

int32_t pd_canvas_descent(int32_t font_index)
{
    const lv_font_t *font = face_at(font_index);
    return font == NULL ? 0 : font->base_line;
}

int32_t pd_canvas_text_width(int32_t font_index, const char *utf8, size_t len)
{
    const lv_font_t *font = face_at(font_index);
    if(font == NULL || utf8 == NULL || len == 0) return 0;
    char *text = lv_malloc(len + 1);
    if(text == NULL) return 0;
    lv_memcpy(text, utf8, len);
    text[len] = '\0';
    lv_point_t size;
    lv_text_get_size(&size, text, font, 0, 0, LV_COORD_MAX, LV_TEXT_FLAG_NONE);
    lv_free(text);
    return size.x;
}
