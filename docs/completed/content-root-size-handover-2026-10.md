# Handover: a content root's default size, and the clip the fit check does not see — 2026-10-09

**For:** the session that fixes these two. **Found while:** fixing R5 of
[qa-app-portability-round2-2026-10-results.md](qa-app-portability-round2-2026-10-results.md)
(commit `b6662cdc`, which worked around both in the demos; neither is fixed in the framework).
**Against:** `main` at `778aeaf5`. Simulator only; nothing here touched a board.

> **Fixed 2026-10-09**, same day, both in one commit on top of `778aeaf5`. B2: `display.rs::reach`
> carries the clip per axis and the size of the layout that makes it; the line now reads
> `content is cut … (by a 160x160 layout)` and `fit ok 160x160 in 320x240 (the root stops 160 short
> of the right edge, 80 short of the bottom)` (clearer than the `leaves 160x80` form proposed
> below, which read as a rectangle). B1: `View.setSize` is Java over `nativeSetSize` and records
> `isSized()`; `Activity.setContentView(View)` gives an unsized root its layout params' size or
> `MATCH_PARENT` both ways, and the `(View, LayoutParams)` overload exists. keynav and callbacktest
> lost their `b6662cdc` workaround and keynav's sim row asserts `fit ok 320x240 in 320x240`. The
> sweep: of the 48 files only alarmdemo (2), askclaude, powerdemo, connectivity, pagerdemo's
> cover, fragmentdemo's second, injectdemo (2), reclaimdemo, keynav and callbacktest left a root
> unsized; each captured on `pico_display2_w` and read, askclaude's prompt and reply given the
> window's width while there. The guide's sentence is replaced by the two line forms.

Both are one symptom seen from two sides: an app whose content root is smaller than the window.
The first is why that happens without the app asking for it; the second is why the layout line
does not say so.

## Summary

| # | Severity | What |
|---|---|---|
| B1 | **medium** | A content view made in code without a size — `new LinearLayout()`, `new ScrollView()`, `setContentView(root)` — is LVGL's default 160×160, not the window. Android gives a content view `MATCH_PARENT` both ways. `setLayoutParams` on it does nothing, silently |
| B2 | low | `[layout]` measures the tree against the *window's* edge only. A root (or any non-scrolling layout) smaller than the window clips its own children, and the line reports only what also passes the window: keynav hid 92 px and the line said 12; a 160×160 ScrollView root with two thirds of the panel blank said `fit ok 160x160 in 320x240` |

## B1 — a code-built content root is 160×160

### What happens

