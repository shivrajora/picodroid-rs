# QA results, round 2: apps on every board after the fixes — 2026-10-09

**For:** the session that fixes what this round found. **Against:** `main` at `eb0dc9c6` (the nine
fix commits `d4fbd146..714ef3fc`, amendment A7 of
[designs/app-portability-2026-10.md](designs/app-portability-2026-10.md), plus the plan commit),
the plan in [qa-app-portability-round2-2026-10.md](qa-app-portability-round2-2026-10.md), the
baseline in [qa-app-portability-2026-10-results.md](qa-app-portability-2026-10-results.md).
**Where:** the simulator only, from the worktree `.claude/worktrees/qa2` (branch `qa2` =
`main`, nothing committed). Nothing here touched a board.

Every capture and its simulator log are under `.claude/worktrees/qa2/shots/` (`<name>.png` and
`<name>.log`; the helper scripts under `shots/_scripts/`, the round-1 pairs under
`shots/compare/`; a copy of the whole tree is in the main checkout under
`build/qa/app-portability-round2-2026-10/shots/`, gitignored). Captures at 240×240 and 320×240
are at 2× (a coordinate read off them is half of what you see), the 320×480 touch kit at 1×.
The screenshot comparison report, round 1 beside round 2 where a fix changed the picture, is
<https://claude.ai/artifact/8wKkfSephV5Tk721RawSNo> (private); round 1's page is
<https://claude.ai/artifact/JB52NAemNHfJ68LgxPq5m1>.

## Summary

All nine fixes do what their commits claim, on every board the plan names: BACK from a child
Activity of a windowed app survives (five rounds in one run, clean), every Activity opens at its
origin, the fit check reports cut content, the snackbar wraps beside its action, a focused stock
widget carries the border on a keys board and nothing on a touch board, a disabled menu item is
dimmed and inert, a press anywhere outside the keyboard dismisses it, and MENU toggles the list.
The round-1 baseline (Q1–Q12) re-run shows no regression: every scenario that passed still
passes, and the expected changes are the ones the plan lists. The probes around the fixes found
seven things, two of them worth fixing before the next release:

| # | Severity | What |
|---|---|---|
| R1 | **medium** | After BACK dismisses the options menu, the next MENU (key or control) does not open it: the Activity still thinks the menu is showing, "closes" it, logs `menu closed`, and only the press after that opens the list |
| R2 | **medium** | A held SELECT with the menu list open picks the highlighted item *and then* reopens the menu (`selected Refresh`, `menu closed #1`, `menu shown #2`) |
| R3 | low | The focus border on a CheckBox or RadioButton grows the widget: its siblings shift down 4 px while it is focused |
| R4 | low | A NumberPicker that holds the focus at rest shows nothing (picoenvmon Settings' first stop): its outline is only for `FOCUS_KEY` and `EDITED`, the F5 gap for one more widget |
| R5 | low | The new fit check reports cut content in four demos that pose as resizeable: resdemo (6 px on the Enviro+, 2 px on the 320×240 boards), menudemo (55 / 135 px past the right edge), callbacktest (82 px past the bottom), keynav (12 px); keynav's 160×160 root hides 92 px that the line does not count |
| R6 | note | A tap on an EditText that opted out of the keyboard (`setShowKeyboardOnTouch(false)`) dismisses the keyboard that another field raised; Android would keep the IME up |
| R7 | note | On a keys board the initial focus of a window taller than the panel sits off-panel (picoclock's `Alarms`); the first DOWN moves to the *second* stop and pans there, so the user never sees the first |

Plus one plan correction: the two-row menu list's rows on the testbench are at y 76 and 105, not
62 (the plan's tap lands on the card above the first row and does nothing, which looked like a
pass in round 1's F7 evidence).

## Defects

### R1 — MENU after a BACK-dismissed menu does nothing useful

testbench, menudemo: tap the ≡ control (`menu shown #1`), soft BACK (`key: BACK -> dialog
dismissed`), tap the control again: the log says `menu closed #1`, no `menu shown #2`, and the
capture `shots/a9-back-then-menu.png` shows no list. The same by keys on `pico_display2_w`
(`shots/a9-back-then-menu-d2w.log`: MENU, BACK, MENU → `menu shown #1`, `key: BACK -> dialog
dismissed`, `menu closed #1`). A third press opens it.

Cause, from `Activity.java`: BACK dismisses the dialog natively (`lifecycle/input.rs`,
`widgets::dismiss_topmost_dialog()`) without telling the Java `AlertDialog`, so
`mOptionsMenuDialog` stays set; the F9 toggle in `onKeyUp` sees it non-null and calls
`closeOptionsMenu()`, which dismisses a dead handle (no sanitizer report; `nativeDismiss`
tolerates it) and fires `onOptionsMenuClosed` a second time for a menu that closed on BACK
with no callback at all (A6 documents the missing callback; the toggle turns that gap into a
lost keypress). Fix: have the native BACK dismiss reach the Java side (an `OnDismissListener`
on the menu dialog that nulls `mOptionsMenuDialog` and fires `onOptionsMenuClosed`, which also
closes A6's gap), or make the toggle ask the dialog whether it is still showing.

### R2 — a held SELECT on the open menu picks and reopens

`pico_display2_w`, menudemo: MENU, then SELECT held (`input keyevent --longpress 23`). Log
(`shots/a9-held-select-d2w.log`):

```text
[MenuDemo] menu shown #1
[MenuDemo] selected Refresh id=1
[MenuDemo] menu closed #1
key: code=23 action=0 repeat=0 consumed=true focus=false
[MenuDemo] menu shown #2
key: code=23 action=0 repeat=1 consumed=true focus=false
key: code=23 action=1 repeat=0 consumed=false focus=false
```

The press reaches the list first (LVGL's button matrix picks on ENTER's press), the dialog
closes, and the same press then reaches `Activity.onKeyDown` with the menu gone, which starts
tracking it; the repeat makes it a long press and `onKeyLongPress` opens the menu again. The
capture `shots/a9-held-select-d2w.png` shows the list up over `selected Refresh id=1`. A quick
SELECT (Q11.4's pick) is fine; the hold is the problem. Fix: a key whose press a dialog consumed
must not start tracking in the Activity (or the pick should come on the release, as Android's
list does, so a hold never splits across the two).

### R3 — the focus border changes a CompoundButton's size

callbacktest on `pico_display2_w`, DOWN ×3 (`shots/a5-cbt-d2w-3.png`): the CheckBox `x` carries
the border, and every row below it sits 4 px lower than in `a5-cbt-d2w-2.png` / `-4.png` (the
SeekBar at y 123 instead of 119, the Spinner at 151 instead of 147, 1× coordinates). The same for
the RadioButton `a` (`a5-cbt-d2w-6.png`: rows `b` and `c` shift). For a Button, Switch, SeekBar or
Spinner the border is drawn inside the bounds and nothing moves. The LVGL checkbox sizes itself
from its content plus border, so a border set on `FOCUSED` grows it. Fix: for `lv_checkbox` set
the border on the indicator part, or pad the row so the size is constant.

### R4 — a NumberPicker's focus at rest is invisible

picoenvmon Settings on `pico_display2_w` (`shots/a5-env-settings-d2w.png`): the first stop of
the ring is the `Temp Hi` picker (SELECT puts it in edit mode, `a5-env-settings-walk-d2w.png`
after five DOWNs — a full cycle of the five-stop ring — and SELECT), but at rest it shows nothing.
`widgets/number_picker.rs` replicates the theme's outline for `LV_STATE_FOCUS_KEY` and
`LV_STATE_EDITED` only; the group's initial focus sets `FOCUSED`. The F5 fix gave Button,
CompoundButton, SeekBar and Spinner both states through `ensure_in_group`; NumberPicker joins
the group itself and never gets the border. After one DOWN the next picker does show the
outline (`FOCUS_KEY`; `shots/x-env-settings-1-d2w.png`). Fix: route NumberPicker through the
same `set_view_focusable` path, or add the `FOCUSED` state to its outline.

### R5 — the fit check now names four demos

Every capture's log was grepped for `[layout] overflow` (A3 probe e). The resizeable flagships
(calculator, picoenvmon, claudeusage, launcher, Settings, dialogdemo, layoutdemo) say `fit ok`
on every panel; these four do not:

| Demo | Board | Line | What the image shows |
|---|---|---|---|
| resdemo | `pico_enviro_mon` | `content is cut 0 past the right edge, 6 past the bottom` | the second `An inflated row` cut (`shots/a3-res-em.png`; round 1's F3 evidence, now reported) |
| resdemo | `testbench_rp2350` | `… 2 past the bottom` | the second row's descenders cut (`shots/q9-res-testbench_rp2350.png`; round 1 read `fit ok` here) |
| menudemo | both 320×240 boards | `… cut 55 past the right edge` | the status line runs off the panel (round 1's note) |
| menudemo | `pico_enviro_mon` | `… cut 135 past the right edge` | same |
| callbacktest | `testbench_rp2350`, `pico_display2_w` | `… 82 past the bottom` | the radio group's `c` row cut (`shots/a5-cbt-tb.png`) |
| keynav | `pico_display2_w`, `pico_enviro_mon` | `overflow 160x160 … 12 past the bottom` | only the SeekBar, Spinner and TimePicker are visible (`shots/a5-keynav.png`) |

The check is right in each case; the demos are the finding (a one-row trim fits resdemo on the
240-tall panels; menudemo's status TextView wants `LV_LABEL_LONG_WRAP` or a shorter string).
One subtlety of the check itself: keynav's root is a fixed 160×160 layout that clips its own
children — the ScrollView, the clickable text and `Done` are 92 px below its edge and reachable
only by key — but the line counts only what reaches past the *window* (12 px). A clip by a
non-scrolling ancestor smaller than the window is not reported; worth a sentence in the guide,
or a second measure against the nearest clipping ancestor.

### R6 — the second field dismisses the keyboard (note)

keyboarddemo on the testbench: tap the first field (keyboard up), tap the second (`Manual
mode`, `setShowKeyboardOnTouch(false)`): the keyboard goes away (`shots/a8-two-fields.png`); no
`afterTextChanged` for the second field. By the F8 rule the press is outside the keyboard and
not on the bound field, so it dismisses. On Android an already-showing IME stays up when the
focus moves to a view with `showSoftInputOnFocus=false`. Arguable; the demo's second field is
meant for the explicit `Keyboard`, so nothing in the tree depends on it. Note only.

### R7 — the first DOWN skips the off-panel first stop (note)

picoclock on `pico_display2_w`: at rest the screen shows the top half of the 320×480 window and
the keypad focus sits on `Alarms`, 240 px below the panel, with no pan (`shots/m-picoclock-pico_display2_w.png`).
The first DOWN moves the focus to `Set time` and pans there (`shots/a2-first-down-d2w.png`);
the second comes back to `Alarms` (`a2-second-down-d2w.png`). Not a regression (round 1 was the
same, and A2's reset is at `setContentView`, before any key), and Android would show the
initially focused view by scrolling to it. Low; the plan's look-harder question answered: the
first DOWN is a focus move that pans, not a pan alone.

## Part A — each fix, verified and probed

| Fix | Verify | Probes |
|---|---|---|
| A1 F1 BACK from a child Activity | PASS: `a1-back.log`, `a1-back-settime.log` (testbench, soft BACK), `a1-back-d2w.log` (keys): `activity: pop …`, no `D3)`, `OOM` or `allocation of`; the clock face at its origin after each | (a) a genuine D3 has no demo: qa_ui's `close()` calls go through `removeView` (`QaUiActivity.java:230-260`), so the report path stays untested; (b) qa_ui and reclaimdemo rows PASS 2 each with no new `[sim]` warnings and no sanitizer report; (c) five rounds in one run clean (`a1-back-x5.log`, five push/pop pairs); `sim-shot.sh` kills the sim, so its logs have no heap figure; ending the app with a last BACK on the clock prints the exit summary (`shots/a1-heap-{0,1,5}.log`): `heap: peak` 180 / 189 / 199 KB and `current` 145 / 154 / 157 KB after 0 / 1 / 5 rounds — the first round costs 8.8 KB (the Alarms classes and views, loaded once), the next four 1 KB each (`cur 157712 B` → `161672 B`, with 3 → 6 GC collections), which is within what the pacing leaves uncollected rather than a per-round leak. The `--mem-diag` census (`shots/a1-memprobe.log`, `shots/_scripts/memprobe.sh`) says the same: after a GC the Java live set is 155 objects / 15,252 B after one round and 160 / 15,504 B after five (floor 17,334 B both times), and the arena's `nused` grows 176,024 → 179,424 B only as the JVM's pools take one more object chunk and a larger field table while the clock's per-second garbage piles up between collections (`storage` lines: `obj_chunks` 4 → 5 → 6, `fields_cap` 2048 → 2816), not per round. No leak |
| A2 F2 origin on every content root | PASS: `a2-alarms.png` (`< Alarms`, `+ New alarm` at the top), `a2-settime.png` (`< Set time`, Date), `a1-back.png` (`Clock` header at the top after BACK) | (a) keys board: after BACK the clock is at its origin (`a1-back-d2w.png`) and the next DOWN pans to the focused `Set time` (`a2-keys-back-down-d2w.png`); (b) claudeusage's page turns do not reset the horizontal pan (`a2-cu-em.png`, scrollbar as in round 1's `lh-cu-keys-em.png`); (c) no non-windowed demo with an overflowing root: not testable; (d) fragmentdemo 4/4 |
| A3 F3 the fit check sees cut content | PASS: `a3-res-em.log` `[layout] overflow 240x240 in 240x240: content is cut 0 past the right edge, 6 past the bottom`; `a3-layout-em.log` `fit ok` | (a) calculator 8, picoenvmon 4, claudeusage 6, picoclock 4, weather 6: all PASS (the commit's 32/32, but picoenvmon has two rows, not four); (b) Settings → About `fit ok 320x240 in 320x240` after the push (`a3-about-d2w.log`); (c) picoenvmon's history list `fit ok` (`a3-env-hist-em.log`); (d) the menu list and the keyboard add no second `[layout]` line (`a3-menu-em.log`, `a8-type.log`: one each); (e) R5 |
| A4 F4 the snackbar wraps | PASS: `a4-snack-em.png` two lines, RETRY whole, no scrollbar, the bar taller; `a4-snack-d2w.png` and `a4-snack-tk.png` one line, no sliver at the right end (`compare/z-snack-right-d2w.png`, `z-snack-right-tk.png`) | (a) Plain (`Saved`, no action) keeps one line's height, shorter than the action bar (`a4-snack-plain-em.png`, `-d2w.png`); UNDO one line (`a4-snack-undo-*.png`); (b) a snackbar under a dialog: no demo, untested; (c) testbench: the BACK control sits on the bar's left end (`a4-snack-tb.png`), RETRY tappable — `user retried` and the bar gone (`a4-snack-tb-retry.log`); F10 stands |
| A5 F5 a focused stock widget shows its border | PASS: `a5-keydemo.png` `Focus me` with the 2 px light border (`compare/z-focusme-d2w.png`); `a5-keydemo-down.png` Row 1 with the border after `focus row 1`, `2`, `3`; `a5-keynav.png` the SeekBar with the border | (a) keynav, menudemo, keydemo rows PASS 2 each (their DOWN counts still land); (b) callbacktest's ring on `pico_display2_w`: Button `b`, toggle `on`, Switch, CheckBox `x`, SeekBar, Spinner, RadioButton `a` — seven stops, each with a visible border (`a5-cbt-d2w-0.png` … `-6.png`); R3 on the two that shift; (c) `setFocusable(false)`: no demo, untested; (d) qa_ui PASS 2; (e) touch kit and testbench: no border on keydemo or callbacktest (`a5-keydemo-tk.png`, `a5-cbt-tk.png`, `a5-keydemo-tb.png`, `a5-cbt-tb.png`); R4 for NumberPicker |
| A6 F6 no border without navigation keys | PASS: `a6-launcher-tk.png` no ring on the first row (round 1's `m-launcher-pico_touch_kit.png` had one); `m-launcher-pico_display2_w.png` keeps its ring | (a) BACK ends keydemo back to the launcher (`a6-back-tk.log`: `activity: pop keydemo/KeyDemoActivity`), HOME goes home (`a6-home-tk.log`: `key: HOME -> launcher`); (b) Settings opened by a tap: no ring on any row (`a6-settings-tap-tk.png`) |
| A7 F7 a disabled MenuItem | PASS: `a7-menu-hidden-d2w.png` Refresh dimmed, two rows; SELECT on it: nothing, list still open (`a7-menu-disabled-pick-d2w.log`: only `menu shown #1`); DOWN, SELECT: `selected Toggle units id=2`. testbench: a tap on Refresh (y 76) leaves the list open (`a7-menu-disabled-tap-tb.log`), a tap on Toggle units (y 105) picks (`a7-menu-units-tap-tb.log`) | (a) dialogdemo's multi- and single-choice dialogs: Cheese and Onion checked, L checked, nothing dimmed (`a7-dialog-em.png`, `a7-dialog-single-em.png`); its row PASS 2 (`picked item 2`, `single picked 2`, `multi 1=true`); (b) the list opens with Refresh dimmed and **no row highlighted** (the matrix itself carries the focus outline); the first DOWN lands on Toggle units (`a7-menu-down-d2w.png`); (c) all three disabled: the list opens with three dimmed rows, SELECT/DOWN/SELECT do nothing, BACK closes it, no crash (`a7-menu-alldisabled-*.png`, both boards) |
| A8 F8 press-outside dismisses the keyboard | PASS: `a8-dismiss-root.png` (tap on the root), `a8-dismiss-strip.png` (the strip), both dismiss; `a8-keep-field.png` keeps it; windowed 240×240: `a8-kbd240-dismiss-window.png` dismisses | (a) a key types (`afterTextChanged: "seede"`, `a8-type.log`) and OK dismisses with `OnEditorAction: actionId=6` (`a8-ok.log`); (b) a dialog over the keyboard: no demo; (c) a tap where the hidden BACK control was (23,217) hits the keyboard's own key: nothing, no BACK, keyboard stays (`a8-hidden-back.png`); (d) doze, wake by touch: the keyboard stays (`a8-doze.png`, `wake #1 (touch)`); (e) launcher, open, raise, HOME, re-open, raise: the keyboard rises again and the strip dismisses it (`a8-reload.png`, `a8-reload-dismiss.png`); (f) R6 |
| A9 F9 MENU toggles | PASS: `a9-toggle.log` `menu shown #1`, `menu closed #1`, `menu shown #2`, the list in `a9-toggle.png`; keys `a9-keys-d2w.log` shown then closed | (a) R1; (b) R2; (c) `menu closed #2` on the pick after open, toggle-close, open (`a9-pick-count.log`); (d) keydemo: `activity DOWN keyCode=82`, `activity UP keyCode=82`, nothing opens, no noise (`a9-keydemo-menu-d2w.log`) |

## Part B — the baseline, re-run

Round 1's batch scripts re-run as they were (`shots/_scripts/batch-{enviro,enviro2,tk,tb,tb2,tb3,tb4,d2w,d2w2,oom}.sh`,
the worktree path changed), every capture under the same name. No regression: everything that
was PASS is PASS, and the differences are the ones the plan expected.

| Scenario | Board(s) | Round 1 | Round 2 | Evidence / what changed |
|---|---|---|---|---|
| Q1.1 dialog | enviro, touch kit, testbench, display2_w | PASS | PASS | `q1-dialog-*.png`, `fit ok`; the BACK control above the scrim on the testbench |
| Q1.2 snackbar | touch kit, enviro, display2_w | **FAIL** (F4) | **PASS** | `q1-snack.png`, `q1-snack-enviro-keys.png` (two lines, RETRY whole), `q1-snack-d2w.png` (no sliver) |
| Q1.3 keyboard | testbench | PASS | PASS | `q1-kbd.png` (tap `120 55`), the BACK control hidden under it |
| Q1.4 theme chrome | four boards | PASS | PASS | the four `q1-dialog-*.png`: same row pitch and button height |
| Q2.1 BACK held → HOME | display2_w | PASS | PASS | `q2-home.log`: `KeyDemo] ready`, `key: BACK held -> HOME`, `key: HOME -> launcher` |
| Q2.2 soft nav | testbench | PASS | PASS | `q2-softnav.log`: `soft nav: tap -> BACK`, `popped 1` |
| Q2.3 fragmentdemo rows | testbench, display2_w | PASS | PASS | sim-run 4/4 |
| Q3.1 powerdemo row | testbench | PASS | (nightly) | see the sim-run table |
| Q3.2 doze / wake | testbench | PASS | PASS | `q3-dozed.log` `display: doze #1 (sleep key)`; `q3-woken.log` `wake #1 (touch)`, no `PowerDemo] click` |
| Q3.3 Settings → Display | display2_w | PASS | PASS | `q3-settings.png` six rows, first highlighted, `push settings/DisplayActivity`; `q3-settings-pick.log` `display timeout 30000` |
| Q4.1 layoutdemo rows | four boards + loop | PASS | (nightly) | |
| Q4.2 layoutdemo | enviro, display2_w, touch kit, testbench | PASS | PASS | `q4-layout-*.png`, `=== ALL PASSED ===`, `fit ok` on all four (the Enviro+'s second card still ends on row 240, which the new walk accepts) |
| Q4.3 row centring | display2_w | PASS | PASS | `q3-settings-root.png`, `m-launcher-*.png` |
| Q5.1 keynav row | display2_w | PASS | PASS | sim-run |
| Q5.2 SeekBar / Spinner / list dialog | display2_w | PASS | PASS | `q5-seek.log` `seek 49`, `seek 48`; `q5-spinner-open.png` Red highlighted; `q5-listdialog.png` the Colour list, Red highlighted; the SeekBar now also carries the F5 border |
| Q5.3 About paging | display2_w | NOTE | NOTE | `q5-about.png`, `q5-about-back.log` `pop settings/AboutActivity`; About has focusable rows, as before; `fit ok` after the push (A3 b) |
| Q6.1 picoenvmon | enviro, display2_w, touch kit, testbench | PASS | PASS | `q6-env-*.png`, `m-env-testbench_rp2350.png`, `fit ok` |
| Q6.2 picoclock pan | testbench | PASS | PASS | `q6-pan-before/after.png`; `window 320x480 @ (0,0) on the 320x240 panel`; `fit ok 320x480 in 320x480`; F1 and F2 gone, F10 stands |
| Q6.3 calculator / launcher / settings / weather rows | matrix | PASS | (nightly) | |
| Q6.4 calculator | enviro, touch kit | PASS | PASS | `q6-calc-*.png` |
| Q7.1 letterbox | touch kit | PASS | PASS | `q7-cu-tk.png`; `window 320x240 @ (0,120) on the 320x480 panel` |
| Q7.2 pan | enviro | PASS | PASS | `q7-cu-em.png`; `window 320x240 @ (0,0) on the 240x240 panel`; `fit ok 320x240 in 320x240` |
| Q7.3 tap in the window | testbench | PASS | PASS | `q7-tap.log`: `window 240x240 @ (40,0)`, exactly 2 `[CBT] BUTTON` |
| Q7.4 installer gate | display2_w, testbench | PASS | PASS | `q7-gate.log` `install refused: requires a feature this board lacks (uses-feature)`; `q7-gate-tb.log` `apps: installed dragdemo (6436 bytes)` |
| Q8.1 Configuration | four boards | PASS | PASS | the four `config {…}` lines as in round 1 (`q4-layout-*.log`, `m-layout-testbench_rp2350.log`) |
| Q8.2 keys | four boards | PASS | PASS | `keys back=true home=true`, `up=true center=true` on the key boards only |
| Q9.1 resdemo rows | four boards + loop | PASS | (nightly) | the rows assert `ResDemo PASS` and the `[res]` line, not `fit ok` |
| Q9.2 resdemo | testbench, display2_w (+ enviro, touch kit) | PASS | PASS | `[res] 320x240dp land finger: 3 of 3`, `… land notouch: 2 of 3`, enviro `0 of 3`, touch kit `2 of 3`, `ResDemo PASS`; the Enviro+ and testbench logs now say `overflow … content is cut` (expected, A3; R5 against the demo) |
| Q9.3 values-night | — | PASS | PASS | build refused: `'night' is not a supported configuration — … sw<N>dp, w<N>dp, h<N>dp, land, port, notouch, finger` |
| Q10.1 keydemo row | testbench | PASS | PASS | sim-run |
| Q10.2 held DOWN | display2_w | PASS | PASS | `q10-focus.log` `focus row 1`, `2`, `3` (wrapping); the row carries the F5 border now |
| Q10.3 held SELECT | display2_w | PASS | PASS | `q10-held.log` `view LONGCLICK`, `view UP keyCode=23`, no click |
| Q11.1 menudemo row | display2_w | PASS | PASS | sim-run |
| Q11.2 touch | testbench | PASS | PASS | `q11-open.log` `soft menu: tap -> MENU`, `menu shown #1`; `q11-pick.log` `selected Refresh id=1`, `menu closed #1` |
| Q11.3 no menu, no control | testbench | PASS | PASS | `q1-dialog-tb.png` |
| Q11.4 keys | display2_w | PASS | PASS | `q11-keys.png` first row highlighted, `menu shown #1`, no `clicked` |
| Q11.5 control under the keyboard | — | NOT RUN | NOT RUN | no demo has both |
| Q12 docs | — | **FAIL** (F11) | **PASS** | `cd website && npm run build`: `All internal links are valid`; `every-board.md` quotes the new `content is cut` form, which matches the logs |

Round 1's look-harder captures re-run the same way (`lh-*.png`, `oom-*.png`): the keyboard and
dialog under a doze, the held key across an Activity change, the windowed callbacktest and
dialogdemo, the six BACK-from-a-child-Activity runs on three boards — all clean
(`oom-*.log`: `activity: pop …`, no fault). Two captures end with `no picodroid window`, as
in round 1: a tap on the BACK control's corner over `Alarms` and on the windowed callbacktest
ends the app (`soft nav: tap -> BACK`, `activity: pop …/ClockActivity`), which is the expected
F10 behaviour, not a crash.

## Part C — the screenshot comparison report

The page: <https://claude.ai/artifact/8wKkfSephV5Tk721RawSNo> (private; the source and its
images are `build/qa/app-portability-round2-2026-10/page/` in the main checkout). The same ten apps on the four panels,
captured again under round 1's names, with round 1's capture beside round 2's for the fixes
that changed the picture (F1, F2, F3, F4, F5, F6, F7, F8, F9), the picoclock face after BACK
on the testbench, keydemo at rest and after a held DOWN, the keyboard dismissed by the strip,
and the captures behind R2–R5. The pair montages are `shots/compare/pair-*.png` (made with
round 1's `montage.py`); the zooms used for the sliver and border checks are `shots/compare/z-*.png`.

## sim-run rows (`./scripts/sim-run.sh --app X --no-email --no-pull`, both shrink modes)

The apps Part A names, one run each, from the worktree (`PICODROID_BENCH_RECORD=0`); the runner's
output is `shots/_scripts/simrun-<app>.log`, the row logs under `build/sim/logs/<run>/`.

| App | Rows × modes | Result | For |
|---|---|---|---|
| picoclock | 2 × 2 | PASS 4 | A1: the new `test.ctrl` pushes and pops Alarms, then Set time, on both boards |
| qa_ui | 1 × 2 | PASS 2 | A1 (b), A5 (d); no new `[sim]` line (the `Canvas: onDraw … 340 dropped` line is round 1's too) |
| reclaimdemo | 1 × 2 | PASS 2 | A1 (b) |
| fragmentdemo | 2 × 2 | PASS 4 | A2 (d) |
| calculator | 4 × 2 | PASS 8 | A3 (a) |
| picoenvmon | 2 × 2 | PASS 4 | A3 (a) |
| claudeusage | 3 × 2 | PASS 6 | A3 (a) |
| weather | 3 × 2 | PASS 6 | A3 (a) |
| keynav | 1 × 2 | PASS 2 | A5 (a) |
| menudemo | 1 × 2 | PASS 2 | A5 (a) |
| keydemo | 1 × 2 | PASS 2 | A5 (a) |
| dialogdemo | 1 × 2 | PASS 2 | A7 (a) |

44 rows, 0 failures.

The rest of round 1's list (powerdemo, layoutdemo, resdemo, the launcher and settings lanes) ran
inside the whole-matrix nightly below.

### The nightly (`PICODROID_BENCH_RECORD=0 ./scripts/sim-run.sh --no-email --no-pull`, 56 min)

`build/sim/results/2026-10-09_12h28m36s_eb0dc9c6.txt` under the worktree: **PASS 200, FAIL 1,
SKIP 42, ERROR 0**, both shrink modes. The 42 skips are the 21 hardware-only rows twice
(helloworld's board rows, blinky, uart, spidemo, pwmdemo, i2cdemo), as in every sim run. The one
failure is the `size-ratchet` lane, and it is the worktree, not the code: the firmware link died
with `rust-lld: error: … memory.x:3: region 'BOOT2' already defined` (and `'FLASH'` for the
RP2350) because cargo merges the worktree's `.cargo/config.toml` with the parent checkout's and
passes `-Tlink.x` twice (memory note `reference_worktree_cargo_config_merge`, item 1). Re-measured
by hand with the worktree's `rustflags` array stripped (`shots/_scripts/size-lane.sh`, the file
restored after): both boards link, and the ratchet reports the pending growth the plan
anticipated, not a fault — `testbench_rp2040 flash 1,039,816 → 1,040,372 (+556 B)`,
`testbench_rp2350 flash 1,570,024 → 1,570,624 (+600 B)`, RAM +0 on both (run dir
`build/size/logs/2026-10-09_20h24m29s_eb0dc9c6` under the worktree, with the main checkout's
`Cargo.lock`, so no phantom crate growth). That is the nine fix commits' cost (the fit walk, the
snackbar's wrap, the disabled mask, the indev hook, `reset_pan`, four `setFocusable` calls); the
fix session accepts it with `./scripts/bench-report.py --ratchet --sizes-from <dir> --accept` and
a `size: testbench_rp2040 +556 B` / `size: testbench_rp2350 +600 B` trailer.

## Where to look harder, answered

| Probe | Result |
|---|---|
| Focusable by default: picoenvmon's Buttons | Network's `Refresh` is auto-focused and carries the border; SELECT refreshes (`a5-env-network-select-d2w.log`: a new `ntp: synced` and `weather:` after the press). Settings: the ring is Temp, Hum, Lux, Switch, Save; UP/DOWN walk it (five DOWNs are a full cycle: `x-env-settings-1-d2w.png` Hum Lo with the picker's own outline, `-3` the Switch and `-4` Save with the F5 border) and SELECT on Save saves and pops (`x-env-settings-save-d2w.log`: `Settings saved: tempHi=3000 humLo=20000 luxLo=10 ok=true`, `activity: pop …/SettingsActivity`); R4 for the picker at rest. The now-focusable Button eats no key. weather, claudeusage, askclaude have no Button |
| The pan reset on a keys board | R7: the first DOWN is a focus move that pans |
| The fit walk's cost | `reach()` stops at the first `SCROLLABLE` object (every list and ScrollView), recurses only into layouts (which clear `SCROLLABLE`: `linear_layout.rs:16`, `frame_layout.rs:29`), depth-capped at 16; picoenvmon's 60-row history is one stop (`a3-env-hist-em.log`) |
| The indev hook and the doze wake | the waking touch is swallowed and the keyboard stays (A8 d) |
| Disabled rows and the checked mask | `MAX_LIST_ITEMS = 12`, the Builder throws past it (`AlertDialog.java:166`), so the 32-bit mask never overflows |
| The nightly | 200 PASS, 42 hardware-only SKIPs, the size lane red for a worktree reason (above) |

## How to continue

- The worktree is ready: `cd .claude/worktrees/qa2` (branch `qa2` = `main` at `eb0dc9c6`; the
  `third_party` symlinks, the formatter jars, `Cargo.lock`, `.wifi-creds.env` and
  `website/node_modules` are in place; the sim target dir is warm for the four boards in debug,
  release for the sim-run boards, and the `--mem-diag` sim for the testbench). Never `git add -A`
  there (`T` symlink entries, `shots/`, `build/`). `.cargo/config.toml` is as committed; strip
  its `rustflags` array before any firmware build and restore it after.
- `shots/_scripts/shot.sh <out.png> <settle> <sim.sh args> [-- ctrl…]` is round 1's wrapper
  (`SHOT_GAP=0.8` in front of it shortens the 2 s pause after each control line);
  `batch-a-{tb,d2w,em,tk}.sh` are Part A, `batch-*.sh` the round-1 baseline, `batch-extra-*.sh`
  the picoenvmon walk and the heap probe, `memprobe.sh` the census, `simrun.sh` the per-app
  rows, `patch-menudemo.py` / `patch-menudemo-all.py` the two menudemo probes (reverted),
  `montage.py` and `zoom.py` the page's helpers; the page source is
  `build/qa/app-portability-round2-2026-10/page/` in the main checkout.
- Fix order suggested: R1 and R2 together (both are the Activity's menu state versus what the
  native layer did on its own: a BACK dismiss the Java side never hears of, a key press a dialog
  consumed that the Activity then tracks), then R3 and R4 (one style rule each), then the demo
  trims of R5 and a sentence in the guide about a fixed-size root that clips. One fix, one
  commit, `./scripts/pre-commit`; for R1 a menudemo `test.ctrl` step (MENU, BACK, MENU →
  `menu shown #2`) would hold it in the nightly, and for R2 a held SELECT on the open list
  (no `menu shown #2`).
