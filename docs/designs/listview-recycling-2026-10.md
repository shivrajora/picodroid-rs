# ListView recycles: a window of rows behind getView/convertView

**Status: built 2026-10-20 — the window (§3), the natives (§4), the keypad order (§5), the
consumers (§6); verification in §7.**

## 1. The finding

The 2026-10 gap analysis ranked this above any missing class:

> ListView should recycle, as Android's does. picoenvmon shows 12 of 60 history rows because
> every row stays alive in the 48 KB LVGL pool; settings builds row chains for the same reason.
> Android's ListView recycled rows out of view long before RecyclerView. Virtualising rows
> behind the existing getView/convertView API removes the main reason RecyclerView keeps coming
> up, without adopting its API.

What was true when the work started: `ListView.refreshFromAdapter` called `getView` for every
position and kept every row as a live LVGL child; `convertView` was only ever the row at the same
position, on a `notifyDataSetChanged()`. picoenvmon's History capped itself at `MAX_ROWS = 12` of
a 60-sample ring, the time-zone picker kept 38 rows alive, and the website told apps to keep
adapter-backed lists to "a dozen rows". `RecyclerView` was listed as missing with the note
"ListView builds every row; no scrap pool". The 2026-10-09 getView/convertView work
(`java-io-streams-2026-10.md`) had left virtualisation as its follow-up and named the hard part:
keypad focus and the scroll position under re-binding.

## 2. The shape

Java owns the window; Rust owns the LVGL geometry and queues the events.

- **The window.** `ListView` keeps its rows as children in position order and knows `mFirst`,
  the adapter position of child 0, and `mPitch`, one row's height. The window holds
  `max(8, ceil(height / pitch) + 1 + 2 × OVERSCAN)` rows with `OVERSCAN = 1`, capped at the
  adapter's count. A list of eight items or fewer (`MIN_WINDOW`) is built whole, measures
  nothing, watches nothing and pads nothing: every menu-sized list is byte-for-byte what it was.
- **Padding stands in for the rows outside the window**: `pad_top = first × pitch`,
  `pad_bottom = (count − first − live) × pitch`. The default theme's `list_bg` has `pad_ver 0`
  and `pad_gap 0`, which `create()` now pins explicitly, so a row for position *p* sits at
  `p × pitch` whatever the window holds, and moving the window never shifts a pixel the user
  sees. No spacer children, so `lv_obj_get_index(row)` stays the window slot the click
  trampoline reports.
- **Scroll events are deferred** like every LVGL callback here: `LV_EVENT_SCROLL` and
  `LV_EVENT_SIZE_CHANGED` on the list push its handle into a de-duplicated ring; the framework
  loop drains it after `g.tick()` into `ListView.fireScroll()` (a new append-only
  `dispatch_sites` row, drained right after the item clicks so a click's slot resolves against
  the window LVGL saw). Java computes `first = clamp(scrollY / pitch − OVERSCAN, 0, count −
  window)` and, when it changed, slides: each row leaving one edge is moved to the other
  (`nativeRecycleRow`), re-attached in the Java child list (`detachViewFromParent` /
  `attachViewToParent`, Android's own recycling primitives, now on `ViewGroup`) and re-bound
  through `getView(position, row, this)`. A jump wider than the window re-binds every row in
  place. One interpreter call per frame while a long list scrolls; nothing when idle, nothing
  for a short list.
- **Rows share one height.** The first row's `getHeight()` (which runs LVGL's layout pass) is
  the pitch, and every later row is pinned to it by `nativeStyleRow(row, height)`. Android lets
  each row choose its own height; here a taller row is clipped. Every in-tree adapter has
  uniform rows.
- **A list sized late** (a weighted child of a `LinearLayout` at `setAdapter` time in
  `onCreate`) starts with the minimum window; the size-changed event re-evaluates it. LVGL
  re-clamps the scroll offset after any layout, so a shrinking `pad_bottom` needs no clamp of
  ours.

## 3. Java (`sdk/java/picodroid/`)

`widget/ListView.java`: `mRows`/`mRowCount` are gone (`getChildAt(i)` is slot *i*);
`refreshFromAdapter` builds or re-binds the window, `settle` moves it, `slide` recycles row by
row, `bindSlot` is the one place rows join or leave the child list. `fireItemClick(slot)` adds
`mFirst`. New: `setSelection`, `smoothScrollToPosition` (`setSelection` is synchronous, the
smooth form follows the frames), the `AdapterView` getters `getFirstVisiblePosition`,
`getLastVisiblePosition`, `getPositionForView`, `getCount`, `INVALID_POSITION`.
`view/ViewGroup.java`: `detachViewFromParent`, `attachViewToParent` (protected, Java-side
bookkeeping plus the widget move; the widget never leaves its parent), and a fix:
`addView(existingChild, lastIndex)` never reordered the widget, because `lv_obj_set_parent`
returns early for an unchanged parent and the move was skipped for the last index.

