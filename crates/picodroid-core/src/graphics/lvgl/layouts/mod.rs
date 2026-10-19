// SPDX-License-Identifier: GPL-3.0-only
//! Layouts of picodroid's own, registered with LVGL as its flex and grid
//! are: LVGL calls the update callback whenever a container on the layout
//! is dirty (a child added, removed or resized, the container resized, a
//! style changed) and the min-size callback when a content-sized container
//! asks how big its content is. The solvers (`relative`, and `linear` to
//! come) are pure; this module reads the children into their terms, writes
//! the rectangles back, and keeps the per-child state.
//!
//! That state — a child's gravity, its weight, its view id, its
//! RelativeLayout rules, and which of its axes `setSize` was told are
//! `match_parent` — lives in style properties of this crate's own
//! (`lv_style_register_prop`), as local style values on the child. No pool,
//! no cap, no delete hook, and the values follow a view that is reparented.
//! The size intent is kept apart from the style width because a solver
//! that stretches a child overwrites that width with pixels.

pub mod place;
pub mod relative;

use core::cell::Cell;

use crate::lvgl_ffi::*;
use crate::util::local::Core0;

use place::{gravity_axis, Kid, Rect};
use relative::{Anchor, Rules, Span, ANCHORS, MAX_KIDS};

/// `View.NO_ID`: what a child with no id carries, which no anchor names.
const NO_ID: i32 = -1;

const INTENT_MATCH_W: i32 = 1;
const INTENT_MATCH_H: i32 = 2;

/// The property ids `register` took from LVGL: `LV_STYLE_PROP_INV` (0)
/// before it runs, or for one LVGL had no id left for, in which case the
/// setters do nothing and the layouts read defaults.
#[derive(Clone, Copy)]
struct Props {
    size_intent: lv_style_prop_t,
    gravity: lv_style_prop_t,
    view_id: lv_style_prop_t,
    rel_flags: lv_style_prop_t,
    anchors: [lv_style_prop_t; ANCHORS],
}

const NO_PROPS: Props = Props {
    size_intent: LV_STYLE_PROP_INV,
    gravity: LV_STYLE_PROP_INV,
    view_id: LV_STYLE_PROP_INV,
    rel_flags: LV_STYLE_PROP_INV,
    anchors: [LV_STYLE_PROP_INV; ANCHORS],
};

struct State {
    props: Cell<Props>,
    /// The layout id LVGL gave `RelativeLayout`; 0 (`LV_LAYOUT_NONE`) until
    /// `register` runs.
    relative: Cell<u32>,
    /// Said once per process: a RelativeLayout with circular rules, or
    /// with more children than `MAX_KIDS`.
    warned_cycle: Cell<bool>,
    warned_many: Cell<bool>,
}

// SAFETY: written at init and by natives, read by LVGL's layout pass, all on
// core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        props: Cell::new(NO_PROPS),
        relative: Cell::new(LV_LAYOUT_NONE),
        warned_cycle: Cell::new(false),
        warned_many: Cell::new(false),
    })
};

/// Register the properties and the layouts with LVGL. Once, right after
/// `lv_init`, before any object exists.
pub(in crate::graphics) fn register() {
    let relayout = LV_STYLE_PROP_FLAG_LAYOUT_UPDATE | LV_STYLE_PROP_FLAG_PARENT_LAYOUT_UPDATE;
    let mut anchors = [LV_STYLE_PROP_INV; ANCHORS];
    for a in &mut anchors {
        *a = lv_style_register_prop(relayout);
    }
    STATE.props.set(Props {
        // The intent accompanies a width or height write, which re-lays
        // out by itself.
        size_intent: lv_style_register_prop(0),
        gravity: lv_style_register_prop(relayout),
        view_id: lv_style_register_prop(relayout),
        rel_flags: lv_style_register_prop(relayout),
        anchors,
    });
    STATE.relative.set(layout_create(
        relative_update_cb,
        Some(relative_min_size_cb),
    ));
}

/// The layout a `RelativeLayout` is put on.
pub(in crate::graphics) fn relative_layout_id() -> u32 {
    STATE.relative.get()
}

