# QA results: apps on every board — 2026-10-08

**For:** the session that fixes what this one found. **Against:** `main` at `90406245` (v0.36.0;
the twelve stage commits `182505e8..853db847` plus the release and ratchet commits), the plan in
[qa-app-portability-2026-10.md](qa-app-portability-2026-10.md), the design in
[designs/app-portability-2026-10.md](designs/app-portability-2026-10.md). **Where:** the
simulator only, from the worktree `.claude/worktrees/qa` (branch `qa-portability`, same tree as
`main`, nothing committed). Nothing here touched a board.

Every capture and its simulator log are under `.claude/worktrees/qa/shots/` (`<name>.png` and
`<name>.log`; the helper scripts that made them under `shots/_scripts/`, the repacked PAPKs
beside them; a copy of the whole `shots/` tree is in the main checkout under
`build/qa/app-portability-2026-10/shots/`, gitignored). The side-by-side page of the same apps on the four panels is
<https://claude.ai/artifact/JB52NAemNHfJ68LgxPq5m1>; the montages behind it are
`shots/montage-<app>.png`. Panel captures at 240×240 and 320×240 are at 2× (a coordinate read off
them is half of what you see), the 320×480 touch kit at 1×.

## Summary

Stages 1–12 do what the design says on all four geometries: the dialog card, the soft-nav and
menu controls, BACK-hold HOME, doze and wake, the layout vocabulary, every stock widget by four
keys, the fit log, design-size windows (letterbox, pan, touch offset), the installer's feature
gate, `Configuration` and the keys, variants, held keys and the options menu. The sim-run rows
the plan names all pass — 56 rows, 0 failures (table below). Twelve defects, two of them worth fixing before the next
release:

