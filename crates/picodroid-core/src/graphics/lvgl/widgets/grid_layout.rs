// SPDX-License-Identifier: GPL-3.0-only
//! LVGL impl of `GridLayout`: an `lv_obj` on LVGL's grid layout.
//!
//! Java chooses the cell (Android's placement cursor lives in
//! `GridLayout.java`); this side keeps, per grid, the column and row
//! templates LVGL lays the cells out from. LVGL does not copy a template —
//! `lv_obj_set_grid_dsc_array` stores the two pointers as style values and
//! reads the arrays at every layout pass — so each live grid owns a slot in
//! a static table whose arrays outlive any object that points at them. A
//! slot is claimed at create time and released by the object's
//! `LV_EVENT_DELETE` hook.
//!
//! A track is content-sized until a child in it carries a weight, then it is
//! `LV_GRID_FR(weight)`; a weighted span raises every track it covers, and
//! the largest weight on a track wins, which makes the result independent of
//! the order the children were added in.
//!
//! LVGL sizes a `LV_GRID_CONTENT` track to its widest cell *without* the
//! cell's margins (`lv_grid.c::calc_cols`), then places the cell at the
//! track's start plus its left margin, so a margined cell ran into the next
//! column (picoenvmon's `Air quality` 3 px from its value where the layout
//! asked for 8; QA 2026-10-22, U7). Android's `GridLayout` counts the
//! margins in the column. So this side measures content tracks itself: the
//! template LVGL reads (`live`) carries, for each content track, the widest
//! single-span cell in it plus that cell's margins, re-measured whenever a
//! child is placed, sized or moved (`LV_EVENT_CHILD_CHANGED`).

use core::cell::Cell;

use crate::lvgl_ffi::*;
use crate::util::local::Core0;

use super::super::handle_table;
use super::super::lifecycle;

/// Grids alive at once. A screen has one or two; the back stack keeps a
/// paused activity's tree, so a few screens deep is still within this.
const MAX_GRIDS: usize = 6;

/// Columns or rows per grid. A template holds one more slot for
/// `LV_GRID_TEMPLATE_LAST`.
pub(crate) const MAX_TRACKS: usize = 16;

/// Java's `GridLayout.ALIGN_*` codes, in `nativePlace`'s arguments.
const ALIGN_START: i32 = 0;
const ALIGN_CENTER: i32 = 1;
const ALIGN_END: i32 = 2;
const ALIGN_FILL: i32 = 3;

/// One axis's template: the tracks in use and the terminator after them.
struct Tracks {
    /// The template as Java asked: content-sized, or a weight.
    dsc: [Cell<i32>; MAX_TRACKS + 1],
    /// What LVGL reads: `dsc` with every content track replaced by the
    /// width (height) of its widest cell, margins included; 0 until a
    /// child is measured.
    live: [Cell<i32>; MAX_TRACKS + 1],
    count: Cell<usize>,
}

impl Tracks {
    const fn new() -> Self {
        Self {
            dsc: [const { Cell::new(LV_GRID_TEMPLATE_LAST) }; MAX_TRACKS + 1],
            live: [const { Cell::new(LV_GRID_TEMPLATE_LAST) }; MAX_TRACKS + 1],
            count: Cell::new(0),
        }
    }

    fn reset(&self) {
        for c in self.dsc.iter().chain(&self.live) {
            c.set(LV_GRID_TEMPLATE_LAST);
        }
        self.count.set(0);
    }

    /// Make the template at least `n` tracks long, new tracks content-sized.
    /// `false` when `n` exceeds `MAX_TRACKS`; the template is unchanged.
    fn extend(&self, n: usize) -> bool {
        if n > MAX_TRACKS {
            return false;
        }
        let count = self.count.get();
        if n > count {
            for c in &self.dsc[count..n] {
                c.set(LV_GRID_CONTENT);
            }
            for c in &self.live[count..n] {
                c.set(0);
            }
            self.dsc[n].set(LV_GRID_TEMPLATE_LAST);
            self.live[n].set(LV_GRID_TEMPLATE_LAST);
            self.count.set(n);
        }
        true
    }

    /// Give every track of `from..from + span` at least `size` (a
    /// `LV_GRID_FR` value); a larger share already there stays.
    fn raise(&self, from: usize, span: usize, size: i32) {
        let end = (from + span).min(self.count.get());
        for (c, l) in self.dsc[from..end].iter().zip(&self.live[from..end]) {
            if c.get() == LV_GRID_CONTENT || c.get() < size {
                c.set(size);
                l.set(size);
            }
        }
    }