fn set_or_clear(o: Obj, prop: lv_style_prop_t, value: i32) {
    if prop == LV_STYLE_PROP_INV {
        return;
    }
    if value != 0 {
        set_style_num(o, prop, value);
    } else {
        lv_obj_remove_local_style_prop(o, prop, LV_PART_MAIN);
    }
}

/// What `setSize` was told about `o`'s axes: `match_parent` or not.
pub(in crate::graphics) fn set_size_intent(o: Obj, match_w: bool, match_h: bool) {
    let bits = if match_w { INTENT_MATCH_W } else { 0 } | if match_h { INTENT_MATCH_H } else { 0 };
    set_or_clear(o, STATE.props.get().size_intent, bits);
}

/// An Android gravity on `o`: a container's (`RelativeLayout.setGravity`,
/// `LinearLayout.setGravity`) or a `LinearLayout` child's `layout_gravity`.
pub(in crate::graphics) fn set_gravity_prop(o: Obj, gravity: i32) {
    set_or_clear(o, STATE.props.get().gravity, gravity);
}

/// A `RelativeLayout` child's id and rules: the verb flags (`relative::F_*`)
/// and the id each sibling verb names (`relative::A_*` order), 0 for none.
pub(in crate::graphics) fn set_relative_rules(
    child: Obj,
    id: i32,
    flags: i32,
    anchors: &[i32; ANCHORS],
) {
    let p = STATE.props.get();
    if p.view_id == LV_STYLE_PROP_INV {
        return;
    }
    set_or_clear(child, p.view_id, id);
    set_or_clear(child, p.rel_flags, flags);
    for (k, &a) in anchors.iter().enumerate() {
        set_or_clear(child, p.anchors[k], a);
    }
    if let Some(parent) = lv_obj_get_parent(child) {
        lv_obj_mark_layout_as_dirty(parent);
    }
}

/// A numeric style property of `o`, through the cascade: the theme's or the
/// default when `o`'s own style has none.
fn style_num(o: Obj, prop: lv_style_prop_t) -> i32 {
    let v = lv_obj_get_style_prop(o, LV_PART_MAIN, prop);
    // SAFETY: every property read here is numeric (sizes, pads, margins),
    // which LVGL returns in `num`.
    unsafe { v.num }
}

fn kid_of(child: Obj) -> Kid {
    let intent = local_style_num(child, STATE.props.get().size_intent).unwrap_or(0);
    Kid {
        w: lv_obj_get_width(child),
        h: lv_obj_get_height(child),
        match_w: intent & INTENT_MATCH_W != 0,
        match_h: intent & INTENT_MATCH_H != 0,
        margin: [
            style_num(child, LV_STYLE_MARGIN_LEFT),
            style_num(child, LV_STYLE_MARGIN_TOP),
            style_num(child, LV_STYLE_MARGIN_RIGHT),
            style_num(child, LV_STYLE_MARGIN_BOTTOM),
        ],
        // What LVGL's own layouts leave alone.
        gone: lv_obj_has_flag(child, LV_OBJ_FLAG_HIDDEN)
            || lv_obj_has_flag(child, LV_OBJ_FLAG_IGNORE_LAYOUT)
            || lv_obj_has_flag(child, LV_OBJ_FLAG_FLOATING),
    }
}

/// The container's content size per axis, `None` on a `wrap_content` one.
fn span_of(cont: Obj) -> Span {
    Span {
        w: (style_num(cont, LV_STYLE_WIDTH) != LV_SIZE_CONTENT)
            .then(|| lv_obj_get_content_width(cont)),
        h: (style_num(cont, LV_STYLE_HEIGHT) != LV_SIZE_CONTENT)
            .then(|| lv_obj_get_content_height(cont)),
    }
}

/// A container's children as the solver sees them, with their rules
/// resolved to sibling indices.
struct Gathered {
    n: usize,
    objs: [Option<Obj>; MAX_KIDS],
    kids: [Kid; MAX_KIDS],
    rules: [Rules; MAX_KIDS],
}