| # | Severity | What |
|---|---|---|
| F1 | **high** | A windowed app crashes the simulator with an OOM on BACK from a child Activity (picoclock on both 320×240 boards; fine on the touch kit) |
| F2 | **high** | The screen's pan offset survives an Activity change in a windowed app: the next screen opens scrolled, showing its blank lower half |
| F3 | medium | `[layout] fit` reports `fit ok` while children of a `match_parent` root are cut by the panel edge |
| F4 | medium | The snackbar does not fit 240 wide: the action is clipped and LVGL draws a scrollbar inside it |
| F5 | medium | A focused Button shows no focus border on a keys board (`requestFocus()` is invisible; DOWN shows only LVGL's clipped outline) |
| F6 | low | The launcher draws a keypad focus ring on the touch kit, which has no navigation keys |
| F7 | low | A disabled `MenuItem` is drawn enabled; SELECT on it closes the menu on a keys board, a tap leaves it open on a touch board |
| F8 | low | Press-outside never dismisses the keyboard inside a design-size window, nor on a tap inside the app's root; keyboarddemo's "Tap me to dismiss" hint does nothing |
| F9 | low | The menu control stays visible and tappable while the menu list is open |
| F10 | low | Overlay controls sit on app content with no inset for the app (calculator's `0`, picoclock's `Alarms`, resdemo's rows, claudeusage's `next`) |
| F11 | docs | `compatibility-matrix.md` lists `Menu` twice: Partial (line 54) and "Unsupported — No menu resources or options menu" (line 59) |
| F12 | plan | Two plan inaccuracies: Q1.3's tap `120 40` misses the field (use `120 55`); Q9.2's "row invisible until a key" is not what resdemo does |

## Defects

### F1 — OOM on BACK from a child Activity in a windowed app

picoclock (design size 320×480) on a 320×240 board: open `Alarms` or `Set time`, press BACK. The
simulator dies:

```text
activity: pop picoclock/ui/AlarmListActivity
[sim] View.close() on nativeHandle 1068 while its widget still sits under a Java-owned container: the parent's child list would keep the freed subtree alive (D3)
[sim] OOM: tried 196608 B — free 140616 B, largest block 86040 B, 30 free blocks, min-ever-free 91456 B, arena 417792 B
memory allocation of 196608 bytes failed
```

- Reproduces on `testbench_rp2350` through the soft-nav control, through `input back`, after
  `Set time` as well (`shots/lh-clock-alarms-back.log`, `shots/oom-tb-alarms-softback.log`,
  `shots/oom-tb-settime-back.log`), and on `pico_display2_w` by keys — DOWN, DOWN, SELECT, BACK
  (`shots/oom-d2w-alarms-back.log`). Every time the same handle 1068, the same 196,608 B
  (= 384×256×2; a canvas or draw buffer of the clock face being re-created while the old one is
  still held).
- Does **not** reproduce on `pico_touch_kit`, picoclock's own panel, where the app is not
  windowed and the arena is *smaller* (336 KB vs 408 KB): `Alarms` → BACK twice in a row is clean
  (`shots/oom-tk-alarms-back-twice.log`, `shots/oom-tk-alarms-back.png` shows the clock back).
  So this is the window path (`graphics/lvgl/window.rs`, `lifecycle::screen_ptr` /
  `screen_handle` from A4), not picoclock: the D3 line says a closed view's widget still sits
  under a Java-owned container, so the previous ClockActivity tree is kept alive under the window
  object when the Activity is re-created, and the next allocation of the face has no room.
- Where to look: `View.close()` / the D3 check in the view-ops path, and how `window.rs` parents
  content roots (`content_root()`) versus how `Activity` teardown detaches them. The sim row
  `picoclock|sim|…|testbench_rp2350` only asserts startup lines, so add a row that pushes and pops
  a child Activity in the windowed configuration (picoclock has no `test.ctrl`; a drag plus a tap
  plus `input back` is enough, see `shots/_scripts/batch-oom.sh`).

Repro, 20 s:

```bash
./scripts/sim-shot.sh shots/oom.png 10 --app picoclock --board testbench_rp2350 \
  -- "input swipe 160 200 160 40 300" "input tap 100 200" "input back"
grep -n -E "D3\)|OOM" shots/oom.log
```

### F2 — the pan offset survives an Activity change (windowed app)

Same setup. After the drag that reveals `Alarms`, tapping it opens `AlarmListActivity` already
scrolled 240 px down: the panel shows the list's empty lower half and a scrollbar
(`shots/lh-clock-alarms-body.png`); a drag back down reveals its header and `+ New alarm`
(`shots/lh-clock-alarms-top.png`). `Set time` opens on the lower rows of its calendar with the
header off-screen (`shots/lh-clock-settime.png`). On the keys board the focus-on-scroll pan has
the same effect (`shots/lh-clock-keys-d2w-select.png` is the Alarms screen opened at the right
place only because the focus landed on its first row). Log: `[layout] fit ok 320x480 in 320x480`
for the new screen — the check does not see the pan either.

The screen's scroll position belongs to the window object (A4: the window is an object on the
panel-sized screen, "which pans"); nothing resets it on `setContentView` or on an Activity push /
pop. Android starts every Activity at its own origin. Fix: scroll the screen (and the window
object) back to `(0,0)` when a content root is replaced — `display.rs::set_content_view`, or
where `window::sync()` reparents the root — and consider the same on resume after a pop.

### F3 — `[layout] fit ok` while content is cut

`display.rs::fit_check_after_tick` measures `lv_obj_get_scroll_right/bottom` of the screen (or
the window object), i.e. only the root overflowing its parent. The guide tells apps to use a
`match_parent` root, and such a root never overflows — its *children* do, inside it, and the
check says `fit ok`. resdemo on `pico_enviro_mon`: the second `An inflated row` is cut by the
bottom edge (`shots/q9-res-pico_enviro_mon.png`, zoom in `shots/_scripts/../z-res-bottom.png`
of the scratchpad) with `[layout] fit ok 240x240 in 240x240` in its log. layoutdemo's second
card ends on the 240th row by one pixel, so it is not evidence, but it is one more row away from
the same silence.

