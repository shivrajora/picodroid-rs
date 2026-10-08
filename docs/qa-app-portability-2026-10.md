# QA plan: apps on every board — 2026-10

**For:** the session that QAs the app-portability work. **Against:** `main` at `853db847` (the twelve
stage commits `182505e8..853db847`, design in
[designs/app-portability-2026-10.md](designs/app-portability-2026-10.md), its §4 table and
amendments A1–A6 say what was built). **Where:** the simulator only — every scenario below
produces a screenshot to look at and a log to grep. Nothing here needs a board.

## How to work

- Work in a worktree off `main` (`git worktree add .claude/worktrees/qa main`; the worktree
  chores — `Cargo.lock`, the `third_party` symlinks, the formatter jars, `.wifi-creds.env`, the
  duplicated rustflags stripped from `.cargo/config.toml` — are in the memory note
  `reference_worktree_cargo_config_merge`). Never commit `.cargo/config.toml` or the `T`
  symlink entries.
- Screenshots: `./scripts/sim-shot.sh <out.png> <settle> <sim.sh args> [-- <ctrl line>…]`
  runs the app under its own Xvfb, sends the control lines, captures the window and writes the
  simulator log beside the image (`<out>.log`). Look at every image with the Read tool; grep the
  log for the lines each scenario names. The window is the panel at 2× (a 320×240 board shows
  as 640×480), so a coordinate you read off an image is half of what you see.
- Rows: `./scripts/sim-run.sh --app <name> --no-email` runs one app's `hil-tests.conf` rows in
  both shrink modes and prints PASS/FAIL per row; `scripts/hil-tests.conf` says what each row
  asserts and `examples/<app>/test.ctrl` what it sends. A row's log is under
  `build/sim/logs/<run>/<app>.<mode>.log`.
- Control-channel verbs (the same `pdb input` takes): `input tap X Y`, `input swipe X1 Y1 X2
  Y2 MS`, `input keyevent [--longpress|--down|--up] CODE`, `input back`, `apps install <papk>`.
  Key codes: 19 UP, 20 DOWN, 23 SELECT, 4 BACK, 3 HOME, 82 MENU, 223 SLEEP, 224 WAKEUP,
  26 POWER. A key with no pin on the board is a *soft key*: it reaches the app, not LVGL's
  focus ring — on a touch board, drive widgets and dialogs by tap.
- A `loop` app never exits under plain `sim.sh`; use `sim-shot.sh` or `sim-run.sh`.
- Record each scenario as PASS / FAIL / NOTE in the table at the end with the image path and the
  log line, one line per finding. A defect: one fix, one commit, `./scripts/pre-commit` green,
  the row or scenario re-run. Push nothing unless asked.

## The boards

| Board | Panel | Input | What it proves |
|---|---|---|---|
| `pico_enviro_mon` | 240×240 | four keys | the floor; `notouch dpad` |
| `pico_enviro_mon_w` | 240×240 | four keys, Wi-Fi | the net rows at the floor |
| `pico_display2_w` | 320×240 | four keys, Wi-Fi | the keys profile at 320 wide; most key rows pin it |
| `testbench_rp2350` | 320×240 | touch, no button → soft nav | the touch profile with the OS controls |
| `testbench_rp2350w` | 320×240 | touch + Wi-Fi, soft nav | the Wi-Fi settings lane |
| `pico_touch_kit` | 320×480 | touch, BACK and HOME buttons | the tall panel; letterboxing |
| `testbench_rp2040` | 320×240 | touch, soft nav | the small-flash build |

`./scripts/sim.sh --app X --board B` builds for any of them; the first build of a board takes
a minute or two, the rest seconds.

## Scenarios

Each scenario: the command, the lines the log must hold, what to look for in the image, and
what a failure looks like. "Expected" lines are exact substrings unless marked regex.

### Q1 — Overlays and the floor (stage 1)