    /// Re-measure the content tracks from the cells: track `i` is the
    /// widest (tallest) single-span cell at position `i` plus that cell's
    /// margins on the axis, as `cell_extent` reports them. `true` when a
    /// track changed.
    fn measure(&self, cells: &[(usize, usize, i32)]) -> bool {
        let mut changed = false;
        for i in 0..self.count.get() {
            if self.dsc[i].get() != LV_GRID_CONTENT {
                continue;
            }
            let size = cells
                .iter()
                .filter(|&&(pos, span, _)| pos == i && span == 1)
                .map(|&(_, _, extent)| extent)
                .max()
                .unwrap_or(0);
            if self.live[i].get() != size {
                self.live[i].set(size);
                changed = true;
            }
        }
        changed
    }

    fn as_ptr(&self) -> *const i32 {
        // `Cell<i32>` is `repr(transparent)` over `i32`.
        self.live.as_ptr().cast()
    }
}

/// The most cells one measuring pass reads; a grid with more keeps the
/// first of them in its tracks' measure.
const MAX_CELLS: usize = 64;

/// Every laid-out child of `cont` as `(position, span, extent)` on one axis,
/// the extent its size plus both margins there.
fn cells_of(cont: Obj, horizontal: bool, out: &mut [(usize, usize, i32); MAX_CELLS]) -> usize {
    let (pos_prop, span_prop, m_start, m_end) = if horizontal {
        (
            LV_STYLE_GRID_CELL_COLUMN_POS,
            LV_STYLE_GRID_CELL_COLUMN_SPAN,
            LV_STYLE_MARGIN_LEFT,
            LV_STYLE_MARGIN_RIGHT,
        )
    } else {
        (
            LV_STYLE_GRID_CELL_ROW_POS,
            LV_STYLE_GRID_CELL_ROW_SPAN,
            LV_STYLE_MARGIN_TOP,
            LV_STYLE_MARGIN_BOTTOM,
        )
    };
    let mut n = 0;
    for i in 0..lv_obj_get_child_count(cont) {
        if n == MAX_CELLS {
            break;
        }
        let Some(item) = lv_obj_get_child(cont, i as i32) else {
            continue;
        };
        if lv_obj_has_flag(item, LV_OBJ_FLAG_HIDDEN)
            || lv_obj_has_flag(item, LV_OBJ_FLAG_IGNORE_LAYOUT)
            || lv_obj_has_flag(item, LV_OBJ_FLAG_FLOATING)
        {
            continue;
        }
        let num = |prop: lv_style_prop_t| style_num(item, LV_PART_MAIN, prop);
        let size = if horizontal {
            lv_obj_get_width(item)
        } else {
            lv_obj_get_height(item)
        };
        out[n] = (
            num(pos_prop).max(0) as usize,
            num(span_prop).max(0) as usize,
            size + num(m_start) + num(m_end),
        );
        n += 1;
    }
    n
}

struct Slot {
    /// The grid object's address, 0 when free.
    container: Cell<usize>,
    cols: Tracks,
    rows: Tracks,
}

impl Slot {
    /// Re-measure both axes' content tracks from `cont`'s children; `true`
    /// when a track changed and the grid wants laying out again.
    fn refresh(&self, cont: Obj) -> bool {
        let mut cells = [(0usize, 0usize, 0i32); MAX_CELLS];
        let n = cells_of(cont, true, &mut cells);
        let cols = self.cols.measure(&cells[..n]);
        let n = cells_of(cont, false, &mut cells);
        let rows = self.rows.measure(&cells[..n]);
        cols || rows
    }
}

struct State {
    slots: [Slot; MAX_GRIDS],
}

// SAFETY: written by natives and by LVGL delete callbacks, read by LVGL's
// layout pass, all on core-0 JVM tasks.
static STATE: Core0<State> = unsafe {
    Core0::new(State {
        slots: [const {
            Slot {
                container: Cell::new(0),
                cols: Tracks::new(),
                rows: Tracks::new(),
            }
        }; MAX_GRIDS],
    })
};

/// What one `nativePlace` call says about a child's cell.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellSpec {
    pub col_align: i32,
    pub col: i32,
    pub col_span: i32,
    /// The column weight in tenths, 0 for none.
    pub col_fr: i32,
    pub row_align: i32,
    pub row: i32,
    pub row_span: i32,
    pub row_fr: i32,
}

pub(in crate::graphics) fn create() -> i32 {
    lifecycle::screen_ptr().map_or(0, create_under)
}

