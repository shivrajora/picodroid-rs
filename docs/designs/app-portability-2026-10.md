# Apps that run on every board — 2026-10

**Status: design settled 2026-10-05, implementation in progress; see §4 for what has landed.
Claims checked against main `0c0eaf39`. The rule this doc adds: an app is written once, against a
240×240 logical-pixel floor and four logical keys, and the OS decides how it lands on each board's
panel and buttons.**

## 0. Why

Every board is a one-off target today. Screen geometry is a compile-time constant; `dp = sp = px`
and `DisplayMetrics.density` is hard-coded to 1 at 160 dpi; the resource packer rejects any
qualified `res/` directory; apps hard-code their panel (`picoclock` 320×480, `picoenvmon` 240×240,
about twenty demos at 240×240 or 320×240). On input, the four-button boards send
`DPAD_UP/DOWN/CENTER/BACK`, the touch kit has BACK and HOME, and the three testbenches have no
button at all: no BACK, no HOME, no way to wake a blanked panel. Java cannot ask whether a board has
touch or keys. Display sleep exists but every board disables it, because the sleeping main loop
runs nothing. `SeekBar`, `Spinner`, `TimePicker`, `DatePicker`, dialog lists and a `ScrollView` of
plain text cannot be driven with four keys.

The owner's requirement: write an app once and run it on all picodroid boards with the OS making
the rendering decisions for different screen sizes; standardise button input so apps work on boards
with no touchscreen; a touchscreen board must have at least one physical button to wake it. Memory,
CPU and flash are constrained, so this is a compromise, not Android.

## 1. What exists (main `0c0eaf39`)

| Board | Panel | dpi | Input | Sleep |
|---|---|---|---|---|
| `testbench_rp2040` | 320×240 landscape 2.8" | 143 (lv_dpi unset → 130) | XPT2046 touch, no buttons | not compiled |
| `testbench_rp2350`, `_w` | 320×240 landscape 2.8" | 143 (130) | XPT2046 touch, no buttons | not compiled |
| `pico_enviro_mon`, `_w` | 240×240 1.54" | 166 | 4 buttons, no touch | `idle_timeout_ms = 0` |
| `pico_display2_w` | 320×240 landscape 2.0" | 200 | 4 buttons, no touch | 0 |
| `pico_touch_kit` | 320×480 portrait 3.5" | 165 | GT911 touch + BACK, HOME | 0 |

- Geometry: `[display] width/height` in `board.toml` → `SCREEN_WIDTH/HEIGHT` consts
  (`crates/build_support/board_cfg.rs` `display_dims.rs`); `lv_display_create(w, h)` once in
  `graphics/lvgl/lifecycle.rs`. `lv_dpi` only sets `LV_DPI_DEF`, which scales LVGL's theme padding,
  so a `wrap_content` button already differs 0.81–1.25× between boards while Java reports density 1.
- Java: `Display.getWidth/getHeight`, `DisplayMetrics` (real pixels, fake density),
  `TypedValue.applyDimension` with DIP and SP as identities. No `Configuration`.
- Layout (after `723925f1`): `LinearLayout` is LVGL flex with weight, `FrameLayout` applies
  gravity and margins through `lv_obj_align`, `MarginLayoutParams`, `<include>`, `<View>`,
  `<shape>` backgrounds, styles and a theme, custom views through `LayoutInflater.Factory`,
  `View.onMeasure`/`MeasureSpec`. Still missing: min/max size, `Space`, a distinct INVISIBLE
  (`view_ops.rs` maps it to HIDDEN like GONE), and the cross axis of a `LinearLayout` defaults to
  CENTER (`widgets/gravity.rs`), not Android's START.
- Resources: `tools/papk-pack/src/res.rs` accepts `values`, `layout`, `drawable` only and rejects
  any name with `-` ("there are no resource configurations"); dimens are packed as pixels with the
  unit dropped; RESR `TABLE_VERSION = 1` has no configuration axis. A PAPK is board-independent;
  install checks only `framework-map-version` (`pd-install/src/orchestrator.rs`).
- Overlays hard-code a 240×240 panel: `toast.rs` 200×40 at (20,180), `snackbar.rs` 220×44 at
  (10,188), `alert_dialog.rs` a 200 px card. They are children of the active screen.
- Keys: `[[button]]` `{pin, lv_key ∈ PREV/NEXT/ENTER/ESC/NONE, keycode}` → `BUTTONS` table and cfg
  `has_buttons`. Pipeline: GPIO IRQ ring → `events/keypad.rs::keypad_read_cb` (5 ms debounce,
  NumberPicker `EditMode`, soft-keyboard remap) → LVGL keypad indev and the Java queue →
  `lifecycle/input.rs::dispatch_key_events` (HOME release → launcher; BACK release closes the
  keyboard, then a dialog; then `View.OnKeyListener`, then `Activity.onKeyDown/Up`). Long-press
  400 ms and repeat 50 ms exist on the Java path only (`key_repeat.rs`); LVGL never sees a held
  key. One linear `lv_group` per Activity; `requestFocus()` is false on button-less boards. HOME's
  DOWN edge leaks to apps (only the UP is consumed at `input.rs:276`).
- Sleep: `lifecycle/mod.rs` under `cfg(all(not(sim), has_buttons))` pauses the tick source and
  blocks in `wait_for_button_event`; the idle timer is refreshed from `has_pending_event()` after
  the tick already drained the ring (`:817`); touch neither refreshes nor wakes
  (`pico_touch_kit/board.toml:64-78`). Backlight is an on/off GPIO.
- Discovery: `PackageManager.hasSystemFeature` knows wifi and ethernet; `Build.BOARD`.

## 2. How the references solve it

