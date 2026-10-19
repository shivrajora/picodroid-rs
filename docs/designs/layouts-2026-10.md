# Layouts: GridLayout, RelativeLayout and per-child gravity in LinearLayout

**Status: design settled 2026-10-18; series 1 (GridLayout) in progress. §4 says what has landed.**

## 1. The finding

A review of the picodroid apps against Android (2026-10-18) found the layouts positioned in
pixels because the only containers are `LinearLayout` and `FrameLayout`:

> Layouts are positioned in pixels because the only containers are Linear and Frame.
> `limit_card.xml` places every label with `layout_marginLeft`/`marginTop` and calls itself
> "pixel art"; the calculator computes key widths by hand; picoclock uses `setPosition`.
> ConstraintLayout is rightly out, but two Android containers map almost directly onto what
> LVGL already has: GridLayout onto LVGL's grid layout (keypads, picoenvmon tiles, the
> launcher) and RelativeLayout's alignParent*/below/toRightOf/centerInParent onto
> `lv_obj_align_to`. Per-child `layout_gravity` inside a LinearLayout is the other frequent
> casualty.

What was true when the work started, verified in source:

- `LinearLayout` is an LVGL flex container (`lvgl/widgets/linear_layout.rs`); `weight` is
  `lv_obj_set_flex_grow(round(weight × 10))`, a `u8`, so weights cap at 25.5 and `1.05 : 1`
  rounds to `10 : 10`. A child's `LayoutParams.gravity` is stored and never applied: LVGL flex
  has one cross-axis placement per container (`lv_flex.c`, `cross_place`), the packer warns, and
  the apps wrap the one child that needs its own placement in a `FrameLayout`.
- `FrameLayout` is a plain `lv_obj`; each child is placed once with `lv_obj_align` from its
  gravity and margins.
- `LV_USE_GRID` was 0 (`crates/pd-lvgl-sys/lvgl/lv_conf.h`); `lv_grid.c` is in the compile list
  and the linker dropped it.
- [app-portability-2026-10.md](app-portability-2026-10.md) D4 and §7 listed ConstraintLayout,
  RelativeLayout, GridLayout and per-child `layout_gravity` as not done ("no LVGL fork").
  This design reverses that for the last three; the amendment is at the bottom of that doc.
- The calculator computes `keyWidth`/`keyHeight` and builds five `LinearLayout` rows;
  `picoclock/ui/Ui.java` is a `setPosition` library with `columnX()`; `limit_card.xml` is a
  `FrameLayout` of absolute margins; picoenvmon's Live page lines up two columns with fixed
  96/112 px widths. The launcher is a list, not a grid: not a target.

## 2. Android's behaviour

- **`GridLayout`** (`android.widget.GridLayout`): `columnCount`/`rowCount`, children placed
  row-major (or column-major under `orientation=vertical`) when they name no cell, a `Spec` per
  axis (`spec(start, size, alignment, weight)`), `layout_gravity` as the cell alignment, weights
  sharing the free space of a track. `match_parent` is meaningless in a cell; the idiom is
  `layout_columnWeight="1"` + `layout_gravity="fill"`.
- **`RelativeLayout`**: a child positioned by rules against the parent's edges
  (`alignParentLeft`…), the parent's centre (`centerInParent`, `centerHorizontal`,
  `centerVertical`) or a sibling (`above`, `below`, `toLeftOf`, `toRightOf`, `alignLeft`…,
  `alignBaseline`), resolved in dependency order per axis; a child constrained on both edges
  with `match_parent` is stretched between them; `wrap_content` on the container is a
  two-pass measure. `setGravity` places the group of children.
- **`LinearLayout`**: children stacked along the orientation with their margins; `weight`
  shares the *remaining* space (`measured + weight / sum × delta`, delta may be negative, which
  is why `match_parent` + weight splits evenly); the cross axis places each child by its own
  `layout_gravity` (a horizontal layout reads only the vertical bits and vice versa), else the
  container's gravity; `fill` stretches.

## 3. Decisions

- **D1 GridLayout is LVGL's grid.** `LV_USE_GRID` goes to 1 on every board. Java keeps
  Android's placement cursor (`validateLayoutParams`) and places a cell with one native per
  child; the native keeps the column and row templates per grid in a small static table whose
  arrays LVGL points at (it does not copy them). A track is `LV_GRID_CONTENT` unless a child in
  it has a weight, then `LV_GRID_FR(round(weight × 10))`, clamped to 1..=99 because
  `LV_GRID_FR(100)` is the template terminator; a weighted span raises every track it covers;
  the largest weight on a track wins. Cell alignment: the spec's, else `layout_gravity`, else
  `FILL` for a `match_parent` axis, else `START`; `FILL` is `LV_GRID_ALIGN_STRETCH`, so a
  `match_parent` axis is sent to LVGL as content-sized and the cell does the stretching.
  Divergences: placement is fixed when the child is added (`removeView` does not free a cell;
  `addView(child, index)` does not reflow); `setColumnCount`, `setRowCount` and
  `setOrientation` after the first child throw `IllegalStateException`; 16 tracks per axis and
  6 live grids; `BASELINE` reads as `TOP`; `setSpacing` is a picodroid addition, as
  `LinearLayout.setSpacing` is.