Fix direction: walk the root's children (or read the root's own `lv_obj_get_scroll_bottom/right`
when it is a scroll container, and the deepest overflow when it is not) and report the overflow
against the window as the design's D6 intends. A single-level `match_parent` root with
overflowing children is the common porting mistake the line exists to catch.

### F4 — the snackbar at 240 wide

`shots/q1-snack-enviro-keys.png` (snackbardemo, `pico_enviro_mon`, raised by DOWN DOWN SELECT):
the bar hugs the bottom at the panel's width, but "Tap RETRY to dismiss" plus the RETRY action
overflow it — the action is cut at the right edge and LVGL draws a horizontal scrollbar inside
the bar. At 320 wide (`shots/q1-snack-d2w.png`, `shots/q1-snack.png` on the touch kit, zoom
`z-snack-right.png`) a vertical scrollbar sliver shows at the right end: the content is a few
pixels taller than the container. `widgets/snackbar.rs` (sized from the display since stage 1):
turn scrolling off on the container (`LV_OBJ_FLAG_SCROLLABLE` clear, scrollbar mode OFF), let the
label take the remaining width with `LV_LABEL_LONG_WRAP` or `DOT`, keep the action at its
natural width, and size the bar's height from its content.

### F5 — a focused Button is invisible on a keys board

`events/groups.rs::ensure_in_group` gives a focusable view a 2 px light border for the `FOCUSED`
and `FOCUS_KEY` states — but only when the view is *not already* in the group. LVGL adds
widgets whose class has `group_def` (button, checkbox, switch, slider, dropdown, roller,
textarea) to the default group at creation, so for every stock widget the branch is skipped and
the border never exists. What the user sees on a four-key board:

- keydemo's `Focus me`, focused by `requestFocus()` and receiving the key events, is a flat blue
  button (`shots/q10-held.png`; zoom in the scratchpad `z-focusme.png`); menudemo's `Focus me`
  and navdemo's `Back to Home` the same.
- `Row 1`, reached by holding DOWN, shows only the theme's `FOCUS_KEY` outline, drawn outside
  the object and clipped by its packed neighbours to two side brackets (`shots/q10-focus.png`,
  `z-row1.png`). The launcher and Settings rows (plain `lv_obj`, added through
  `setFocusable`) get the border and read fine.

Fix: apply the border styles unconditionally in `ensure_in_group` (or at widget creation under
`has_nav_keys`), and consider `lv_obj_add_state(…, LV_STATE_FOCUS_KEY)` on a programmatic
`requestFocus` so the theme's own outline shows too. This is the "shared focus style" A2 left
undone; the evidence says it is needed.

### F6 — a focus ring on the touch kit

`shots/m-launcher-pico_touch_kit.png` shows the first launcher row with the keypad focus ring;
`shots/m-launcher-testbench_rp2350.png` (touch, no buttons) shows none. The touch kit's BACK and
HOME buttons give it a keypad indev and a default group, and `LauncherActivity` calls
`row.setFocusable(true)` (line 208) and `rows[0].requestFocus()` (line 136) without consulting
`isInTouchMode()`, which is true there (`layoutdemo` prints `finger nonav` on that board).
Either guard both in the launcher, or have `set_view_focusable` / the border styles apply only
when `has_nav_keys` (the framework rule for clickable ⇒ focusable already does).

### F7 — a disabled MenuItem

With menudemo patched to `setEnabled(false)` on Refresh and `setVisible(false)` on About
(`shots/_scripts/patch-menudemo.py`, reverted after): the hidden item is gone — two rows
(`shots/lh-menu-hidden-d2w.png`, `lh-menu-hidden-tb.png`) — but the disabled one is drawn like
the enabled one. On the keys board SELECT on it logs `menu closed #1` with no `selected`
(`shots/lh-menu-disabled-pick-d2w.log`); on the testbench a tap on it leaves the list open
(`shots/lh-menu-disabled-tap-tb.png`). Android greys a disabled item and ignores the tap with
the menu still open. Fix in the list presentation (`alert_dialog.rs` items path used by
`OptionsMenu`): `LV_STATE_DISABLED` on the row's button-matrix cell (`LV_BUTTONMATRIX_CTRL_DISABLED`)
and no dismiss on a disabled pick. `invalidateOptionsMenu` was not exercised.