**Android.** A compatibility definition fixes a minimum screen, so an app that lays out at the
floor lays out everywhere. Layouts use `dp` against a bucketed density, and containers express
relationships (`match_parent`, `wrap_content`, weight, margins, gravity) rather than coordinates.
Resource qualifiers (`sw<N>dp`, `w<N>dp`, `land`, `notouch`, …) are chosen at run time by best
match. `Configuration` reports `screenWidthDp`, `densityDpi`, `orientation`, `touchscreen`,
`navigation`. An app not declared resizeable runs in screen-compat mode: a fixed logical size with
a compat density, letterboxed. Every device must offer Home and Back (keys, a software bar or
gestures) and must wake on a key; non-touch devices navigate by D-pad with `FocusFinder`, and
"touch mode" hides focus while a finger is in use. Apps declare `<uses-feature
android.hardware.touchscreen required=false>`; the options menu is the input-neutral action
surface (MENU key or an overflow button). `View.setKeepScreenOn` and a screen-timeout setting
govern sleep.

**iOS.** Points with an integer scale (@1x/@2x/@3x), Auto Layout, compact/regular size classes, a
safe area, Dynamic Type. An app not built for a screen runs letterboxed or scaled, never broken.
Every iPhone has a sleep/wake button; tap-to-wake is an extra. tvOS navigates by a focus engine
with select and menu (hold = home); watchOS has a crown and a side button.

**Pebble and others.** Pebble's Back/Up/Select/Down is the canonical four-button model: Up/Down
scroll, Select activates, long Select is the secondary action, and long Back always exits to the
watch face and cannot be taken by an app. Garmin's Connect IQ maps device-specific buttons and
touch to semantic behaviours (`onSelect`, `onBack`, `onNextPage`, `onMenu`), which is the
write-once idea for input. LVGL itself offers keypad and encoder input devices and groups.

## 3. The compromise

### Screens

- **D1 Floor.** 240×240 logical pixels is the smallest conforming screen; the build asserts it. An
  app that lays out at 240×240 with the containers of D4 lays out on every board.
- **D2 Units.** `dp` is one logical pixel on every board, as it is today. A board may declare an
  integer `[display] scale` (1 on every board in the tree); LVGL then renders at the logical
  resolution and the flush replicates pixels. There is no Java-side or widget-native multiply: a
  size crosses the Java/native boundary in about forty places and a density applied at the API
  would have to cover each one and its inverse. `DisplayMetrics.density` stays 1; `xdpi`/`ydpi`
  report the panel's real dpi from a new `board.toml` key `dpi` (the physical figure; `lv_dpi` is
  no longer read as one). True 2× rendering and density-specific drawables are not done.
- **D3 Theme chrome.** `LV_DPI_DEF` is pinned to 160 on every board, so the same logical layout
  gives the same pixels. Today the four boards that set `lv_dpi` scale LVGL's default padding by
  0.81–1.25×, and an app that fits one 320×240 board can overflow the other. This changes pixels
  on those boards; the Stage 1 A/B screenshots are the check.
- **D4 Layout vocabulary, not a solver.** `723925f1` brought margins, applied gravity, includes,
  styles and `onMeasure`. This adds `View.setMinimumWidth/Height`, `TextView.setMaxWidth`,
  `picodroid.widget.Space`, a real INVISIBLE (keeps its space; GONE collapses), and moves the
  `LinearLayout` cross-axis default to Android's START. No ConstraintLayout, RelativeLayout or
  GridLayout. Per-child cross-axis `layout_gravity` inside a `LinearLayout` stays unsupported
  (LVGL flex has no align-self; the packer warns; wrap the child in a `FrameLayout`).
- **D5 Compat window.** An app whose manifest declares a design size runs at exactly that logical
  size: `lv_display_set_resolution` plus `lv_display_set_offset`, which LVGL already has. On a
  larger panel it is centred with cleared bars; on a smaller one it pans; it is never refused.
  Integer up-scaling when a panel is at least twice the design is reserved for the first such
  board. No declaration means resizeable, which is today's behaviour, so nothing changes for
  existing apps until they opt in.
- **D6 Overflow.** A resizeable app whose content exceeds the window pans: LVGL creates every
  object scrollable, the screen included, and a non-scrollable layout chains the drag upward.
  Overlays move to `lv_layer_top` so they stay put. The sim and debug builds log
  `[layout] fit ok WxH` or `[layout] overflow …` once per `setContentView`, and nightly rows assert
  `fit ok` for the flagship apps on each geometry.
- **D7 Touch targets.** Not a layout constraint. On touch boards the framework widens the click
  area of small clickable views to the board's minimum target (about 7 mm from `dpi`) with
  `lv_obj_set_ext_click_area`, so a layout tuned for a dense panel stays tappable on a coarse one.
- **D8 Escape hatch.** A closed subset of Android's qualifiers for `values-` and `layout-`:
  `w<N>dp`, `h<N>dp`, `sw<N>dp`, `land`, `port`, `notouch`, `finger`, in Android's precedence.
  Variants are sparse override blocks in a new RESR directory type; a match is chosen once when the
  app's resources open. The PAPK stays board-independent, `TABLE_VERSION` stays 1, and firmware
  that predates the type reads the base values.
- **D9 Manifest.** Android-shaped elements with this project's kebab attributes:
  `<supports-screens design-width="240" design-height="240"/>` and
  `<uses-feature name="picodroid.hardware.touchscreen" required="true"/>`. The default for every
  feature is "not required": stock widgets work on both profiles, so only an app that reads raw
  touch declares it. The installer refuses an app whose required feature the board lacks.

### Input and power

- **K1 Logical keys.** The KEYS profile is UP, DOWN, SELECT, BACK, delivered as
  `KEYCODE_DPAD_UP/DOWN/CENTER/BACK`. This settles F1 of
  [claudeusage-decisions-2026-10.md](claudeusage-decisions-2026-10.md): the DPAD mapping is what
  lets every stock widget navigate; `KEYCODE_BUTTON_A/B/X/Y` stay as extras no board maps. A
  five-way adds LEFT and RIGHT. A board with fewer than four buttons reaches the four keys by
  build-time synthesis (a long or double press declared on a `[[button]]`); that is specified here
  and built when such a board exists.