/// [`create`], under `parent` instead of the screen: the inflater makes a
/// node where it belongs, so no re-parenting follows.
#[inline(never)]
pub(in crate::graphics) fn create_under(parent: Obj) -> i32 {
    let Some(o) = lv_obj_create(parent) else {
        return 0;
    };
    super::frame_layout::make_flat(o);
    // Android GridLayout never scrolls; the fit check (display.rs) also
    // treats a scrollable object as a scroll view and stops there.
    lv_obj_remove_flag(o, LV_OBJ_FLAG_SCROLLABLE);
    if let Some(slot) = STATE.slots.iter().find(|s| s.container.get() == 0) {
        slot.container.set(o.addr());
        slot.cols.reset();
        slot.rows.reset();
    }
    // A grid that found no slot still works as a plain container; `place`
    // then refuses its first child and Java throws.
    add_event_cb(o, slot_delete_cb, LV_EVENT_DELETE);
    add_event_cb(o, child_changed_cb, LV_EVENT_CHILD_CHANGED);
    handle_table::register(o)
}

/// Gap in pixels between adjacent columns and rows.
pub(in crate::graphics) fn set_spacing(id: i32, px: i32) {
    super::linear_layout::set_spacing(id, px);
}

/// Put `child` in its cell and grow the templates to cover it. `false` when
/// the grid has no slot or the cell lies past `MAX_TRACKS`; nothing is
/// changed then.
pub(in crate::graphics) fn place(id: i32, child: i32, c: CellSpec) -> bool {
    let (Some(obj), Some(kid)) = (handle_table::get(id), handle_table::get(child)) else {
        return false;
    };
    let Some(slot) = STATE.slots.iter().find(|s| s.container.get() == obj.addr()) else {
        return false;
    };
    let (col, col_span, row, row_span) = (
        c.col.max(0) as usize,
        c.col_span.max(1) as usize,
        c.row.max(0) as usize,
        c.row_span.max(1) as usize,
    );
    if col + col_span > MAX_TRACKS || row + row_span > MAX_TRACKS {
        return false;
    }
    slot.cols.extend(col + col_span);
    slot.rows.extend(row + row_span);
    if c.col_fr > 0 {
        slot.cols.raise(col, col_span, fr(c.col_fr));
    }
    if c.row_fr > 0 {
        slot.rows.raise(row, row_span, fr(c.row_fr));
    }
    lv_obj_set_grid_cell(
        kid,
        grid_align(c.col_align),
        col as i32,
        col_span as i32,
        grid_align(c.row_align),
        row as i32,
        row_span as i32,
    );
    slot.refresh(obj);
    // SAFETY: both arrays live in `STATE`, a static, and `extend` keeps a
    // `LV_GRID_TEMPLATE_LAST` after the tracks in use; LVGL only ever reads
    // them, and re-setting the same pointers re-lays the grid out.
    unsafe { lv_obj_set_grid_dsc_array(obj, slot.cols.as_ptr(), slot.rows.as_ptr()) };
    true
}

lv_event_cb! {
    /// A child placed, sized or moved: re-measure the content tracks, and
    /// lay the grid out again when one changed. The layout pass itself
    /// moves children, which lands here too; a pass that changes no
    /// track marks nothing, which is what lets the grid come to rest.
    fn child_changed_cb(e) {
        let cont = e.current_target();
        let Some(slot) = STATE.slots.iter().find(|s| s.container.get() == cont.addr()) else {
            return;
        };
        if slot.refresh(cont) {
            lv_obj_mark_layout_as_dirty(cont);
        }
    }
}

/// Java's alignment code as LVGL's. Anything unexpected places at the start.
pub(crate) fn grid_align(code: i32) -> lv_grid_align_t {
    match code {
        ALIGN_CENTER => LV_GRID_ALIGN_CENTER,
        ALIGN_END => LV_GRID_ALIGN_END,
        ALIGN_FILL => LV_GRID_ALIGN_STRETCH,
        ALIGN_START => LV_GRID_ALIGN_START,
        _ => LV_GRID_ALIGN_START,
    }
}

/// A weight in tenths as a track size: `LV_GRID_FR(tenths)` clamped to
/// `1..=99` (`LV_GRID_FR(100)` is the template terminator), content-sized
/// for 0.
pub(crate) fn fr(tenths: i32) -> i32 {
    if tenths <= 0 {
        LV_GRID_CONTENT
    } else {
        lv_grid_fr(tenths.min(99))
    }
}