`LinearLayout()` / `ScrollView()` / `FrameLayout()` call `nativeCreate()` with no size, so the
object has LVGL's default (`lv_obj_class.width_def/height_def` = `LV_DPI_DEF`,
`third_party/lvgl/src/core/lv_obj.c:352`; 160 in `crates/build_support/lvgl.rs:113`). `Activity.setContentView(View)` (`Activity.java:631`) and
`display.rs::set_content_view` (line 114) re-parent it to the screen and reset the pan; neither
sizes it. The XML path is fine: the inflater applies the root tag's `layout_width/height`, which
is why resdemo's `match_parent root` check passes and why the guide's advice ("`match_parent` on
the root", `every-board.md:30`) reads as if it were the default.

`View.setLayoutParams` (`View.java:946`) only records the params for a parent's `addView` to
read. On a content root there is no parent, so `root.setLayoutParams(new LayoutParams(MATCH_PARENT,
MATCH_PARENT))` — the first thing an Android developer writes — changes nothing, and the
`[layout]` line (B2) does not say so either. `setSize(MATCH_PARENT, MATCH_PARENT)` is what works.

### Evidence

- keynav before `b6662cdc`: `[layout] overflow 160x160 in 320x240: … 12 past the bottom` with the
  ScrollView, the clickable text and `Done` invisible (round 2 `shots/a5-keynav.png`). The root
  was `new LinearLayout(this)` with no size. `setLayoutParams(MATCH_PARENT, MATCH_PARENT)` on it
  was tried first and left the line at `fit ok 160x160 in 320x240`; `setSize` fixed it.
- callbacktest, first attempt at R5: `new ScrollView()` around the column read
  `fit ok 160x160 in 320x240` — a 160×160 scroll area in the top-left corner, the rest of the
  panel blank. `scroll.setSize(MATCH_PARENT, MATCH_PARENT)` fixed it.
- menudemo, dialogdemo and most code-built demos avoid it only because their `onCreate` calls
  `root.setSize(MATCH_PARENT, MATCH_PARENT)` by hand.

### Fix

Mirror Android in `Activity.setContentView(View root)`: a content view fills the window unless
told otherwise.

1. If `root.getLayoutParams()` is non-null, apply its `width`/`height` through `setSize` (this is
   what `setContentView(View, LayoutParams)` does on Android; add that overload while there).
2. Otherwise, if the root was never given an explicit size, `setSize(MATCH_PARENT, MATCH_PARENT)`.
   `View` does not record whether `setSize` was called; add a boolean set by `setSize` (and by
   the inflater's size attributes) rather than reading the native size back, so an app that
   deliberately asked for 160×160 keeps it.

Do it in Java, not in `display.rs`: the natives have no view of `LayoutParams`, and a windowed
app (`<supports-screens>`) wants the window's 100 %, which is what `MATCH_PARENT` already means
there.

### What it can break

Every code-built example that never sized its root and happens to look right at 160×160. Find
them with

```bash
grep -L "setSize(.*MATCH_PARENT\|match_parent" $(grep -rl "setContentView(" examples/*/java system-apps 2>/dev/null)
```

(48 files today, most of them single-Activity demos) and read each: a root that is a `LinearLayout` gets wider (children with `MATCH_PARENT` width
stretch to the panel; centred ones move), a `FrameLayout` root's gravity now works against the
window. Re-run the four-panel capture matrix from the round-1 scripts
(`.claude/worktrees/qa2/shots/_scripts/batch-{enviro,tk,tb,d2w}.sh`, one capture per app and
board) and compare against `build/qa/app-portability-round2-2026-10/shots/`. The sim rows that
assert `fit ok` (layoutdemo, calculator, picoenvmon, claudeusage, picoclock, weather) are the
regression gate; add one that builds its root in code with no size and asserts
`fit ok 320x240 in 320x240` on `pico_display2_w`.

Document it in `every-board.md` (the "`match_parent` on the root" bullet becomes "the default; a
root you size yourself is clipped at that size") and in the `View.setLayoutParams` Javadoc ("on a
content view, use `setSize`" until step 1 lands, then nothing).

## B2 — the fit check does not see a clip inside the window

### What happens

`display.rs::fit_check_after_tick` (line 160) computes two things: the window's scroll extent
(the "pans" form, for an oversized root) and `reach(root, 0)` (line 243), the farthest right and
bottom edge of any shown, non-floating descendant, stopping at the first `LV_OBJ_FLAG_SCROLLABLE`
object. The cut it reports is `reach − window edge`. Every picodroid layout clips its children
(LVGL's default; no `OVERFLOW_VISIBLE`), so a child that passes its non-scrolling parent's edge is
cut *there*, whether or not it also passes the window. The walk never compares a child against
its parent.

Two false readings follow:

- a non-scrolling root (or inner layout) smaller than the window: the content between the
  layout's edge and the window's edge is invisible and uncounted — keynav's 92 px (the line said
  12, the part past the window);
- a scrollable root smaller than the window: the walk stops at it and says `fit ok <small> in
  <window>`, which is true of the clip and silent about the two thirds of the panel left blank.

The guide carries a sentence on this since `b6662cdc` (`every-board.md:51-54`); it should go
once the line says it.

### Fix

In `reach`, carry the clip as well as the reach: for a non-scrollable `obj`, after the children
are walked, `clip = max(clip, child_reach − obj's edge)` on each axis (a child that ends inside
its parent contributes nothing). Return `(reach_x, reach_y, clip_x, clip_y)`. Then:

- report `cut` as `max(reach − window, clip)` per axis, so the line names the whole of what is
  not visible, and add the cause when it is a layout rather than the window:
  `content is cut 0 past the right edge, 92 past the bottom (by a 160x160 layout)`;
- when the root itself is smaller than the window on either axis and is not `MATCH_PARENT`,
  say so in the `fit ok` form too: `fit ok 160x160 in 320x240 (the root leaves 160x80 of the
  window empty)` — this is the B1 symptom and worth a line even after B1 is fixed, for a root an
  app sized itself.

Keep the walk's shape: depth cap 16, stop at scrollables, skip hidden and floating children;
`clip` for a scrollable child is zero by definition (its content is reached by scrolling it). The
cost is one `lv_obj_get_coords` per visited object, already paid.

### Checking it

- keynav at `b6662cdc^` (root 160×160, `git show b6662cdc^:examples/keynav/java/keynav/KeyNavActivity.java`
  into a scratch copy, or simply drop its `setSize` locally): expect `content is cut … 92 past
  the bottom (by a 160x160 layout)`, then `fit ok 320x240` with `setSize` back.
- callbacktest with its ScrollView at the default size: expect the `fit ok … leaves … empty` form.
- The rows that assert the exact `fit ok WxH in WxH` text (`grep -n "fit ok" scripts/hil-tests.conf`)
  must still match: keep the first clause byte-identical and append the parenthesis.
- The sentence in `every-board.md:51-54` is replaced by the two new line forms in the code block
  above it.

## Order

B2 first (half a day: a few lines in `reach`, the two line forms, the guide), because it makes
B1's sweep self-checking: every example then says in its own log whether its root is the window.
Then B1 with the grep above, the capture matrix and the new row. One commit each,
`./scripts/pre-commit`; B1 is Java only, B2 is Rust under `cfg(any(feature = "sim",
debug_assertions))`, so neither moves the size ratchet on a release image.
