# QA plan, round 2: apps on every board after the fixes — 2026-10

**For:** the session that QAs the nine fixes from round 1. **Against:** `main` at `714ef3fc`
(the fix commits `d4fbd146..714ef3fc`, one per finding, written up as amendment A7 of
[designs/app-portability-2026-10.md](designs/app-portability-2026-10.md)). **What round 1
found:** [qa-app-portability-2026-10-results.md](qa-app-portability-2026-10-results.md); its
plan, [qa-app-portability-2026-10.md](qa-app-portability-2026-10.md), is the baseline this round
re-runs. **Where:** the simulator only.

Two deliverables: the results doc (`docs/qa-app-portability-round2-2026-10-results.md`, same
shape as round 1's: a severity table, one section per defect with the capture and the log line,
the scenario table, the sim-run table) and a **screenshot comparison report** — the same apps on
the four panels, round 1's capture beside round 2's where a fix changed the picture (snackbar,
focus, launcher, menu, picoclock after BACK). Round 1's captures are in
`build/qa/app-portability-2026-10/shots/` in the main checkout (and in the `qa` worktree's
`shots/`); its comparison page is <https://claude.ai/artifact/JB52NAemNHfJ68LgxPq5m1>. Publish
round 2's page the same way and link both from the results doc.

## How to work

Everything in round 1's "How to work" holds. In short:

- A worktree off `main` (`git worktree add -b qa2 .claude/worktrees/qa2 main`); the sim needs
  only the `third_party` symlinks, `Cargo.lock`, `.wifi-creds.env` and `npm ci --prefix website`
  (memory `reference_worktree_cargo_config_merge`). Never commit the `T` symlink entries or
  `.cargo/config.toml`. Round 1's worktree `.claude/worktrees/qa` is a week stale; make a new one.
- `./scripts/sim-shot.sh <out.png> <settle> <sim.sh args> [-- <ctrl line>…]` captures one
  frame and writes `<out>.log` beside it. The settle counts from the `sim.sh` launch, so build
  the APK first (`bash scripts/build-apk.sh --app X --board B`) or a cold Gradle eats it; round
  1's wrapper `shots/_scripts/shot.sh` does that for you, and its `batch-*.sh` are every capture
  of round 1 as one line each — copy them into the new worktree and re-run. `sim-run.sh` needs
  `--no-pull` in a worktree. cargo and Gradle serialize: never capture while a sim-run builds.
- The window is the panel at 2× for 240×240 and 320×240 (halve what you read off an image), 1×
  for the 320×480 touch kit.
- Soft keys reach the app, not LVGL's focus ring; on a touch board drive dialogs by tap.
  Key codes: 19 UP, 20 DOWN, 23 SELECT, 4 BACK, 82 MENU, 223 SLEEP, 224 WAKEUP.
- A defect: one capture, one log line, one entry in the results doc. Fix nothing unless asked;
  push nothing.

## The boards

As in round 1: `pico_enviro_mon` (240×240, four keys, the floor), `pico_display2_w` (320×240,
four keys), `testbench_rp2350` (320×240 touch, soft BACK and menu controls), `pico_touch_kit`
(320×480 touch, BACK and HOME buttons). `pico_enviro_mon_w` only for the weather rows.

## Part A — each fix, verified and probed

One section per fix. "Verify" is the commit's own claim; "probe" is where the fix could have
broken something near it. Every verify needs its capture; every probe needs at least the log.

### A1 — F1, BACK from a child Activity of a windowed app (`b970435f`)

Verify: `./scripts/sim-shot.sh shots/a1-back.png 10 --app picoclock --board testbench_rp2350 --
"input swipe 160 200 160 40 300" "input tap 100 200" "input back"`: the log has `activity: pop
picoclock/ui/AlarmListActivity` and **no** `D3)`, `OOM` or `allocation of` line; the image is the
clock face at its origin (see A2). Same with `"input tap 240 200"` for Set time, and on
`pico_display2_w` by keys: `"input keyevent 20" "input keyevent 20" "input keyevent 23" "input
keyevent 4"`. `./scripts/sim-run.sh --app picoclock --no-email --no-pull`: both rows pass with
the new `test.ctrl` (push and pop Alarms, then Set time).