lv_event_cb! {
    /// Free the grid's slot when its object is deleted, so a later grid can
    /// take it; the templates stay valid memory either way (they are static).
    fn slot_delete_cb(e) {
        let container = e.target().addr();
        if container == 0 {
            return;
        }
        for slot in &STATE.slots {
            if slot.container.get() == container {
                slot.container.set(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_extend_fills_content_and_moves_the_terminator() {
        let t = Tracks::new();
        assert_eq!(t.dsc[0].get(), LV_GRID_TEMPLATE_LAST);
        assert!(t.extend(3));
        assert_eq!(t.count.get(), 3);
        assert_eq!(t.dsc[0].get(), LV_GRID_CONTENT);
        assert_eq!(t.dsc[2].get(), LV_GRID_CONTENT);
        assert_eq!(t.dsc[3].get(), LV_GRID_TEMPLATE_LAST);
        // Shorter never shrinks.
        assert!(t.extend(2));
        assert_eq!(t.count.get(), 3);
        assert_eq!(t.dsc[3].get(), LV_GRID_TEMPLATE_LAST);
    }

    #[test]
    fn tracks_refuse_a_seventeenth_track() {
        let t = Tracks::new();
        assert!(t.extend(MAX_TRACKS));
        assert_eq!(t.dsc[MAX_TRACKS].get(), LV_GRID_TEMPLATE_LAST);
        assert!(!t.extend(MAX_TRACKS + 1));
        assert_eq!(t.count.get(), MAX_TRACKS);
    }

    #[test]
    fn raise_takes_the_max_over_a_span_and_keeps_content_elsewhere() {
        let t = Tracks::new();
        assert!(t.extend(4));
        t.raise(1, 2, fr(10));
        assert_eq!(t.dsc[0].get(), LV_GRID_CONTENT);
        assert_eq!(t.dsc[1].get(), lv_grid_fr(10));
        assert_eq!(t.dsc[2].get(), lv_grid_fr(10));
        assert_eq!(t.dsc[3].get(), LV_GRID_CONTENT);
        // A smaller weight later does not lower a track; a larger one raises it.
        t.raise(1, 1, fr(5));
        assert_eq!(t.dsc[1].get(), lv_grid_fr(10));
        t.raise(2, 1, fr(20));
        assert_eq!(t.dsc[2].get(), lv_grid_fr(20));
        // Past the count is ignored.
        t.raise(3, 5, fr(10));
        assert_eq!(t.dsc[4].get(), LV_GRID_TEMPLATE_LAST);
    }

    #[test]
    fn a_content_track_measures_its_widest_cell_with_margins() {
        let t = Tracks::new();
        assert!(t.extend(3));
        assert_eq!(t.live[0].get(), 0);
        // picoenvmon's row: a 60 px label with 8 px margins in column 0, a
        // wider label with none; the value in column 1; a two-column span
        // counts for neither.
        let cells = [(0, 1, 76), (0, 1, 70), (1, 1, 40), (0, 2, 300)];
        assert!(t.measure(&cells));
        assert_eq!(
            (t.live[0].get(), t.live[1].get(), t.live[2].get()),
            (76, 40, 0)
        );
        assert_eq!(t.dsc[0].get(), LV_GRID_CONTENT, "the intent is kept");
        // The same cells again: nothing changed.
        assert!(!t.measure(&cells));
        // A weighted track is left to LVGL, however wide its cells.
        t.raise(1, 1, fr(10));
        assert!(!t.measure(&[(0, 1, 76), (1, 1, 500)]));
        assert_eq!(t.live[1].get(), lv_grid_fr(10));
        // A track whose cells went away measures 0 again.
        assert!(t.measure(&[(1, 1, 500)]));
        assert_eq!(t.live[0].get(), 0);
        assert_eq!(t.live[3].get(), LV_GRID_TEMPLATE_LAST);
    }

    #[test]
    fn fr_clamps_to_one_through_ninety_nine_and_zero_is_content() {
        assert_eq!(fr(0), LV_GRID_CONTENT);
        assert_eq!(fr(-3), LV_GRID_CONTENT);
        assert_eq!(fr(1), lv_grid_fr(1));
        assert_eq!(fr(99), lv_grid_fr(99));
        assert_eq!(fr(250), lv_grid_fr(99));
        assert_ne!(fr(250), LV_GRID_TEMPLATE_LAST);
    }

    #[test]
    fn grid_align_maps_javas_codes() {
        assert_eq!(grid_align(ALIGN_START), LV_GRID_ALIGN_START);
        assert_eq!(grid_align(ALIGN_CENTER), LV_GRID_ALIGN_CENTER);
        assert_eq!(grid_align(ALIGN_END), LV_GRID_ALIGN_END);
        assert_eq!(grid_align(ALIGN_FILL), LV_GRID_ALIGN_STRETCH);
        assert_eq!(grid_align(-1), LV_GRID_ALIGN_START);
        assert_eq!(grid_align(9), LV_GRID_ALIGN_START);
    }

    #[test]
    fn reset_clears_a_used_template() {
        let t = Tracks::new();
        assert!(t.extend(5));
        t.raise(0, 5, fr(10));
        t.reset();
        assert_eq!(t.count.get(), 0);
        assert!(t.dsc.iter().all(|c| c.get() == LV_GRID_TEMPLATE_LAST));
        assert!(t.live.iter().all(|c| c.get() == LV_GRID_TEMPLATE_LAST));
    }
}