## 4. Rust (`crates/picodroid-core`, `crates/pd-lvgl-sys`)

`graphics/lvgl/widgets/list_view.rs`: the scroll ring and its callback, `watch_viewport`,
`scroll_y`, `scroll_to_y`, `set_spacers`, `recycle_row` (focus hand-off, then
`lv_obj_move_to_index`), `sync_focus_order`; `style_row` takes the height and is idempotent
(`lv_obj_remove_event_cb` before `add_event_cb`). `graphics/widgets/list_view.rs` has the shims;
`method_tables.rs`, `lvgl_backend.rs`, `dispatch_sites.rs` (`LIST_VIEW_SCROLL`) and
`lifecycle/widget_events.rs` (`dispatch_list_view_scrolls`) the plumbing. `pd-lvgl-sys` binds
`lv_group_swap_obj` and `lv_group_get_obj_by_index`.

## 5. The keypad group

A keys board walks the Activity's LVGL group in insertion order, so a row recycled from the top
of the window to the bottom, or a replacement row the adapter built for a middle slot, lands in
the group where it was added, not where it sits. `graphics/lvgl/focus_order.rs` (pure,
host-tested) puts the rows back: the lowest group index any row holds is the run's base, and
row *i* is walked into slot `base + i` by adjacent swaps, which keep the relative order of the
widgets outside the run (a button below the list). `nativeSyncFocusOrder` runs it after every
slide and refresh; it also fixes the latent order bug of a row appended by a growing
`notifyDataSetChanged`. `lv_group_swap_obj` swaps the focus along with the positions, so the
sync holds the focused object: the rows' `SCROLL_ON_FOCUS` is cleared for its duration and the
object that was focused is focused again at the end (without that, one DOWN press walked the
focus 110 rows: swap, focus hop, scroll, slide, swap). DOWN on the last visible row focuses the overscan row below it, LVGL
scrolls it into view (`row_focus_cb`), the scroll event slides the window. That flag is
`lv_button`'s, which the native text rows are; an adapter row is a label or a layout and never
had it, so since the 2026-10-09 adapter rows DOWN past the last visible row wrapped the focus to
the top instead of scrolling (qa_list found it: twelve DOWNs on a ten-row window clicked position
2). The flag itself cannot be the fix: LVGL's scroll-into-view counts the parent's padding as a
viewport inset, and the bottom padding of a recycling list is the rest of the content, so one
DOWN press scrolled to row 186. Rows get a `FOCUSED` callback of this file's instead
(`scroll_row_into_view`, from the row's and the list's coordinates), held off while the window's
own bookkeeping hands the focus on. If a touch scroll
pushes the focused row itself out, `recycle_row` hands the focus to its neighbour with
`SCROLL_ON_FOCUS` cleared around the call, so nothing fights the scroll in progress. On a board
without a group all of this is a no-op.

Rejected: a Rust-owned per-list window (a keyed table with no delete hook and an upcall per
row); spacer children (every child index shifts); taking rows out of the group and routing
PREV/NEXT through `edit_mode.rs` (a keypad redesign for one widget); `RecyclerView`
(`android-parity-roadmap-2026-08.md`, "not doing").

## 6. Consumers

- picoenvmon History (Java and Kotlin): `MAX_ROWS` and `firstShown` gone, every sample a row,
  `setSelection(count − 1)` opens on the newest, the click uses the position directly, and the
  log line `History rows <live> of <samples>`.
- The time-zone picker and tutorial_service's log viewer are unchanged and now cost a window.
- `qa_ui` gains `listRecycling`; `examples/qa_list` is new (200 items, driven by `test.ctrl`
  on a touch board and a keys board; two `hil-tests.conf` rows).

## 7. Verified (2026-10-20, simulator)

- `qa_ui` (`listRecycling`, both shrink modes, 282 checks green): a 60-row two-line adapter in a
  200×120 list keeps an 8-row window and builds exactly 8 views; `setSelection(30)` moves the
  window without a build (`rebound > 0`); a notify re-binds in place; shrinking the adapter to
  20 clamps the window to the end, to 5 rebuilds it whole; a `performClick` on slot 2 after
  `setSelection(45)` reports position 46; `smoothScrollToPosition(59)` ends with
  `getLastVisiblePosition() == 59`.
- `qa_list` (200 items, 320×240): 12 rows built at `setAdapter` (the list measured at its
  pre-layout size), 10 alive once laid out; a drag of four rows then a tap reports
  `click ok position=9 first=6 … rebound=16`; twelve paced DOWN presses and ENTER on
  `pico_display2_w` click the focused row past the first window, text matching the item.
- `focus_order.rs`: 8 host tests (moves to either end, a replacement row appended after a
  trailing button, two trailing widgets keeping their order, rows outside the group, no group).
- The flash delta is in the devlog entry and the ratchet commit.
- Not yet verified on a board: History on `pico_enviro_mon` with a full ring (A/B through 60
  rows, X on the highlighted sample); the touch kit's 64 KB pool is not at issue.