1. `./scripts/sim-shot.sh shots/q1-dialog-enviro.png 8 --app dialogdemo --board pico_enviro_mon`
   and the same on `pico_touch_kit` and `testbench_rp2350`. The dialog is centred on the panel,
   its card `clamp(w·5/8, 200, 360)` wide (200 on every current board), the list rows inside
   it readable, nothing clipped by the panel edge. On the testbench the round BACK control sits
   bottom-left *above* the dialog's scrim (it must stay tappable). Expected log: `[layout] fit ok`.
2. `./scripts/sim-shot.sh shots/q1-snack.png 8 --app snackbardemo --board pico_touch_kit`:
   the snackbar hugs the bottom edge, full width; on `pico_enviro_mon` it is not wider than
   the panel.
3. `./scripts/sim-shot.sh shots/q1-kbd.png 10 --app keyboarddemo --board testbench_rp2350 --
   "input tap 120 40"` (tap the field): the keyboard rises from the bottom, the BACK control
   hides while it is up (it would sit under the keyboard's bottom-left key).
4. Theme chrome: the four boards that used to set `lv_dpi` (enviro, display2_w, touch kit,
   testbench) now render the same padding. Compare `shots/q1-dialog-*.png` across boards: the
   dialog's button height and list-row spacing are identical in pixels.

### Q2 — BACK, HOME and the soft-nav control (stage 2)

1. Keys board: `PICODROID_BOOT=launcher ./scripts/sim-shot.sh shots/q2-home.png 12 --app
   keydemo --board pico_display2_w --system-apps -- "input keyevent 23" "input keyevent
   --longpress 4"`. Expected: `KeyDemo] ready`, then `key: BACK held -> HOME` and `key: HOME ->
   launcher`; the image shows the launcher again. The app saw its long press on the way:
   `activity UP keyCode=4 canceled` is **not** expected here (keydemo leaves BACK to the
   defaults) — but `onBackPressed` must not have fired (no `KeyDemo] finish`-style exit before
   HOME).
2. Soft nav: `./scripts/sim-shot.sh shots/q2-softnav.png 8 --app fragmentdemo --board
   testbench_rp2350 -- "input tap 23 217"` (the control's centre, bottom-left). Expected:
   `soft nav: tap -> BACK` and the fragment demo's own `popped 1`. Then a hold: the control
   channel cannot hold a tap, so check the hold on the keys board instead (1) — and note in the
   results that the soft-nav hold is bench-only.
3. `./scripts/sim-run.sh --app fragmentdemo --no-email`: both the testbench and the
   `pico_display2_w` rows pass (the testbench one runs through soft BACK).

### Q3 — Doze and wake (stage 3)

1. `./scripts/sim-run.sh --app powerdemo --no-email` passes (SLEEP, tap-wake with no click,
   POWER toggle, WAKEUP, the stored timeout, keep-screen-on).
2. See it: `./scripts/sim-shot.sh shots/q3-dozed.png 6 --app powerdemo --board
   testbench_rp2350 -- "input keyevent 223"`: the image is black (the panel is off); log
   `display: doze #1 (sleep key)`. Then `… -- "input keyevent 223" "input tap 100 100"`: the
   image shows the app again, log `display: wake #1 (touch)` and **no** `PowerDemo] click`
   from that tap (the waking touch is swallowed).
3. Settings → Display: `PICODROID_BOOT=launcher ./scripts/sim-shot.sh shots/q3-settings.png
   14 --app helloworld --board pico_display2_w --system-apps -- "input keyevent 20" "input
   keyevent 20" "input keyevent 20" "input keyevent 20" "input keyevent 23" "input keyevent
   23"` — walk the launcher to Settings, open it, open Display, open the timeout list: the list
   dialog shows six rows with the **first row highlighted** (the keypad focus), log
   `activity: push settings/DisplayActivity`. Add `"input keyevent 20" "input keyevent 23"` to
   pick the second: `Settings] display timeout 30000`. (The launcher's row order may differ —
   read `Launcher] ready` and the image, adjust the DOWN count.)

### Q4 — The layout vocabulary (stage 4)

1. `./scripts/sim-run.sh --app layoutdemo --no-email`: the loop row and the four geometry rows
   pass (ten PASS).
2. `./scripts/sim-shot.sh shots/q4-layout-<board>.png 8 --app layoutdemo --board <board>` on
   `pico_enviro_mon`, `pico_display2_w`, `pico_touch_kit`: `=== ALL PASSED ===` in each log;
   the image shows the demo's rows without overflow at the right edge.
3. Launcher and Settings rows: in `shots/q3-settings.png` (and a launcher shot) the row text
   is **vertically centred** in its row (the cross-axis default moved to START; these two set
   CENTER_VERTICAL explicitly — a top-aligned label is a regression).

### Q5 — Every stock widget on four keys (stage 5)

1. `./scripts/sim-run.sh --app keynav --no-email` passes (SeekBar, Spinner, TimePicker,
   list dialog, clickable TextView, all by keys).
2. Watch one: `./scripts/sim-shot.sh shots/q5-seek.png 8 --app keynav --board pico_display2_w
   -- "input keyevent 23" "input keyevent 20" "input keyevent 20"`: the SeekBar carries the
   edited outline and its value moved (`KeyNav] seek …` in the log). Try the dropdown the same
   way further down the screen; a list dialog opened by SELECT must show its first row
   highlighted.
3. A screen with nothing focusable pages its first `ScrollView` on UP/DOWN. No demo has a
   text-only scroller (askclaude's takes UP/DOWN itself); Settings → About is one: from the
   Q3.3 launcher setup walk to Settings, open About, then `"input keyevent 20" "input keyevent
   20"` — the text scrolls a page per DOWN, and BACK still leaves. If About has focusable rows,
   note it and skip.

### Q6 — Fit, overflow and the screen matrix (stage 6)

1. picoenvmon on three geometries: `./scripts/sim-shot.sh shots/q6-env-<board>.png 8 --app
   picoenvmon --board <board>` for `pico_enviro_mon`, `pico_display2_w`, `pico_touch_kit`.
   Each log: `[layout] fit ok WxH in WxH` with the board's size. Each image: the title top-left,
   the list filling the middle, the `A:Up B:Down X:Open Y:Exit` hint bar **on the bottom edge**
   (not mid-screen on the tall panel), the list's rows as wide as the panel less the padding.
2. The pan: `./scripts/sim-shot.sh shots/q6-pan-before.png 8 --app picoclock --board
   testbench_rp2350` then `… q6-pan-after.png 8 … -- "input swipe 160 200 160 40 300"`. Before:
   the clock face; after: the `Alarms` / `Set time` buttons from the bottom of the 320×480
   layout. Log: `window 320x480 @ (0,0) on the 320x240 panel` and `[layout] fit ok 320x480 in
   320x480`. (This is stage 7's window; the pan is stage 6's mechanism.)
3. `./scripts/sim-run.sh --app calculator --no-email` (eight PASS: the loop row plus three
   geometries), `--app launcher` (six: three boards), `--app settings`, `--app weather` (six,
   needs nothing but the host: the listeners are started for you; if port 7000 is held,
   `ss -ltnp | grep 7000` and wait for that run).
4. Calculator images on `pico_enviro_mon` and `pico_touch_kit`: the keypad fills the width;
   on the tall panel the keys are taller, nothing is left blank at the bottom.

### Q7 — Design size and required features (stage 7)

1. Letterbox: `./scripts/sim-shot.sh shots/q7-cu-tk.png 10 --app claudeusage --board
   pico_touch_kit`: log `window 320x240 @ (0,120) on the 320x480 panel`; the image shows the
   320×240 dashboard vertically centred with dark bands above and below, the BACK/HOME controls
   absent (the touch kit has buttons) — but the **menu control is absent too** (claudeusage has
   no options menu).
2. Pan: `… q7-cu-em.png 10 --app claudeusage --board pico_enviro_mon`: `window 320x240 @ (0,0)
   on the 240x240 panel`; the right 80 px of the dashboard are off the panel and a horizontal
   scrollbar shows; `[layout] fit ok 320x240 in 320x240` (the fit is measured against the
   window).
3. A tap lands in the window: build a windowed copy of a tappable demo —
   `bash scripts/build-apk.sh --app callbacktest -o /tmp/cbt.papk --board testbench_rp2350 &&
   cargo run --quiet --manifest-path tools/papk-pack/Cargo.toml -- --repack /tmp/cbt.papk
   --design-size 240x240 --output /tmp/cbt240.papk` — then `./scripts/sim-shot.sh
   shots/q7-tap.png 8 --apk /tmp/cbt240.papk --board testbench_rp2350 -- "input tap 20 15"
   "input tap 90 15"`. Log: `window 240x240 @ (40,0)`; exactly **two** `[CBT] BUTTON` lines
   (the startup performClick and the tap inside the window at panel x 90; the tap at x 20 is
   beside the window and must not click). Image: the demo's buttons start 40 px in.
4. The installer gate: `PICODROID_BOOT=launcher ./scripts/sim-shot.sh shots/q7-gate.png 12
   --app helloworld --board pico_display2_w --system-apps -- "apps install
   build/apks/dragdemo.papk"` after `bash scripts/build-apk.sh --app dragdemo --board
   pico_display2_w`: log `apps: install refused: requires a feature this board lacks
   (uses-feature)`, launcher still up. The same on `testbench_rp2350` (build dragdemo for it):
   `apps: installed dragdemo`.
5. `./scripts/papk-info.sh build/apks/picoclock.papk` lists `design-width 320`, `design-height
   480`; dragdemo's lists `requires-features picodroid.hardware.touchscreen`.

### Q8 — Configuration and keys (stage 8)

1. `./scripts/sim-run.sh --app layoutdemo --no-email` asserts the printed `Configuration` on
   four boards; read the four `config {…}` lines from the logs and check them against the
   board table above (`sw`, `w`, `h`, `land`/`port`, `notouch`/`finger`, `dpad`/`nonav`).
2. `keys back=true home=true …` on every board; `up=true center=true` only on the four-key
   boards.
3. `./scripts/sim-run.sh --app qa_ui --no-email` passes (its `configuration` checks included).

### Q9 — Configuration variants (stage 9)

1. `./scripts/sim-run.sh --app resdemo --no-email` (ten PASS): each geometry's log has its
   `[res] WxHdp … : N of 3 variants apply` line and `variants geometry=… columns=…`.
2. `./scripts/sim-shot.sh shots/q9-res-<board>.png 8 --app resdemo --board <board>` on
   `testbench_rp2350` (finger: the inflated row is **visible** at once) and `pico_display2_w`
   (notouch: the row is invisible until a key). `ResDemo PASS` in both logs.
3. `./scripts/papk-info.sh build/apks/resdemo.papk` shows `overrides 3` and three `variant-…`
   lines. A directory with an unsupported qualifier (`examples/resdemo/res/values-night/`,
   any file) must fail the APK build with a message naming the supported ones — remove it after.

### Q10 — Held keys (stage 10)

1. `./scripts/sim-run.sh --app keydemo --no-email` passes (`view LONGCLICK`, `focus row 3`).
2. `./scripts/sim-shot.sh shots/q10-focus.png 8 --app keydemo --board pico_display2_w --
   "input keyevent --down 20" "input keyevent --up 20"` (the `--down` is held across the two
   seconds between lines): the image shows the focus ring on one of the `Row` buttons, not on
   `Focus me`; the log has `focus row 1`, `focus row 2`, `focus row 3` (it may wrap back).
3. A held SELECT on `Focus me` long-clicks it (`view LONGCLICK`) and the **release does not
   click**: no `KeyDemo] view DOWN keyCode=23` … followed by a click line.

### Q11 — The options menu (stage 11)

1. `./scripts/sim-run.sh --app menudemo --no-email` passes (held SELECT, DOWN+SELECT picks
   `Toggle units`, soft MENU reopens, `About` is taken by its own listener).
2. Touch: `./scripts/sim-shot.sh shots/q11-control.png 8 --app menudemo --board
   testbench_rp2350`: a round ≡ control bottom-right beside the BACK control bottom-left. Then
   `… q11-open.png 8 … -- "input tap 297 217"`: the three-row list; log `soft menu: tap ->
   MENU`, `menu shown #1`. Then `… q11-pick.png 8 … -- "input tap 297 217" "input tap 160
   62"`: `selected Refresh id=1`, `menu closed #1`, the list gone.
3. No menu, no control: `shots/q1-dialog-*.png` on the testbench shows the BACK control only.
4. Keys board: `./scripts/sim-shot.sh shots/q11-keys.png 8 --app menudemo --board
   pico_display2_w -- "input keyevent --longpress 23"`: the list with its first row
   highlighted; `Focus me` was **not** clicked (no `MenuDemo] clicked`).
5. The control hides under the keyboard: `keyboarddemo` has no menu; to see it, give any app a
   menu? Not needed — note it as untested unless a demo with both exists.

### Q12 — Docs

Open `website/src/content/docs/guides/every-board.md`, `reference/manifest.md` (Screens and
features), `guides/resources.md` (Configuration variants), `guides/button-navigation.md` (The
options menu), `reference/compatibility-matrix.md` (Configuration, KeyCharacterMap, Menu,
MenuItem rows): every command and log line they quote should match what you saw above. `cd
website && npm run build` validates the links.

## Where to look harder

These are the places the author would probe first; none is a known defect.

- **Overlay controls over app content.** On the touch kit, claudeusage's bottom-right `auto`
  hint and picoclock's bottom-left button sit where the soft-nav (testbench) and menu controls
  are drawn. Run picoclock on `testbench_rp2350` and see whether the BACK control covers a
  tappable part of `Alarms`; run any menu-bearing app on the touch kit.
- **The window and the overlays.** A dialog on a letterboxed app (claudeusage on the touch kit
  opens none; try `dialogdemo` repacked `--design-size 240x240` on the touch kit): the dialog
  is centred on the *panel*, the scrim covers the panel — is that right, or should they follow
  the window?
- **Pan plus focus.** claudeusage on `pico_enviro_mon` is 320 wide on a 240 panel with four
  keys: can the keys reach what is off-panel (focus-on-scroll), and does the horizontal
  scrollbar ever hide?
- **Doze while a dialog or the keyboard is up**, and the wake after: `dialogdemo` → open the
  dialog → `input keyevent 223` → `input keyevent 224`. The dialog must still be there and
  respond.
- **Holding a key across a screen change.** keynav/navdemo: `input keyevent --down 23` while a
  SELECT opens a new Activity, `--up 23` after: nothing on the new screen should click or
  long-press.
- **The list dialog's empty lower half** (`shots/q11-open.png`): the card keeps its full height
  for three rows. Cosmetic; worth a note.
- **`invalidateOptionsMenu`, `setVisible(false)`, a disabled item**: no demo exercises them.
  A quick edit of menudemo (hide `About`, disable `Refresh`) and a re-run of its row tells.
- **The nightly.** `./scripts/sim-run.sh --no-email` (the whole matrix, ~90 min) in both shrink
  modes is the regression sweep; the size ratchet lane is expected red (an accept is pending).

## Results

| Scenario | Board | Result | Evidence (image, log line) | Note |
|---|---|---|---|---|
| | | | | |