### F8 — press-outside and the keyboard

The dismiss hook sits on `lv_screen_active()` (`events/touch.rs:315`) and fires only for a press
that reaches the screen object:

- a tap beside the app's root, on the bare screen, dismisses (`shots/lh-kbd-dismiss-screen.png`);
- a tap on the app's own container does not (`shots/lh-kbd-dismiss-root.png`), so keyboarddemo's
  "Tap me to dismiss" strip (its comment at `KeyboardDemoActivity.java:34` promises it) does
  nothing;
- in a design-size window the window object takes every press: a tap on the letterbox band
  dismisses (`shots/lh-kbd240-dismiss-band.png`), a tap inside the window beside the root does not
  (`shots/lh-kbd240-dismiss-window.png`). A window that fills or overflows the panel
  (claudeusage on the Enviro+, picoclock on the testbench) has no band, so there the keyboard
  only leaves by BACK or the OK key.

BACK (`key: BACK -> keyboard dismissed`, `shots/lh-kbd-dismiss-back.log`) and OK
(`shots/lh-kbd-dismiss-ok.png`) work, and doze + wake under the keyboard keep it up
(`shots/lh-kbd-doze.png`). Fix: hook the window object too (or the indev's press, filtering the
keyboard and the focused textarea), and either make the demo's strip a real target or correct
its comment.

### F9 — the menu control while the menu is open

`shots/q11-open.png` and `shots/lh-menu-open-pico_touch_kit.png`: the ≡ control is drawn above
the scrim with the list open. A second tap sends another soft MENU, which Android would treat as
a toggle (close). Hide it with the list, as the keyboard does, or make the second tap close.

### F10 — overlay controls on app content

By design (stage 2, A4) the soft-nav and menu controls float over the app, and an app has no way
to know where. On the testbench: the BACK control covers the corner of calculator's `0` key
(`shots/m-calc-testbench_rp2350.png`), picoclock's `Alarms` after the pan
(`shots/q6-pan-after.png`; a tap on that corner is BACK and ends the app,
`shots/lh-clock-alarms-corner.log`), resdemo's rows (`shots/q9-res-testbench_rp2350.png`),
layoutdemo's checkbox (`shots/m-layout-testbench_rp2350.png`), claudeusage's `next`
(`shots/m-cu-testbench_rp2350.png`), picoenvmon's `A:Up` (`shots/m-env-testbench_rp2350.png`).
Not a defect against the design; a follow-up candidate — a `WindowInsets`-like value
(`Display`/`Configuration` reporting the two 46×46 corners) or a bottom inset on the content
root when `soft_nav` is set.

### F11 — the compatibility matrix

`website/src/content/docs/reference/compatibility-matrix.md` has `| Menu | Partial | add …`
at line 54 and `| Menu | Unsupported | No menu resources or options menu. |` at line 59. The
second is the pre-stage-11 row; drop it, or reword it to menu *resources* only. `cd website &&
npm run build` passes (all internal links valid).

### F12 — the plan

- Q1.3: `input tap 120 40` lands 2 px above keyboarddemo's field (top edge y≈42); the keyboard
  did not rise (`shots/q1-kbd.log` from the first run has no keyboard lines). `120 55` works.
- Q9.2: "on notouch the row is invisible until a key" — `ResDemoActivity.java:120` sets the row
  VISIBLE right after the visibility check, on every board; the difference is only in the
  `row attributes` check line. Both boards show the row at once.
- Q10.3: keydemo's `Focus me` has no click listener, so "the release does not click" is only
  observable through menudemo's row (Q11.4, no `clicked` line), which passes.
- Q5.3: Settings → About has focusable rows (Board, MCU, Release, Storage, Heap), so the
  lone-ScrollView paging is not exercised by anything in the tree.

## Notes (not defects)