- **K2 System functions on every board: BACK, HOME, WAKE.** HOME is a dedicated key where there is
  one, otherwise **BACK held for 1 s** (`home_hold_ms`, default 1000). Apps cannot intercept it:
  the app sees its 400 ms long-press as before, then an UP flagged `FLAG_CANCELED`, and the
  framework launches the launcher. This ends the trap where an app that consumes BACK (claudeusage)
  has no way out, and gives the touch-only testbenches a HOME.
- **K3 Touch profile.** A touchscreen and at least one physical system button: short press BACK,
  hold HOME, any press while asleep wakes (and is swallowed). The three testbenches have no button
  and conform through `soft_nav = true`: an OS-drawn corner control (tap BACK, hold HOME) and
  touch wakes the panel. A new touch board must declare a button; `soft_nav` is for boards that
  cannot be changed.
- **K4 Conformance is a build check.** `build_support` reads `board.toml`: a board with a display
  has touch or the four keys; a touch board has a BACK/HOME button or `soft_nav`; a multi-app board
  can reach HOME. It emits cfgs `has_nav_keys` (any PREV/NEXT/ENTER button) and `soft_nav`, and the
  const `HOME_HOLD_MS`. Reading the Pico's BOOTSEL as a button is rejected: it has no IRQ, and every
  poll floats QSPI CS, which needs RAM-resident code, masked interrupts and core 1 parked.
- **K5 Doze, not freeze.** Sleep turns the panel and backlight off, skips `g.tick`, slows the tick
  source to about 100 ms and leaves the main loop running, so Runnables, alarms, network callbacks
  and sensors continue. The previous design blocked the loop in `wait_for_button_event`, which is
  why every board set `idle_timeout_ms = 0`. User activity is any key edge or touch sample; wake is
  any button, or a touch where sampling continues (the XPT2046 is polled, the GT911 INT already
  wakes its sampler), with no ISR change. The wake press is swallowed. The default timeout is 60 s
  on every board; appliance apps call `View.setKeepScreenOn(true)`; an alarm Activity calls
  `Activity.setTurnScreenOn(true)`. Settings → Display → Screen timeout persists at
  `/system/display` the way Wi-Fi settings do.
- **K6 Clickable implies focusable** on `has_nav_keys` boards, as on Android, so an app written
  with `OnClickListener` alone navigates with keys. One shared focus `lv_style_t` replaces the six
  per-view style writes. `View.isInTouchMode()` is `!has_nav_keys`, static per board.