- **D2 RelativeLayout and LinearLayout are registered LVGL layouts.** `lv_layout_create`
  registers a Rust `layout_update_cb` and `get_min_size_cb`; LVGL calls them whenever the
  container is dirty, exactly as it calls its own flex and grid. The callback positions with
  `lv_obj_move_to` and stretches with `lv_obj_set_width/height` (the flex-private
  `w_layout` bits are not reachable without a fork). `lv_obj_align_to` was rejected: it is
  computed once, has no answer for a `wrap_content` container, and re-running it on every size
  change reinvents the dependency order. A post-flex `translate` correction was rejected:
  translations carry layout flags, flex folds them into its positions and `getLeft/getTop`
  subtract them, and the re-dirty loop runs to LVGL's 100-pass assert.
- **D3 Per-child layout state lives in LVGL custom style properties.**
  `lv_style_register_prop` gives this crate its own property ids; a child's gravity, weight
  (× 1000), view id, relative-rule flags and anchor ids are local style values on the child,
  read back with `lv_obj_get_local_style_prop`. No static pool, no caps, no delete hooks, and
  the values follow a view that is reparented. A size-intent property records `match_parent`
  per axis when `setSize` sees it, because a solver that stretches a child overwrites the
  style width the intent used to live in.
- **D4 The solvers are pure.** `graphics/lvgl/layouts/{relative,linear}.rs` take sizes,
  margins and rules and return rectangles; the LVGL callback is a thin adapter. The Android
  arithmetic (dependency passes, the two-pass `wrap_content`, the weight delta) is unit-tested
  on the host without LVGL, as `gravity.rs` is.
- **D5 LinearLayout moves off flex for every instance.** Android's weight semantics and
  `fill`, no `u8` weight cap, and per-child gravity come together; LVGL flex stays for the
  snackbar, the time picker and the alert dialog, which use it directly. The gate is a
  before/after screenshot comparison of the flagship apps in the simulator; a ±1 px centring
  difference from Android's integer rounding is accepted, anything else is a bug.
- **D6 XML.** `<GridLayout>` and `<RelativeLayout>` with their Android attributes. Grid cell
  attributes are deferred to Java (the child then joins through `GridLayout.addView`, as a
  custom view does); relative rules are nine words (the booleans folded into one flags word,
  `start`/`end` folded into `left`/`right`) and applied on the native inflate path.

## 4. What lands, per commit

Series 1, GridLayout:

1. `lvgl: enable LV_USE_GRID and bind the grid API` — this doc, the bindings, the header
   drift test, the size measurement below.
2. `graphics: GridLayout container and cell placement natives`.
3. `inflater: <GridLayout> in XML layouts`.
4. `layoutdemo: a GridLayout section`.
5. `calculator: the keypad as a GridLayout`.
6. `picoenvmon: Live tiles as a GridLayout`.

Series 2, RelativeLayout: the custom-layout bindings; the container, the solver and the
rules natives; the XML attributes; picoclock, `alarm_card.xml`, claudeusage.

Series 3, LinearLayout: the solver, per-child gravity and Android weights; layoutdemo
checks; the `FrameLayout` wrappers in the apps.

## 5. Verification

Per commit: `./scripts/pre-commit` and `./scripts/sim.sh --app helloworld`. Per series:
`layoutdemo` on the four geometries (`=== ALL PASSED ===`, `[layout] fit ok`), the
`calculator`, `picoclock`, `claudeusage` and `picoenvmon` rows of `scripts/hil-tests.conf`,
the size ratchet on both testbench boards, and for series 3 the screenshot comparison.

Sizes (release, rustc 1.98.1, `parity-bench.sh --size-only`):

| Commit | testbench_rp2040 flash | testbench_rp2350 flash |
|---|---|---|
| baseline `e02bae5d` | 1,029,244 | 1,507,236 |
| 1. `LV_USE_GRID 1` (lv_grid.c linked) | 1,032,112 (+2,868) | 1,510,012 (+2,776) |

## 6. Not doing

ConstraintLayout; `ALIGN_BASELINE`; RTL (`START`/`END` are `LEFT`/`RIGHT`, as `Gravity`
already does); a grid that reflows after `removeView`; `setId` after `addView` into a
`RelativeLayout`; a GONE anchor's rule chain-through; a `match_parent` child contributing to a
`wrap_content` axis.