- The list dialog keeps its full card height for three rows (`shots/q11-open.png`); cosmetic,
  known.
- menudemo's status line is wider than 320 and clips at the panel edge; the demo's own TextView.
- picoenvmon shows `A:Up B:Down X:Open Y:Exit` on the touch boards; the app could read
  `Configuration.navigation`.
- claudeusage on the 240×240 Enviro+: the right 80 px (sync, the plan label, `auto`) are never
  reachable — the app consumes UP/DOWN for pages and has nothing focusable, so focus-on-scroll
  cannot pan (`shots/lh-cu-keys-em.png`); the horizontal scrollbar shows at rest in every Enviro+
  capture. A `layout-w240dp` variant of its chrome would close the gap; A6 left it as is.
- A dialog over a windowed app is centred on the **panel** (dialogdemo repacked `--design-size
  320x240` on the Enviro+: card at panel centre, `shots/lh-dialog320-pico_enviro_mon.png`; keys
  pick a row fine). A4 says so; it looks right because the scrim covers the panel.
- Soft MENU (`input keyevent 82`) opens the menu on the testbench; the BACK control dismisses it
  with `key: BACK -> dialog dismissed` and no `onOptionsMenuClosed` (A6 documents this).
- Holding SELECT across an Activity change (navdemo, `--down 23` opens Detail, `--up 23` on the
  new screen): nothing on Detail clicks or long-presses (`shots/lh-nav-held.log`).
- Doze with a dialog up, wake by key, tap a row: the dialog is there and responds
  (`shots/lh-dialog-doze.log`: `multi 1=true` after `wake #1 (key)`).
- The soft-nav *hold* (BACK held → HOME on a touch board) is bench-only; the control channel
  cannot hold a tap. The keys-board hold passes (Q2.1).
- `invalidateOptionsMenu`, a menu with the keyboard up, and the nightly sweep were not run.

## Scenario results