fn gather(cont: Obj) -> Gathered {
    let p = STATE.props.get();
    let count = lv_obj_get_child_count(cont) as usize;
    if count > MAX_KIDS && !STATE.warned_many.replace(true) {
        crate::pd_warn!(
            "RelativeLayout: more than {} children; the rest are placed as unanchored",
            MAX_KIDS
        );
    }
    let n = count.min(MAX_KIDS);
    let mut g = Gathered {
        n,
        objs: [None; MAX_KIDS],
        kids: [Kid::default(); MAX_KIDS],
        rules: [Rules::default(); MAX_KIDS],
    };
    let mut ids = [NO_ID; MAX_KIDS];
    for (i, id) in ids.iter_mut().enumerate().take(n) {
        let Some(child) = lv_obj_get_child(cont, i as i32) else {
            continue;
        };
        g.objs[i] = Some(child);
        g.kids[i] = kid_of(child);
        *id = local_style_num(child, p.view_id).unwrap_or(NO_ID);
        g.rules[i].flags = local_style_num(child, p.rel_flags).unwrap_or(0) as u16;
    }
    for i in 0..n {
        let Some(child) = g.objs[i] else {
            continue;
        };
        for k in 0..ANCHORS {
            g.rules[i].anchors[k] = match local_style_num(child, p.anchors[k]) {
                None | Some(0) => Anchor::None,
                Some(id) => ids[..n]
                    .iter()
                    .position(|&x| x == id && x != NO_ID)
                    .filter(|&j| j != i)
                    .map_or(Anchor::Missing, |j| Anchor::Sibling(j as u8)),
            };
        }
    }
    g
}

/// Put the solved rectangles on the children: a size only where the solver
/// stretched the child, and the position. Each is a no-op when unchanged,
/// which is what lets the pass LVGL runs after this one come to rest.
fn apply(g: &Gathered, out: &[Rect]) {
    for (i, &r) in out.iter().enumerate().take(g.n) {
        let Some(o) = g.objs[i] else {
            continue;
        };
        let k = &g.kids[i];
        if k.gone {
            continue;
        }
        if r.w != k.w || r.h != k.h {
            if r.w != k.w {
                lv_obj_set_width(o, r.w);
            }
            if r.h != k.h {
                lv_obj_set_height(o, r.h);
            }
            lv_obj_refr_size(o);
        }
        lv_obj_move_to(o, r.x, r.y);
    }
}

fn relative_solve(cont: Obj, span: Span, g: &Gathered, out: &mut [Rect]) -> relative::Solved {
    let gravity = local_style_num(cont, STATE.props.get().gravity).unwrap_or(0);
    let solved = relative::solve(
        span,
        (gravity_axis(gravity, false), gravity_axis(gravity, true)),
        &g.kids[..g.n],
        &g.rules[..g.n],
        &mut out[..g.n],
    );
    if solved.cycle && !STATE.warned_cycle.replace(true) {
        crate::pd_warn!(
            "RelativeLayout: circular rules; the children in the cycle are placed as unanchored"
        );
    }
    solved
}

lv_layout_cb! {
    /// Lay a `RelativeLayout` out: LVGL calls this whenever the container
    /// is dirty.
    fn relative_update_cb(cont) {
        let g = gather(cont);
        let span = span_of(cont);
        let mut out = [Rect::default(); MAX_KIDS];
        relative_solve(cont, span, &g, &mut out);
        apply(&g, &out);
        if span.w.is_none() || span.h.is_none() {
            // As LVGL's flex does: a content-sized container takes its
            // children's extent now, not a pass later.
            lv_obj_refr_size(cont);
        }
    }
}

lv_min_size_cb! {
    /// How big a content-sized `RelativeLayout` is on one axis: the
    /// children's extent plus the padding.
    fn relative_min_size_cb(cont, width) -> Option<i32> {
        let g = gather(cont);
        let mut span = span_of(cont);
        if width {
            span.w = None;
        } else {
            span.h = None;
        }
        let mut out = [Rect::default(); MAX_KIDS];
        let solved = relative_solve(cont, span, &g, &mut out);
        let pads = if width {
            style_num(cont, LV_STYLE_PAD_LEFT) + style_num(cont, LV_STYLE_PAD_RIGHT)
        } else {
            style_num(cont, LV_STYLE_PAD_TOP) + style_num(cont, LV_STYLE_PAD_BOTTOM)
        };
        Some(if width { solved.w } else { solved.h } + pads)
    }
}