Probe: the fix moved the handle sanitizer's backtrace capture onto the simulated heap and taught
the D3 check about the window object. (a) A **genuine** D3 report must still panic: there is no
demo that closes a view under a Java container on purpose; note it as untested unless `qa_ui`
or `bugbash_ui` covers it (grep their sources for `View.close`). (b) Run `./scripts/sim-run.sh
--app qa_ui --no-email --no-pull` and `--app reclaimdemo` (the sanitizer's other customers)
and read their logs for new `[sim]` warnings. (c) Repeat the BACK five times in one run
(`"input tap 100 200" "input back"` ×5 after the swipe, settle 10) and compare `heap: peak` at
the end of the log with a single round's: a leak shows as growth per round.

### A2 — F2, every content root starts at the screen's origin (`b0b21add`)

Verify: after the swipe and `"input tap 100 200"` on `testbench_rp2350`, the Alarms screen
shows its header `< Alarms` and `+ New alarm` at the top (`shots/a2-alarms.png`; round 1's
`lh-clock-alarms-body.png` showed the blank lower half). Set time shows its title, not the
calendar's lower rows. BACK then shows the clock face from its origin (`Clock` header at the
top), not panned.

Probe: `reset_pan` runs on every `setContentView` and on a pop that uncovers a parked root.
(a) Keys board: `pico_display2_w`, picoclock, `"input keyevent 20" "input keyevent 20" "input
keyevent 23" "input keyevent 4" "input keyevent 20"`: after BACK the clock is at its origin and
the next DOWN pans again to the focused view (the commit says so) — capture before and after the
last DOWN. (b) An app that changes content without an Activity change: claudeusage on
`pico_enviro_mon` turns pages with DOWN inside one Activity; the horizontal pan must **not**
reset per page (it never panned by key, so expect no change, but confirm the scrollbar's
position in `shots/a2-cu-em.png` matches round 1's `lh-cu-keys-em.png`). (c) A non-windowed
app whose root overflows (keyboarddemo's is 240×100, fits): skip unless one exists; note it.
(d) fragmentdemo on the testbench: fragment transactions replace views under one Activity;
`./scripts/sim-run.sh --app fragmentdemo --no-email --no-pull` still 4/4.

### A3 — F3, the fit check sees cut content (`a50ab263`)

Verify: `./scripts/sim-shot.sh shots/a3-res-em.png 8 --app resdemo --board pico_enviro_mon`:
the log now says `[layout] overflow 240x240 in 240x240: content is cut 0 past the right edge,
2 past the bottom` (the number may differ by a pixel; the form is what matters). layoutdemo on
the same board stays `fit ok` (its second card ends on the last row).

Probe: the walk stops at the first scroll container and the sim rows assert `fit ok` on every
flagship. (a) `./scripts/sim-run.sh --app calculator --no-email --no-pull`, `--app picoenvmon`,
`--app claudeusage`, `--app picoclock`, `--app weather`: all green (the commit says 32/32).
(b) An app with a ScrollView root: Settings → About on `pico_display2_w` (walk: launcher DOWN,
SELECT, then SELECT) — the log after the push must say `fit ok` for About, not an overflow of
its scrolled content. (c) picoenvmon's history list (`"input keyevent 20" "input keyevent 23"`
from Home on `pico_enviro_mon`): a list is a scroll container, so `fit ok`. (d) The menu list
dialog and the keyboard live on `lv_layer_top`, not under the root; opening either must not
produce a second `[layout]` line. (e) Every other demo you capture in Part C: grep its log for
`[layout] overflow` and list each one — a resizeable demo that now reports cut content is a
finding against the demo (or the check, if the image shows nothing cut).

### A4 — F4, the snackbar wraps beside its action (`ae265167`)

Verify: `./scripts/sim-shot.sh shots/a4-snack-em.png 10 --app snackbardemo --board
pico_enviro_mon -- "input keyevent 20" "input keyevent 20" "input keyevent 23"`: the bar on the
bottom edge, the message on two lines, RETRY whole, no scrollbar, the bar taller than before
(round 1's `q1-snack-enviro-keys.png` had the action cut). On `pico_display2_w` (same keys) and
`pico_touch_kit` (`"input tap 120 115"`): one line, no sliver at the right end — zoom the right
end of the bar to be sure (round 1's `z-snack-right.png` is the sliver).

Probe: the bar's height is `LV_SIZE_CONTENT` anchored to the bottom. (a) The "Plain
(auto-dismiss)" and "With UNDO" snackbars (first and second buttons: `"input keyevent 23"` and
`"input keyevent 20" "input keyevent 23"`): UNDO has an action, Plain none — the bar with no
action keeps one line's height, not the action's. (b) A snackbar while a dialog is up
(dialogdemo opens one at start; snackbardemo has no dialog): untestable without an edit; note
it. (c) The snackbar on the testbench with the soft-nav control: the control sits bottom-left
*on* the bar (round 1 noted overlays over content); capture `shots/a4-snack-tb.png` and say
whether RETRY is still tappable (`"input tap 275 220"` after the snackbar is up should dismiss
it: look for the demo's dismiss line in the log).

### A5 — F5, a focused stock widget shows its border (`f49156cc`)

Verify: `./scripts/sim-shot.sh shots/a5-keydemo.png 8 --app keydemo --board pico_display2_w`:
`Focus me` carries the 2 px light border (round 1's `z-focusme.png` is the flat button). Then
`-- "input keyevent --down 20" "input keyevent --up 20"`: the row the walk stopped on carries the
border, not only LVGL's side brackets. keynav on the same board at rest: the SeekBar (first
stop) shows the border.

Probe: Button, CompoundButton, SeekBar and Spinner are focusable by default now, so every demo
with those widgets has a different focus order and a first stop it did not have. (a)
`./scripts/sim-run.sh --app keynav --no-email --no-pull` and `--app menudemo`, `--app keydemo`
still pass (their scripts count DOWNs). (b) callbacktest on `pico_display2_w` with
`"input keyevent 20"` ×6 (settle 8): walk the ring and read the log — every stop must be a widget
the eye can see; a stop that shows nothing (a CheckBox in a row, a Switch) is a finding. (c)
A widget with `setFocusable(false)` explicitly: no demo does; note it. (d) `View.isFocusable()`
now true on a Button: qa_ui's checks (`sim-run --app qa_ui`) still pass. (e) On the touch kit
and testbench the default focus must draw **nothing** (A6): keydemo and callbacktest captures
there show no border on any widget.

### A6 — F6, no focus border without navigation keys (`c6aa2f66`)

Verify: `PICODROID_BOOT=launcher ./scripts/sim-shot.sh shots/a6-launcher-tk.png 14 --app
helloworld --board pico_touch_kit --system-apps`: no ring on the first row (round 1's
`m-launcher-pico_touch_kit.png` had one); `pico_display2_w` keeps its ring.

Probe: the group membership is unchanged, only the border. (a) The touch kit's BACK and HOME
buttons still work as keys (`"input keyevent 4"` ends an app, `"input keyevent 3"` goes HOME
with the launcher booted). (b) The Settings rows on the touch kit: tap-driven; no ring after a
tap either (`"input tap 160 60"` on the launcher's Settings row, capture).

### A7 — F7, a disabled MenuItem (`329464fb`)

Verify: menudemo has no disabled item, so patch it as round 1 did
(`shots/_scripts/patch-menudemo.py`: Refresh disabled, About hidden; `git checkout --
examples/menudemo` after). `pico_display2_w`, `-- "input keyevent --longpress 23"`: Refresh
dimmed, two rows; then `… "input keyevent 23"`: nothing selected, the list **still open**
(round 1: `menu closed #1`); then `… "input keyevent 20" "input keyevent 23"`: `selected Toggle
units id=2`. Testbench: `"input tap 297 217" "input tap 160 76"` leaves the list open; a tap on
the second row (y 105) picks. (A two-row list sits lower than a three-row one: its rows are at
y 76 and 105, and a tap at y 62 lands on the card above the first row and does nothing.)

Probe: the disabled mask rides on the checked-rows mask of the choice modes. (a) dialogdemo's
single- and multi-choice dialogs (`./scripts/sim-run.sh --app dialogdemo --no-email --no-pull`,
and the capture `shots/a7-dialog-em.png` on `pico_enviro_mon`): the checked rows are still
the checked rows, nothing dimmed. (b) The keypad's walk skips a disabled row: with Refresh
disabled and first, the list must open with **Toggle units** highlighted (or Refresh dimmed and
DOWN landing on Toggle units — say which). (c) All items disabled: edit the patch to disable
all three and open the menu: it must open and BACK must close it, with no crash.

### A8 — F8, press-outside dismisses the keyboard (`26e8994c`)

Verify on `testbench_rp2350`, keyboarddemo, settle 10: `-- "input tap 120 55" "input tap 60
70"` dismisses (round 1's `lh-kbd-dismiss-root.png` kept it); `"input tap 120 55" "input tap 100
32"` (the "Tap me to dismiss" strip) dismisses; `"input tap 120 55" "input tap 120 55"` keeps
it (the field). The windowed copy (`shots/_scripts/kbd240.papk`, or rebuild with `papk-pack
--repack … --design-size 240x240`): `"input tap 160 55" "input tap 270 15"` dismisses.

Probe: the hook is on the pointer indev and sees every press first. (a) A press on the keyboard's
own keys types (`"input tap 120 55" "input tap 85 118"` → `afterTextChanged` with one more
character); the OK key still dismisses. (b) A dialog over the keyboard: none in a demo; note.
(c) The soft-nav and menu controls are hidden under the keyboard; a press where the hidden BACK
control was (`"input tap 23 217"`, which is the keyboard's bottom-left key) types or does
nothing but must not send BACK. (d) Doze while the keyboard is up, wake by touch (`"input tap
120 55" "input keyevent 223" "input tap 100 100"`): the waking touch is swallowed and the
keyboard **stays** (it is a press outside the keyboard, but the wake must eat it first). (e)
An app reload: `PICODROID_BOOT=launcher` on the testbench, open keyboarddemo from the launcher
by tap, raise the keyboard, BACK-hold is bench-only — instead `"input keyevent 3"` (HOME) and
then re-open the app: the keyboard rises again on a tap (the hook re-attached). (f) Two
EditTexts: tap field 1, then field 2 (`"input tap 120 55" "input tap 120 95"`): the keyboard
stays and moves its target (`afterTextChanged` lines name which).

### A9 — F9, MENU's release toggles the menu (`714ef3fc`)

Verify: testbench, menudemo, `-- "input tap 297 217" "input tap 297 217" "input tap 297 217"`:
`menu shown #1`, `menu closed #1`, `menu shown #2`; the capture after three taps shows the list.
Keys board: `"input keyevent 82" "input keyevent 82"`: shown then closed.

Probe: (a) BACK with the list open still dismisses with no `menu closed` (A6 of the design) —
or does the toggle now log it? Say which. (b) Held SELECT while the list is open (keys board):
no second menu, no long-click on the dimmed row behind. (c) `onOptionsMenuClosed` count after
open, toggle-close, open, pick: `menu closed #2` on the pick. (d) MENU with no menu (keydemo):
`"input keyevent 82"` reaches `onKeyUp` and nothing opens, no log noise.

## Part B — the baseline, re-run

Round 1's scenarios Q1–Q12 as written (with its two plan corrections folded in: Q1.3 taps
`120 55`; Q9.2 expects the row visible on both boards). Run every `sim-shot.sh` line and every
`sim-run.sh` app again; anything that was PASS and is not now is a regression and goes first in
the results. The fastest way is round 1's batch scripts in `shots/_scripts/` — `batch-enviro.sh`,
`batch-tk.sh`, `batch-tb.sh`, `batch-d2w.sh`, their `2`/`3`/`4` second passes, `simrun.sh` with
the twelve apps — after editing the worktree path at the top of each.

Expected changes from round 1, which are not regressions: the resdemo Enviro+ log says
`overflow … content is cut` (A3); the snackbar is two lines on the Enviro+ (A4); keydemo's and
keynav's focused widgets carry a border (A5); the touch-kit launcher has no ring (A6); the
picoclock rows have more patterns (A1).

## Part C — the screenshot comparison report

Capture the same ten apps on the four panels as round 1 did (calculator, picoenvmon, launcher,
dialogdemo, layoutdemo, resdemo, claudeusage, picoclock, snackbardemo, menudemo), with the same
file names, plus: picoclock on the testbench **after** BACK from Alarms, keydemo at rest and
after a held DOWN on `pico_display2_w`, the keyboard dismissed by the strip on the testbench. Put
them beside round 1's where the fix changed the picture. Compose the montages with round 1's
`montage.py` and publish the page as round 1 did (one tile per board at a common logical scale,
a caption per app, an orange note where a capture shows a defect). Keep the page private; link
it from the results doc.

## Where to look harder

- **Focusable by default (A5) is the widest change.** Every app with a Button on a keys board
  now has a focus stop it did not; picoenvmon, weather, claudeusage and askclaude handle keys
  themselves — do their UP/DOWN still reach their `onKeyDown` first, or does a now-focusable
  Button eat SELECT? `grep -rl "new Button\|<Button" examples/{claudeusage,weather,picoenvmon,askclaude}`
  says which of them have one (claudeusage's `X retry now` is a label, not a Button); for each
  hit, press SELECT on `pico_display2_w` with that screen up and check the app's own log line
  still arrives.
- **The pan reset on a keys board (A2).** A focused view below the panel on a windowed app:
  after `setContentView` the screen is at the origin and the focus is off-panel until a key
  moves it — is the first DOWN a pan or a focus move? Capture both frames on `pico_display2_w`
  with picoclock.
- **The fit walk's cost (A3).** It runs once per `setContentView` in sim and debug only; read
  `display.rs` for anything that could recurse into a list's thousand rows (picoenvmon's history
  has 60).
- **The indev hook (A8) and the doze wake.** The wake path also reads the pointer; a press that
  wakes the panel must not be the press that dismisses the keyboard — A8 probe (d).
- **Disabled rows and the checked mask (A7).** A choice dialog with `setDisabledItems` is
  package-private, so only the menu uses it; but a list dialog with more than 32 rows would
  overflow a mask — read `nativeCreateWithList` for the mask's width and the row cap (~12).
- **The nightly.** `./scripts/sim-run.sh --no-email --no-pull` (whole matrix, ~90 min) once
  everything above is done; the size ratchet lane may be red (an accept is pending).

## Results

Record in `docs/qa-app-portability-round2-2026-10-results.md`, same shape as round 1's.