| Scenario | Board(s) | Result | Evidence | Note |
|---|---|---|---|---|
| Q1.1 dialog | enviro, touch kit, testbench, display2_w | PASS | `q1-dialog-{enviro,tk,tb,d2w}.png`, `[layout] fit ok` | card 200 px, centred; BACK control above the scrim on tb |
| Q1.2 snackbar | touch kit, enviro, display2_w | **FAIL** | `q1-snack.png`, `q1-snack-enviro-keys.png`, `q1-snack-d2w.png` | F4: clipped action and scrollbar at 240; sliver at 320 |
| Q1.3 keyboard | testbench | PASS | `q1-kbd.png` (tap `120 55`) | F12: the plan's `120 40` misses; BACK control hidden under the keyboard |
| Q1.4 theme chrome | four boards | PASS | `montage-dialogdemo.png` | row pitch and button height identical in pixels |
| Q2.1 BACK held → HOME | display2_w | PASS | `q2-home.png`; `key: BACK held -> HOME`, `key: HOME -> launcher`, `KeyDemo] ready`, no finish | |
| Q2.2 soft nav | testbench | PASS | `q2-softnav.log`: `ready for back 1` → `soft nav: tap -> BACK` → `popped 1` | hold is bench-only |
| Q2.3 fragmentdemo rows | testbench, display2_w | PASS | sim-run 4/4 | |
| Q3.1 powerdemo row | testbench | PASS | sim-run 2/2 | |
| Q3.2 doze / wake | testbench | PASS | `q3-dozed.png` black, `display: doze #1 (sleep key)`; `q3-woken.png`, `wake #1 (touch)`, no `PowerDemo] click` | |
| Q3.3 Settings → Display | display2_w | PASS | `q3-settings.png` six rows, first highlighted; `activity: push settings/DisplayActivity`; `q3-settings-pick.png`, `Settings] display timeout 30000` | walk: DOWN, SELECT (launcher) then DOWN×4, SELECT, SELECT |
| Q4.1 layoutdemo rows | four boards + loop | PASS | sim-run | see table below |
| Q4.2 layoutdemo | enviro, display2_w, touch kit, testbench | PASS | `q4-layout-*.png`, `=== ALL PASSED ===`, `fit ok` | no right-edge overflow; the second card ends on row 240 of the floor |
| Q4.3 row centring | display2_w | PASS | `q3-settings-root.png`, `m-launcher-*.png` | |
| Q5.1 keynav row | display2_w | PASS | sim-run | |
| Q5.2 SeekBar / Spinner / list dialog | display2_w | PASS | `q5-seek.png` (edited outline, `seek 49`, `seek 48`), `q5-spinner-open.png` (Red highlighted), `q5-listdialog.png` (Red highlighted) | |
| Q5.3 About paging | display2_w | NOTE | `q5-about.png`, `q5-about-back.log`: `activity: pop settings/AboutActivity` | About has focusable rows; lone-ScrollView paging untested |
| Q6.1 picoenvmon | enviro, display2_w, touch kit (+ testbench) | PASS | `q6-env-*.png`, `fit ok` | hint bar on the bottom edge everywhere |
| Q6.2 picoclock pan | testbench | PASS | `q6-pan-before/after.png`; `window 320x480 @ (0,0) on the 320x240 panel`; `fit ok 320x480 in 320x480` | then F1, F2, F10 |
| Q6.3 calculator / launcher / settings / weather rows | matrix | PASS | sim-run | see table below |
| Q6.4 calculator | enviro, touch kit | PASS | `q6-calc-*.png` | keypad fills; taller keys on the tall panel |
| Q7.1 letterbox | touch kit | PASS | `q7-cu-tk.png`; `window 320x240 @ (0,120) on the 320x480 panel` | no controls on the touch kit |
| Q7.2 pan | enviro | PASS | `q7-cu-em.png`; `window 320x240 @ (0,0) on the 240x240 panel`; `fit ok 320x240 in 320x240` | h-scrollbar shown; right 80 px off |
| Q7.3 tap in the window | testbench | PASS | `q7-tap.png`; `window 240x240 @ (40,0)`; exactly 2 `[CBT] BUTTON` | |
| Q7.4 installer gate | display2_w, testbench | PASS | `q7-gate.log`: `apps: install refused: requires a feature this board lacks (uses-feature)`, launcher up; `q7-gate-tb.log`: `apps: installed dragdemo (6436 bytes)` | |
| Q7.5 papk-info | — | PASS | picoclock `design-width 320`, `design-height 480`; dragdemo `requires-features picodroid.hardware.touchscreen`; claudeusage 320×240 | |
| Q8.1 Configuration | four boards | PASS | `config {sw240dp w240dp h240dp 160dpi port notouch dpad nokeys}` (enviro); `{sw240dp w320dp h240dp … land notouch dpad}` (display2_w); `{sw320dp w320dp h480dp … port finger nonav}` (touch kit); `{sw240dp w320dp h240dp … land finger nonav}` (testbench) | |
| Q8.2 keys | four boards | PASS | `keys back=true home=true` everywhere; `up=true center=true` on enviro and display2_w only | |
| Q8.3 qa_ui row | testbench | PASS | sim-run | |
| Q9.1 resdemo rows | four boards + loop | PASS | sim-run | |
| Q9.2 resdemo | testbench, display2_w (+ enviro, touch kit) | PASS | `[res] 320x240dp land finger: 3 of 3 variants apply`; `[res] 320x240dp land notouch: 2 of 3`; enviro `0 of 3`; touch kit `2 of 3`; `ResDemo PASS` | F12 on the "invisible row"; F3 on the enviro clipping |
| Q9.3 papk-info / values-night | — | PASS | `overrides 3`, `variant-land`, `variant-w320dp`, `variant-finger`; build refused: `'night' is not a supported configuration — … a directory may name sw<N>dp, w<N>dp, h<N>dp, land, port, notouch, finger` | |
| Q10.1 keydemo row | testbench | PASS | sim-run | |
| Q10.2 held DOWN | display2_w | PASS | `q10-focus.png` ring on Row 1 (wrapped); `focus row 1`, `2`, `3`; `activity UP keyCode=20` | F5: the ring is LVGL's clipped outline |
| Q10.3 held SELECT | display2_w | PASS | `q10-held.log`: `view LONGCLICK`, then `view UP keyCode=23`, no click | observable only via Q11.4 (F12) |
| Q11.1 menudemo row | display2_w | PASS | sim-run | |
| Q11.2 touch | testbench | PASS | `q11-control.png` (≡ bottom-right, BACK bottom-left); `q11-open.png`, `soft menu: tap -> MENU`, `menu shown #1`; `q11-pick.png`, `selected Refresh id=1`, `menu closed #1` | F9 |
| Q11.3 no menu, no control | testbench | PASS | `q1-dialog-tb.png` | |
| Q11.4 keys | display2_w | PASS | `q11-keys.png` first row highlighted; `menu shown #1`; no `clicked` | |
| Q11.5 control under the keyboard | — | NOT RUN | | no demo has both |
| Q12 docs | — | **FAIL** | website build: all internal links valid; quoted log lines match | F11 |