- **K7 Every stock widget operable with four keys.** NumberPicker's edit mode generalises: SELECT
  enters, UP/DOWN adjust, SELECT or BACK leaves, for SeekBar, Spinner, TimePicker, DatePicker and
  dialog lists. A `ScrollView` with nothing focusable scrolls by key until it cannot, then focus
  moves on (Android's `arrowScroll`). `ViewPager2` pages by key when nothing inside consumes it.
- **K8 Injection.** `pdb input keyevent` and the sim use the pin path when the board has the key
  and a four-slot soft-key queue otherwise, so a test can send BACK or HOME to any board
  (closes FR-11 of [fragments-follow-ups.md](../fragments-follow-ups.md)).
- **K9 Options menu.** `Activity.onCreateOptionsMenu(Menu)` / `onOptionsItemSelected(MenuItem)` as
  the write-once action surface. The OS presents it as a list dialog, opened by MENU where a board
  has one, by an unhandled long SELECT on KEYS boards, and by a corner "⋮" the OS draws on touch
  boards when the menu is not empty. Apps stop hand-mapping buttons to actions.

## 4. Stages

Each stage ships on its own with its own ratchet accept. Status is kept here.

| # | Stage | Status |
|---|---|---|
| 0 | This doc, index row, amendments in the docs it supersedes | landed |
| 1 | Floor assert, `dpi` key, `LV_DPI_DEF` 160, button validation, HOME-down fix, `xdpi/ydpi`, overlays on `lv_layer_top` sized from the display, touch targets | built 2026-10-05: qa_ui 205 checks, dialogdemo and layoutdemo rows on testbench, display2_w and touch kit; screenshots of dialog, snackbar, keyboard on the touch kit and enviro |
| 2 | Conformance check, `has_nav_keys`/`soft_nav`, BACK-hold HOME, soft-nav overlay, soft-key queue, `KEYCODE_MENU/ENTER/POWER/SLEEP/WAKEUP`, `FEATURE_TOUCHSCREEN`, `isInTouchMode` | built 2026-10-06: launcher + keydemo on `pico_display2_w` (hold BACK → `key: HOME -> launcher`), soft-nav tap and hold on `testbench_rp2350`, `fragmentdemo` row now passes on the testbench through soft BACK, keydemo row asserts the hold |
| 3 | Doze: `power.rs` idle timer, ungated sleep state, touch wake, sim sleep, `setKeepScreenOn`, `setTurnScreenOn`, `PowerManager.isInteractive`, `SCREEN_OFF_TIMEOUT` + Settings → Display, boards back to the 60 s default | built 2026-10-06: `examples/powerdemo` sim row (SLEEP, tap wake with no click, POWER toggle, WAKEUP, the stored 1.5 s timeout, keep-screen-on); Settings → Display walked by keys on `pico_display2_w`; see A1 |
| 4 | Min/max size, `Space`, INVISIBLE vs GONE, cross-axis START, packer attrs and class, `layoutdemo` extended | built 2026-10-06: `layoutdemo` row and the same PAPK on `pico_display2_w`, `pico_touch_kit`, `pico_enviro_mon`; the packer warns on a LinearLayout child's `layout_gravity` |
| 5 | Four-key widgets: `EditKind`, dialog lists in the modal group, ScrollView by key, ViewPager2 key paging, clickable ⇒ focusable, shared focus style | built 2026-10-06: `examples/keynav` row on `pico_display2_w` (SeekBar, Spinner list, TimePicker roller, list dialog, clickable text); Settings → Display timeout picked by keys. Not done, see A2: ViewPager2 key paging, the shared focus style |
| 6 | `[layout] fit` log, flagship migrations (picoclock, picoenvmon, claudeusage pages), screen-matrix sim rows | built 2026-10-06: `[layout] fit ok WxH in WxH` / `overflow … pans` after each `setContentView` (sim and debug); picoclock on the 320×240 testbench pans to its buttons on a drag; picoenvmon(_kt) `match_parent` + weighted lists fit 240×240, 320×240, 320×480; five single-literal demos `match_parent`; matrix rows for calculator, picoenvmon, weather and the launcher lanes on three boards; see A3 |
| 7 | `<supports-screens>`, `<uses-feature>`, `window.rs` compat window, touch offset, hardware-scroll gate, installer feature check | built 2026-10-06 (see A4: the window is an object on the screen, so no touch offset or scroll gate): picoclock 320×480 pans on the 320×240 testbench and is the panel on the touch kit; claudeusage 320×240 is letterboxed at (0,120) on the touch kit and pans on the 240×240 Enviro+; a repacked 240×240 `callbacktest` sits at (40,0) on the testbench and a tap beside it does not click while one inside does; `dragdemo` (requires touch) is refused by the `pico_display2_w` launcher's installer and installs on the testbench; matrix rows for the windowed apps; `papk-format`, `pd-install` and `papk-pack` tests |
| 8 | `Configuration`, `Resources.getConfiguration()`, `KeyCharacterMap.deviceHasKey` | built 2026-10-06: `Configuration` is assembled in Java from the window (`Display`), `PackageManager.FEATURE_TOUCHSCREEN` and `KeyCharacterMap.deviceHasKey(KEYCODE_DPAD_CENTER)` — one new native, `nativeDeviceHasKey` (`input_inject::device_has_key`: a mapped button, or BACK/HOME as the board synthesises them); `layoutdemo` prints `config {sw240dp w320dp h240dp 160dpi land notouch dpad nokeys}` and `keys back=… home=… up=… center=…` and sim rows assert the values on `pico_enviro_mon`, `pico_display2_w`, `pico_touch_kit`, `testbench_rp2350`; `qa_ui` checks the same |
| 9 | Qualifier subset in the packer, RESR override blocks, run-time selection, `resdemo` variant | built 2026-10-06 (see A5: RESR type 10, `papk_format::res::config` shared by packer and runtime): `resdemo` carries `values-w320dp`, `values-land` and `layout-finger` and its sim rows assert `[res] 320x240dp land notouch: 2 of 3 variants apply` plus the values it reads on `pico_enviro_mon`, `pico_display2_w`, `pico_touch_kit` and `testbench_rp2350`; `papk-info` lists the variants; papk-format (80), papk-pack (33) tests |
| 10 | Held keys into LVGL: focus auto-repeat, long SELECT reaches `OnLongClickListener` | built 2026-10-06 (see A6): `keydemo` row on `pico_display2_w` asserts `view LONGCLICK` from `--longpress 23` and `focus row 3` from DOWN held a second, beside its Java-side repeats and the BACK-hold HOME |
| 11 | Options menu (`Menu`, `MenuItem`, the three openers), app migrations | built 2026-10-06 (see A6): `examples/menudemo` row on `pico_display2_w` (held SELECT opens, DOWN+SELECT picks, soft MENU reopens, an item's own listener) and the control + a row tapped on `testbench_rp2350`; the flagship apps keep their direct keys (A6) |
| 12 | Website guides, limits, porting checklist, compatibility matrix; closed items moved | built 2026-10-06: new guide `guides/every-board.md` (sidebar "Apps on every board"), `reference/limits.md` Screens section, matrix rows for `Configuration`, `KeyCharacterMap`, `Menu`, `MenuItem` and the Activity/Resources/PackageManager rows, `button-navigation.md` options-menu section, `resources.md` configuration variants, `manifest.md` screens and features; FR-11 filed in `docs/completed/fragments-follow-ups.md` (multi-app A7 and claudeusage F1 already point here); release notes per stage |

### Where each stage lands

- **1** `crates/build_support/board_cfg.rs` (floor assert, `dpi` → `PHYSICAL_DPI`),
  `crates/build_support/lvgl.rs` (`LV_DPI_DEF`), `crates/build_support/config.rs::finish_button`
  (pair and duplicate validation), `lifecycle/input.rs` (HOME both edges), `graphics/display.rs` and
  `sdk/java/picodroid/util/DisplayMetrics.java` (`xdpi/ydpi`), `graphics/lvgl/widgets/{toast,
  snackbar,alert_dialog,keyboard}.rs` (parent `lv_layer_top()`, size from
  `lv_display_get_*_resolution`; dialog card `clamp(w·5/8, 200, 360)` so nothing moves at 320 wide
  or less), `widgets/button.rs` (`lv_obj_set_ext_click_area` under `has_touch`).
- **2** `board_cfg.rs::check_input_conformance` called from `emit_neutral`;
  `graphics/lvgl/events/keypad.rs` (BACK hold timer right after debounce, because `EditMode`
  swallows ESC presses; cancelled UP to the app; reuses the HOME path in `input.rs`);
  new `graphics/lvgl/soft_nav.rs` (pattern `fps_overlay.rs`); soft-key queue drained at the top of
  `dispatch_key_events`, fed by `pdb/input.rs` and `hal/sim/display.rs` when no pin matches;
  `platforms/rp/boards/testbench_*/board.toml` `soft_nav = true`; SDK constants and
  `View.isInTouchMode()`; `native_handler/os.rs` for the feature.
- **3** New `crates/picodroid-core/src/power.rs` (pure `IdleTimer`, `user_activity()` from
  `keypad.rs` and `touch_read_cb`); `lifecycle/mod.rs` (the four gated blocks become one doze
  state; wake drains the key and touch rings and calls `lv_indev_wait_release` on the pointer
  indev, declared in `pd-lvgl-sys`); `hal/sim/display.rs` (blank on sleep); `View.setKeepScreenOn`
  (XML `keepScreenOn`), `Activity.setTurnScreenOn`, `picodroid.os.PowerManager`,
  `picodroid.provider.Settings.System.SCREEN_OFF_TIMEOUT`; `/system/display` written like
  `hal/wifi.rs` (tmp + rename); `system-apps/settings` `DisplayActivity`; the four board files lose
  `idle_timeout_ms = 0`; claudeusage, picoenvmon(_kt), weather and picoclock call
  `setKeepScreenOn(true)`.
- **4** `pd-lvgl-sys/src/lib.rs` (min/max, opa declarations), `graphics/lvgl/view_ops.rs`
  (`set_min_max`; INVISIBLE = opa 0 and not clickable), `widgets/gravity.rs` (cross default
  START), `papk-format/src/res.rs` (attrs appended after 51, class `Space`),
  `tools/papk-pack/src/res.rs`, `LayoutInflater.java`, `View.java`, `TextView.java`, new
  `widget/Space.java`, `method_tables.rs`, `examples/layoutdemo`.
- **5** `graphics/lvgl/edit_mode.rs` (`EditKind {Step, Remap}`), `events/keypad.rs`
  (classify the focused object by LVGL class), `widgets/alert_dialog.rs` (list matrix joins the
  modal group), `widgets/scroll_view.rs`, `widget/ViewPager2.java`, `widgets/button.rs` and
  `View.java::setOnClickListener` (focusable), `events/groups.rs` (shared style).
- **6** `graphics/display.rs::set_content_view` (fit log), the apps, `scripts/hil-tests.conf`.
- **7** `buildSrc/.../ManifestSchema.kt`, `sdk/PicodroidManifest.xsd`, `papk-format/src/lib.rs`
  and `write.rs`, `tools/papk-pack/src/main.rs`; new `graphics/lvgl/window.rs` (pure
  `fit(design, panel)`), applied in `boot.rs::run_app` and reset on exit; `lifecycle.rs`
  `touch_read_cb` and `display.rs::poll_touch` subtract the offset; `hw_scroll.rs` disabled while
  windowed; bars cleared through `hal/facade.rs`; `pd-install/src/orchestrator.rs`. (As built the
  window is an object on the screen and none of the offset, clipping, bar or scroll work exists;
  see A4.)
- **8** `sdk/java/picodroid/content/res/Configuration.java`, `Resources.java`,
  `sdk/java/picodroid/view/KeyCharacterMap.java`, natives in `os.rs`. Values come from the app's
  window, not the panel.
- **9** `tools/papk-pack/src/res.rs` (qualifier parse, precedence), `papk-format/src/res.rs`
  (directory type 9, reader and writer), `crates/picodroid-core/src/resources.rs` (select once in
  `init_from_papk`, cache the resolved layout).
- **10** `events/keypad.rs` (quiet pass reports the held key; one-slot stash for a second key),
  `events/groups.rs` (`lv_indev_wait_release` at each `lv_indev_set_group`).
- **11** `sdk/java/picodroid/view/{Menu,MenuItem}.java`, `Activity.java`, presentation through
  the Stage 5 dialog list, `soft_nav.rs` for the touch opener; excluded on `testbench_rp2040`
  through `framework_class_excludes`.

## 5. Costs (estimates; each stage is measured with `parity-bench.sh --size-only` on landing)

| Stage | Flash RP2350 | Flash RP2040 | RAM | CPU |
|---|---|---|---|---|
| 1 | ~0.5 KB | ~0.5 KB | 0 | none |
| 2 | ~0.5 KB | 1.5–2.5 KB (overlay) | ~32 B + ~250 B LVGL pool for the overlay | none |
| 3 | 0.8–1.2 KB + the Settings screen | 0.6–1 KB | ~100 B | lower while dozing |
| 4 | ~1.5 KB | ~1.5 KB | 0 | none |
| 5 | 1–1.5 KB | ~50 B | ~16 B | one class compare per key edge |
| 6 | 0 in release | 0 | 0 | one layout read per `setContentView` (sim/debug) |
| 7 | 0.8–1.2 KB | 0.8–1.2 KB | ~16 B | a bar clear per launch (≤36 ms on the touch kit) |
| 8 | 1.5–2 KB | 1.5–2 KB | one object | none |
| 9 | 0.6–0.8 KB | 0.6–0.8 KB | ~24 B | a few binary searches per lookup |
| 10 | ~0.4 KB | 0 | ~8 B | none |
| 11 | 3–4 KB | excluded | 0 until used | none |

About 11–15 KB on an RP2350 board and 8–10 KB on the RP2040 testbench against its 1152 KB region.
Every stage trips the 0 % size ratchet (`bench/parity/ratchet.toml`) and is accepted with a `size:`
trailer. Font ladders and the LVGL pool are untouched.

## 6. Verification

- After every stage: `./scripts/sim.sh --app helloworld` prints `[HelloWorld] Hello, World!` and
  `./scripts/pre-commit` ends `==> All checks passed.`; new `hil-tests.conf` rows pass
  `scripts/check-hil-conf.sh` and `scripts/sim-run.sh --app X`.
- The write-once proof is the Stage 6 matrix: the same PAPK of launcher, settings, calculator,
  weather, claudeusage, picoclock and picoenvmon logs `[layout] fit ok` on `pico_enviro_mon`
  (240×240), `pico_display2_w` (320×240) and `pico_touch_kit` (320×480); is navigable by four keys
  on `pico_display2_w`, by touch and soft nav on `testbench_rp2350`, by touch, BACK and HOME on
  `pico_touch_kit`; and every board dozes and wakes.
- Per stage, in the sim unless noted:
  - **1** `dialogdemo`, `snackbardemo` rows; the same on `pico_touch_kit` and `pico_enviro_mon`;
    the `settings-uninstall` pdb row (dialog tap coordinates unchanged); A/B screenshots for D3.
  - **2** `navdemo` on `testbench_rp2350` with `input back`; the launcher row on `pico_display2_w`
    with `input keyevent --longpress 4` expecting `key: HOME -> launcher`. Bench: hold Y on the
    enviro slot; tap the overlay on `testbench_rp2040`; BACK and HOME on `pico_touch_kit`.
  - **3** `input keyevent 223` → `[sim] Display: sleep`; `input tap 10 10` wakes with no click;
    `input keyevent 23` wakes with no view UP; `alarmdemo` fires while dozing. Bench: touch and
    button wake on the touch kit, button wake on `pico_display2_w`, touch wake on `testbench_rp2040`.
  - **4** `papk-pack` tests and `codes_match_the_java_sdk`; `layoutdemo` term row and sim rows on
    the three geometries.
  - **5** a dpad/enter script row on `pico_display2_w` asserting SeekBar, Spinner and TimePicker
    values; the same PAPK on `testbench_rp2350` by `input tap`; `pickerdemo`, `dialogdemo`,
    `keyboarddemo`, `fragmentdemo` as regression.
  - **7** `fit()` unit tests; `papk-format` generative tests for the new keys; a 240×240 app on
    `pico_touch_kit` logs `window 240x240 @ (40,120)` and a test.ctrl tap inside it lands; a
    touchscreen-required app is refused on the `pico_display2_w` sim.
  - **8** `qa_ui` prints the `Configuration`, asserted per board.
  - **9** best-match tests; a type-9 table reads base values through a reader that ignores the
    type; `resdemo` gains `layout-w320dp`, asserted on two boards.
  - **10** `keydemo`: `--longpress 23` gives a long click; `--down 20` held 1 s moves at least three
    rows; SELECT held across `startActivity` gives no click on the new screen.
  - **11** `menudemo` on `pico_display2_w` (long SELECT) and `testbench_rp2350` (tap ⋮).

## 7. Not doing

ConstraintLayout, RelativeLayout, GridLayout (`LV_USE_GRID`); API-visible density, fractional or
down-scaling, density-specific drawables; runtime rotation or `onConfigurationChanged`; round
screens; locale and night qualifiers; a status bar or insets; per-child cross-axis `layout_gravity`
in a `LinearLayout` (no LVGL fork); layout-affecting touch targets; installer-side variant
stripping; a run-time resolution override in the sim (the board matrix covers it); key synthesis
for boards with fewer than four buttons (specified in K1, built with such a board); BOOTSEL as a
button; tickless idle and dormant.

## 8. Docs this supersedes or amends

- [claudeusage-decisions-2026-10.md](claudeusage-decisions-2026-10.md) F1: settled by K1.
- [multi-app-2026-09.md](multi-app-2026-09.md) A2 ("touch-only boards have no BACK"): K2/K3.
- [fragments-follow-ups.md](../fragments-follow-ups.md) FR-11: K8.
- `platforms/rp/boards/pico_touch_kit/board.toml` sleep comment: K5.
- [android-parity-roadmap-2026-08.md](android-parity-roadmap-2026-08.md) T3.2 "no
  configurations": D8 relaxes it to the closed subset.
- Website: `guides/button-navigation.md` (HOME is consumed on both edges; held keys; the pointer
  input device exists on every board), `reference/limits.md`, the porting guide, the compatibility
  matrix — Stage 12.

## Amendments

### A1 (2026-10-06) — Stage 3 as built

K5 said the tick source slows to about 100 ms while dozing. The 16 ms tick stays: with no
tickless idle on the RP2040/RP2350 the saving would have been small, and `tick_source` has no
period change; a dozing tick skips `g.tick` (no render, no input dispatch) and costs little. The
sensor sampler also keeps running while dozing (the old sleep paused it), since the point of doze
is that the appliance keeps measuring. The wake sources are as designed: the GPIO ring left
undrained, and `touch_sampler::latest()` (the sampler task on the GT911, `sample_panel` inline on
the XPT2046). Keep-screen-on counts live views through `graphics/lvgl/keep_on.rs`;
`Activity.setTurnScreenOn(true)` requests a wake at once rather than at resume.

### A2 (2026-10-06) — Stage 5 as built

K7's edit mode generalised as `edit_mode::EditKind`: `Step` for the NumberPicker, `Keys` for a
slider or roller (UP/DOWN) and for a calendar's day matrix (LEFT/RIGHT, SELECT kept for the day).
A `Spinner` is not edited: LVGL's dropdown opens on SELECT, and while its list is open PREV/NEXT
become UP/DOWN and SELECT/BACK stay with the widget (`keypad.rs::widget_remap`). A list dialog's
matrix moves from the Activity's group to the modal group and is focused with row 0 selected, so
PREV/NEXT walk its rows; at the ends they stay put rather than reaching the dialog's buttons, which
is tolerable because BACK dismisses. The TimePicker's rollers report a key-driven change through
`LV_EVENT_KEY`, since `lv_roller_set_selected` sends no VALUE_CHANGED. The DatePicker's calendar
container leaves the focus ring so the picker is one stop.

A `ScrollView` is not a focus stop. On a screen whose group is empty the keys page the first live
scroller (`scroll_lone_scroll_view`), which covers the plain-text screen; a scroller beside focusable
widgets still scrolls only through focus-on-scroll of its children, not by key, as Android's
`arrowScroll` would. Found on the way: a key whose press changes the keypad's group — SELECT on a
dialog row, a key that starts an Activity — had its release delivered to whatever the new group
focused, so a list dialog picked on the press was reopened by the release clicking the row
beneath it; every keypad group swap now calls `lv_indev_wait_release`, which is what the plan's
Stage 10 had set aside for held keys. K6 is done on the Java side: `View.setOnClickListener` makes the view
focusable when `isInTouchMode()` is false, through the existing `setFocusable` path, so the focus
border styles stay per view (the shared `lv_style_t` was not needed yet). ViewPager2 key paging is
not built; the guide says to call `setCurrentItem` from `onKeyDown`, as `claudeusage` does.

### A3 (2026-10-06) — Stage 6 as built

D6 holds as written: LVGL's screen scrolls by default, so an oversized root pans under a drag with
nothing added (picoclock's 320×480 on the 320×240 testbench reaches its buttons), and on a
four-key board through focus-on-scroll. The `[layout]` line is logged from the main loop after the
first tick that follows a `setContentView` (`display.rs::fit_check_after_tick`), not inside
`setContentView` itself, because the tree is laid out by that tick, not at the call; the measure is
the screen's scroll extent (`lv_obj_get_scroll_right/bottom`), so a child placed past the screen
counts even when its root fits.

The flagship migrations shrank: claudeusage's chrome already used `match_parent` and weights
(claudeusage-remaining-shape A2/E2, done 2026-10-01) but its pages are drawn in 320-wide
dimensions (`card_width` 304 dp, two 150 dp limit cards), and picoclock's every screen is pixel
literals against 320×480 — so both are design-size apps and get Stage 7's `<supports-screens>`
rather than a rewrite; their matrix rows assert `fit ok` against that window. picoenvmon(_kt) is
the resizeable migration: `match_parent` root and rows, the home and history lists weighted so the
hint bar sits on the bottom edge of every panel. The write-once proof on three geometries is
therefore calculator, picoenvmon, weather (its three `has_network` boards), the launcher (the
launcher lane now takes a board and an input profile) and settings at 320×240; `[layout] fit ok`
is asserted only by sim rows and lanes, since release firmware does not log it.

### A4 (2026-10-06) — Stage 7 as built

D5 named `lv_display_set_resolution` plus `lv_display_set_offset`. Built that way first, it needed
three things LVGL does not do: the offset is added to every flushed area but nothing clips the
area to the panel (a window larger than the panel flushes off its edge), the pointer is not
offset, and a window larger than the panel has nothing to pan with (the screen *is* the window,
so nothing overflows). The window is instead an LVGL object of the design size on the
panel-sized screen (`graphics/lvgl/window.rs`): every widget is created under it
(`lifecycle::screen_ptr`) and every content root parented to it (`screen_handle`), so the app's
`match_parent` is the design width, `Display.getWidth()` reports it (`window::size()`), and what
the app lays out past its edge is clipped by the object as a screen edge would clip it. On a
larger panel the object is centred and the screen shows through around it (the theme background,
not cleared bars); on a smaller one it starts at the origin and overflows the screen, which pans
exactly as Stage 6 showed for any oversized root — a drag, or the focus. The display stays the
panel, so flushes, touches, the overlays on `lv_layer_top` (sized from the panel, which keeps a
dialog on the panel rather than on an off-screen part of the window) and the panel scroll are
untouched; the `[layout]` check measures against the window object. `set_design(None)` in
`run_app`'s reset deletes the previous app's window object, its views with it; the new app's is
created once its manifest is read, or in `lifecycle::init` when the display comes up later.

D9's `<uses-feature>` is packed as one `requires-features` key (the required names,
comma-joined; features not required are informational and dropped at build time) and checked in
`pd-install` right after the framework-map gate, through a `PackageDirectory::has_feature` the
core answers from `board_features::has` — the table `PackageManager.hasSystemFeature` reads too.
The refusal is `STATUS_ERR` with "requires a feature this board lacks (uses-feature)" on every
path (`pdb install`, the launcher's installer, the simulator's `apps install`); an image that
reached flash another way runs and logs the missing feature once. Only picoclock (320×480) and
claudeusage (320×240) declare a design size; the small demos that size a root to one panel stay
as they are (their rows inject panel coordinates), and `dragdemo` requires the touchscreen.

### A5 (2026-10-06) — Stages 8 and 9 as built

`Configuration` needs no native of its own: it is assembled in Java from `Display` (the window),
`PackageManager.hasSystemFeature(FEATURE_TOUCHSCREEN)` and `KeyCharacterMap.deviceHasKey
(KEYCODE_DPAD_CENTER)`, which is the one new native (`input_inject::device_has_key`: a button
`board.toml` maps to the code, or BACK / HOME as the board synthesises them). `View.isInTouchMode()`
is an instance method, as on Android, so the static side reads the key table instead; both are the
same board fact.

D8's override blocks are RESR type **10**, not 9 — `TYPE_STYLE` took 9 in September. A block is
one `values-<q>` or `layout-<q>` directory: its qualifiers (`[u16 sw][u16 w][u16 h][u8
orientation][u8 touch]`) and `(id, value)` pairs over the base entries, the value of the base
entry's shape (a word, or the offset of a string / word-stream blob laid out beside it). The
parser, the matching and Android's `isBetterThan` live in `papk_format::res::config`, shared by
the packer and the runtime, which ranks the matching blocks once in `resources::init_from_papk`
(`best_first`, eight kept) and reads every lookup through `ResTable::with(selected)`; an older
reader skips the type and sees the base. The runtime's config is the window — so `run_app` sets
the design size before the resources open, and `window::size()` answers from the manifest before
LVGL exists. A variant may only redefine what the base defines (`R` is the base's); a variant
value's references resolve against the base with the variant laid over it, a variant layout's
against the base values; drawables and styles do not vary.

### A6 (2026-10-06) — Stages 10 and 11 as built

Stage 10 is one cell: the LVGL key the last press edge was given stays reported `PRESSED` on
every quiet read pass until its pin comes up (`keypad.rs::LVGL_KEY_HELD`), so LVGL's own keypad
clock runs — `LV_EVENT_LONG_PRESSED` on the focused widget from a held ENTER (the existing
long-click queue, with the click suppressed when consumed), the focus ring walking at LVGL's
repeat rate from a held PREV/NEXT. No second-key stash was needed: the newest press wins, and a
release of another pin leaves the held key alone. The group swaps already wait for the release
(A2), and an app reset forgets the held key (`reset_held_keys`).

K9's three openers are built as designed, with two refinements. A held SELECT opens the menu only
when the focused view has no `OnLongClickListener` of its own (`Activity.nativeFocusTakesLongPress`
reads the long-click map for the keypad's focused object), so a view's long press stays its own;
and the menu's dialog is created in the same tick as the Java long-press, whose keypad-group swap
makes LVGL ignore the SELECT release — the focused view is not clicked. The touch opener is a
second corner control (`graphics/lvgl/menu_button.rs`, `LV_SYMBOL_BARS`, bottom-right, hidden with
the soft-nav control under the keyboard), shown while the resumed Activity's menu has visible
items (`Activity.performResume` → `prepareOptionsMenu` → `nativeSetOptionsMenuAvailable`); its tap
is a soft MENU key, so every opener ends in `Activity.onKeyUp(KEYCODE_MENU)`. `Menu` and
`MenuItem` are interfaces, as Android's are, implemented by the package-private
`picodroid.app.OptionsMenu`; the menu is not excluded on the RP2040 (215 KB of flash headroom
since 2026-09-29 made the exclusion moot). `onOptionsMenuClosed` follows a pick or
`closeOptionsMenu`, not a BACK dismissal (the dialog has no dismiss callback). The flagship apps
keep their direct keys; the menu is the surface for the actions that would otherwise need a button
the board lacks. Verified: `examples/menudemo` row on `pico_display2_w` (held SELECT, DOWN+SELECT,
soft MENU, an item's own listener), and on `testbench_rp2350` by tapping the control and a row
(screenshots) — a sim row cannot drive a dialog by soft keys there, since soft keys reach Java but
not the LVGL keypad, which a touch board does not have.

### A7 (2026-10-08) — Simulator QA fixes

What [qa-app-portability-2026-10-results.md](../qa-app-portability-2026-10-results.md) found,
one commit per finding:

- **F1.** BACK from a child Activity of picoclock on a 320×240 board ended the simulator in an
  OOM abort. Not the window path: the sim's D3 check (`View.close()` under a Java-owned
  container) took the window object for a Java container — it is in the handle table, as the
  content parent — and reported, and the handle sanitizer's backtrace capture for that report
  asked the 408 KB arena for 192 KB in one block. The check now passes the window object, and
  both sanitizer captures run off the simulated heap, so a genuine report panics as designed.
  The picoclock rows push and pop Alarms and Set time (`examples/picoclock/test.ctrl`).
- **F2.** The screen's pan belongs to the screen, so it carried over from one content root to the
  next: an Activity pushed from a panned picoclock opened scrolled to its blank lower half.
  `window::reset_pan` scrolls the screen (and the window object) back to the origin on every
  `setContentView` and when a pop uncovers a parked root — every Activity starts at its own
  origin, as on Android. The keys board pans again from the focused view on the next key.
- **F3.** `[layout] fit ok` while content is cut. A3's measure (the screen's scroll extent) sees
  only the root; the guide's `match_parent` root never overflows, its children do, inside it,
  and a layout does not scroll. `fit_check_after_tick` now also walks the root's tree down to the
  first scroll container and reports what reaches past the window as
  `overflow … content is cut X past the right edge, Y past the bottom`; the pan form is kept for
  an oversized root. The rows that assert `fit ok` still do.
- **F4.** The snackbar at 240 wide clipped its action behind a scrollbar, and at 320 showed a
  scrollbar sliver. The bar no longer scrolls; the label takes the width left beside the action
  and wraps, and the bar's height follows its content up from the bottom edge.
- **F5.** A focused Button showed no focus border on a keys board. The border is set by
  `set_view_focusable`, which only `View.setFocusable` (or K6's click-listener rule) reaches; a
  stock Button, CheckBox, Switch, SeekBar or Spinner is a stop for the keys regardless, because
  LVGL adds a widget with a `group_def` to the default group as it is created, so keydemo's rows
  were walked without the border, and its `requestFocus()` on a Button returned false (the view
  was not "focusable") while LVGL's own focus on the first group member made it look as though it
  had worked. Those widgets are focusable by default now, as on Android, through the same
  `setFocusable(true)` EditText already did; and `ensure_in_group` sets the border styles whether
  or not it added the view (a stock widget is in the group already), which is where the border
  used to be skipped.
- **F6.** The launcher drew a focus ring on the touch kit, whose BACK and HOME buttons give it a
  keypad indev and a default group but no key that moves the focus. The border styles are set
  only where `has_nav_keys` — the fact `View.isInTouchMode()` already reports — so a programmatic
  `requestFocus()` on a touch-only board no longer paints a selection nobody can move.
- **F7.** A disabled `MenuItem` was drawn enabled and a pick on it closed the menu. For a plain
  item list the mask `nativeCreateWithList` already takes (the checked rows of the choice modes)
  now names the disabled rows, which get `LV_BUTTONMATRIX_CTRL_DISABLED`: dimmed, skipped by the
  keypad's walk, and inert to a tap or ENTER, so the list stays open as Android's does.
  `AlertDialog.Builder.setDisabledItems` is package-private, for the options menu only.
- **F8.** Press-outside never dismissed the keyboard inside a design-size window or on the
  app's own root: the hook sat on the screen object and saw only presses that reached it. It is
  on the pointer input device now (`lv_indev_add_event_cb`), which LVGL hands every press before
  the pressed object, with that object as the parameter; a press on the keyboard or on the field
  it types into keeps it up, anything else dismisses. keyboarddemo's "Tap me to dismiss" strip
  does what it says.
- **F9.** The menu control stayed tappable with the list open and a second tap did nothing.
  MENU's release toggles the menu, as Android's window does: open when closed, closed when open;
  the control is left visible as the thing that toggles it.