## sim-run rows (`./scripts/sim-run.sh --app X --no-email --no-pull`, both shrink modes)

| App | Rows × modes | Result |
|---|---|---|
| fragmentdemo | 2 × 2 | PASS 4 |
| powerdemo | 1 × 2 | PASS 2 |
| keydemo | 1 × 2 | PASS 2 |
| menudemo | 1 × 2 | PASS 2 |
| keynav | 1 × 2 | PASS 2 |
| layoutdemo | 5 × 2 | PASS 10 |
| resdemo | 5 × 2 | PASS 10 |
| calculator | 4 × 2 | PASS 8 |
| qa_ui | 1 × 2 | PASS 2 |
| launcher (lane) | 3 boards × 2 | PASS 6 |
| settings (lane) | 1 × 2 | PASS 2 |
| weather | 3 boards × 2 | PASS 6 |

Logs: `build/sim/logs/<run>/<app>.<mode>.log` under the worktree; the runner's own output per
app in the scratchpad was copied to `shots/_scripts/simrun-<app>.log`.

## Where to look harder, answered

| Probe | Result |
|---|---|
| Overlay controls over app content | F10; picoclock's `Alarms` corner is BACK on the testbench |
| A dialog on a letterboxed / panned window | panel-centred (A4); fine |
| Pan plus focus (claudeusage on the Enviro+) | keys cannot reach the off-panel strip; scrollbar never hides (note above) |
| Doze with a dialog / the keyboard up | both survive the doze and respond after the wake |
| A key held across a screen change | clean (navdemo) |
| The list dialog's empty lower half | cosmetic, as noted |
| `setVisible(false)`, a disabled item, `invalidateOptionsMenu` | hidden works; disabled is F7; invalidate not run |
| The nightly | not run |

## How to continue

- The worktree is ready: `cd .claude/worktrees/qa` (branch `qa-portability` = `main`; the
  `third_party` symlinks, `Cargo.lock`, `.wifi-creds.env` and `website/node_modules` are in
  place; the sim target dir is warm for all four boards in debug and the release side for the
  sim-run boards). Never `git add -A` there (`T` symlink entries, `scripts/lint-md/node_modules`,
  `shots/`).
- `shots/_scripts/shot.sh <out.png> <settle> <sim.sh args> [-- ctrl…]` wraps
  `scripts/sim-shot.sh` with the APK pre-built so the settle timer is not eaten by Gradle;
  `batch-*.sh` are the runs above, re-runnable one line at a time; `montage.py` and `zoom.py`
  compose and enlarge captures. A `sim-shot.sh` that could `wait <regex>` like `test.ctrl`
  would remove every timing guess in them.
- Fix order suggested: F1 (a crash), F2 (same code path), F3 (the check apps rely on), F4, F5,
  then the small ones. One fix, one commit, `./scripts/pre-commit`, the row or capture re-run;
  for F1 add the row described above so the nightly holds it.
