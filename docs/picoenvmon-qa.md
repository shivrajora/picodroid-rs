# picoenvmon — Simulator QA Report

- **App:** `picoenvmon` (Pimoroni Enviro+ environmental monitor)
- **Board:** `pico_enviro_mon` (ST7789 display, no touch; 4 buttons A/B/X/Y on GP12–15)
- **Date:** 2026-06-04 · **Commit:** `51c341f` (v0.10.0)
- **Method:** host simulator via `./scripts/sim-remote.sh --board pico_enviro_mon --app picoenvmon`,
  driven over the control FIFO (`tap A/B/X/Y`) with frames captured by `scrot` on the Xvfb display.

The 4-button nav model is: **A = up, B = down, X = open/activate, Y = back** (legend shown on every
screen). Screens: **Home** (hub menu: Live / History / Settings), **Live** (5 sensor tiles + Logger
switch), **History** (temp sample list), **Settings** (3 threshold fields + units switch + Save).

## Summary

| # | Severity | Area | Issue | Status |
|---|----------|------|-------|--------|
| 1 | 🔴 Critical | JVM GC / navigation | After the first GC, every newly-opened Activity throws `NoSuchMethod` and renders broken | **Fixed** — unrooted `Display` singleton (see below) |
| 2 | 🟠 High | Live / Switch | Logger toggle (X) never fires `OnCheckedChangeListener`; the logging service never starts/stops | **Fixed** — same swept-obj_ref cause as #1 |
| 3 | 🟠 High | History | List never shows data; X→Info dialog unreachable | **Fixed** — via #2 (logger persists) + clearer empty state |
| 4 | 🟡 Low | Fonts | Em-dash `—` and ellipsis `…` render as tofu (`□`) | **Fixed** — ASCII in the 3 rendered strings |
| 5 | 🟡 Low | Settings / EditText | Field clears its displayed value when edited; QWERTY keyboard on a numeric field | **Fixed** — one-line EditText + numeric inputType |
| 6 | 🟡 Low | Settings | Hint bar overflows: "Y:Back" clipped to "Y:B" | **Fixed** — shortened the hint |
| 7 | ⚪ Nit | Home | Menu highlight is teal on first render, blue after any navigation | **Fixed** — also style LV_STATE_FOCUS_KEY |

---

## 1. 🔴 Critical — App breaks after the first GC (`NoSuchMethod` on every new screen)

**Symptom.** After the JVM runs its **first garbage collection**, every subsequently-opened Activity
logs `Activity lifecycle error: NoSuchMethod` during `onCreate` and renders broken: Live tiles stay
as `□` placeholders and never fill, History sticks on "Connecting…", and BACK no longer recovers the
wedged screen. Reproduced **3×**; it breaks by the **2nd–3rd screen transition** — sooner with more
interaction, because that allocates faster and trips the GC sooner.

**Evidence (shutdown log of one session).** `gc: 1 collections, 2327 freed` lines up exactly with the
onset:

```text
[PicoEnvMon] Settings.onCreate          ← nav #1  OK
[PicoEnvMon] History.onCreate           ← nav #2  OK (samples=0)
[PicoEnvMon] Live.onCreate              ← nav #3
[sim] Activity lifecycle error: NoSuchMethod    ← fails here
[sim] JVM ... gc: 1 collections, 2327 freed     ← the one GC that broke it
```

In isolation each screen works fine as the *first* navigation, so this is cumulative, not
screen-specific.

**Root cause (confirmed by runtime tracing).** The JVM GC ([jvm/src/gc/mod.rs](../jvm/src/gc/mod.rs))
is non-moving mark-sweep with slot reuse; class/method tables are append-only and never GC-managed. So
`NoSuchMethod` means a *still-referenced* object was swept (a missing GC root), its `u16` heap slot was
reused by a later allocation, and a subsequent dispatch hit the wrong class's vtable.

The swept object is the **`Display` singleton.** `Display.getInstance()`
([sdk/.../graphics/Display.java](../sdk/java/picodroid/graphics/Display.java)) is a native method that
caches the singleton's heap slot in a Rust `DISPLAY_INSTANCE` cell
([display.rs:20](../platforms/rp/src/system/picodroid/graphics/display.rs#L20)) and hands the same
`ObjectRef` back every call; nothing on the Java side keeps a field to it. That cell was **not visited
by `gc_visit_roots`**, so the first GC swept the Display. Its slot was then reused by a transient
`SensorEvent` (the Live screen's service emits them continuously), and `get_instance`'s staleness
check was only `is_live(existing)` — which *passes* on the reused slot. So `Display.getInstance()`
returned a `SensorEvent`. Every `Activity.setContentView(root)` calls
`Display.getInstance().setContentView(root)`, so the next Activity's `onCreate` resolved
`setContentView` on `SensorEvent` → `NoSuchMethod`. Runtime trace at the failure:
`recv_class=picodroid/hardware/SensorEvent method=setContentView`, frame = `picodroid/app/Activity`.

This is the same class of bug the v0.10.0 fix addressed for the click/key/touch/dialog View maps
(memory `project_gc_collects_unfielded_callback_views`); the `Display` singleton and several widget
listener maps were simply missed.

**Fix.** Root the Display singleton: add
[`display::visit_gc_roots`](../platforms/rp/src/system/picodroid/graphics/display.rs) (visits
`DISPLAY_INSTANCE`) and call it from `gc_visit_roots`; also harden `get_instance` to verify the cached
slot is still a `Display` (re-allocate if a future regression lets it be reused). As defense-in-depth
for the same bug class, also added `visit_*_roots` to the **Switch / CheckBox / ToggleButton /
EditText** listener maps (their local-only widgets had the identical missing-root hazard) and wired
all four into `gc_visit_roots`. Regression test: a jvm-layer test that the GC honors the `extra_roots`
hook (`gc_retains_object_via_extra_roots` / `gc_collects_object_when_extra_roots_omits_it` in
[jvm/src/gc/tests.rs](../jvm/src/gc/tests.rs)). **Verified:** a Live→History→Settings→Live walk across
**5 GC collections** now produces **0** `NoSuchMethod` and every screen renders/binds correctly.

> Note: the unit tests guard the GC mechanism, not the per-widget/Display wiring — the `graphics`
> module is `#[cfg(not(test))]` so it can't be host-unit-tested. The wiring is covered by the sim walk
> above.

---

## 2. 🟠 High — Logger toggle never starts/stops the service (Live) — FIXED

Pressing **X** on the focused Logger `Switch` toggled it *visually* but the `OnCheckedChangeListener`
never fired, so the foreground logging service never started/stopped.

**Same root cause as #1, confirmed by tracing.** The `value_changed_cb` *did* queue and the drain ran,
but the Java `Switch` object's obj_ref was in the **unrooted Switch listener map**, so a GC during
Live's heavy allocation swept it; its slot was reused, and `fireCheckedChanged` then dispatched
`onCheckedChanged` on a wrong-class object — failing silently (`let _ = invoke_instance(...)`). The
widget-map rooting added in the GC-fix commit keeps the `Switch` alive, so the listener now fires.
Verified: 5 consecutive toggles → 5 `Logger started`/`stopped` + `foreground started`/stopped
transitions, including the first deliberate toggle on a freshly-opened Live. (The Settings units
`Switch` shares this path and is likewise fixed.)

## 3. 🟠 High — History never shows data; Info dialog unreachable — FIXED

History always displayed **"No samples yet"** because the `SensorLoggerService` was only ever *bound*
(it died on screen-leave, resetting its ring buffer), so `onServiceConnected`'s one-shot snapshot
always read 0 samples. The root cause was bug #2: the Logger toggle was broken, so the service could
never be promoted to a persistent foreground/started service.

With bug #2 fixed, the intended flow works: turn on **Logger** in Live → the service runs in the
foreground and survives screen changes → opening **History** binds the *same* running service and its
snapshot returns the accumulated ring. **Verified:** `History bound, samples=60`, the list renders the
recent 12 rows, and **X → Info `AlertDialog`** ("Sample N / Temperature: …") now opens and dismisses
with Y. Also softened the empty state to point at the Logger toggle
([HistoryActivity.java](../examples/picoenvmon/java/picoenvmon/ui/history/HistoryActivity.java)).
(A live in-place refresh while History is foreground was considered but rejected: rebuilding the
`ListView` resets the D-pad focus to the top each tick, breaking row navigation.)

## 4. 🟡 Low — Missing-glyph tofu for `—` and `…` — FIXED

The em-dash `—` (U+2014) and ellipsis `…` (U+2026) rendered as `□`: the bundled LVGL Montserrat
subset has neither codepoint (`°` U+00B0 is present). Only three *rendered* strings used them — the
Live tile placeholder, `Formatter.formatGasIaq`'s fallback, and History's `"Connecting…"`; the rest
are in Javadoc/comments, which never render. Replaced those three with ASCII (`--`, `Connecting...`).
(Adding the glyphs to the font subset was the alternative but costs flash on this heap-tight board and
needs the font toolchain — not worth it for two characters.)

## 5. 🟡 Low — EditText clears its value on edit; QWERTY for a numeric field — FIXED

The field never actually cleared — tracing showed its text became `"30\n"`. The `EditText` SDK is
documented as *"Single-line text input,"* but `create()` never called `lv_textarea_set_one_line`, so
the textarea was multi-line; the keypad **X** (= ENTER) that opens the keyboard *also inserts a
newline*, moving the cursor to an empty second line so the field looks blank (and `parseOr("30\n")`
falls back). Fixed by honoring the documented contract: `edit_text::create` now sets one-line, so
ENTER no longer inserts and "30" stays put.

For the QWERTY-on-numeric half, added Android-style input types: a `picodroid.text.InputType`
(`TYPE_CLASS_NUMBER`), `EditText.setInputType(int)`, a per-field numeric flag, and `show_system_for`
now picks `LV_KEYBOARD_MODE_NUMBER` vs the text layout for the field it binds. `SettingsActivity`
marks its three integer fields numeric. **Verified:** the Temp Hi field keeps "30" on edit and the
soft keyboard opens as a digit pad (1/2/3/…).

## 6. 🟡 Low — Settings hint bar clipped — FIXED

The Settings legend `"A:Up  B:Down  X:Edit/Save  Y:Back"` overflowed the 224 px `ButtonHintBar` and
"Y:Back" clipped to "Y:B" (the other screens use a single-word X hint like "X:Open"). Shortened it to
`"A:Up  B:Down  X:Edit  Y:Back"` — the same length as the others, so the whole legend fits. The Save
button is self-labelled, so dropping "/Save" loses nothing. **Verified:** "Y:Back" now renders fully.

## 7. ⚪ Nit — Home highlight color inconsistency — FIXED

The Home menu's focused row was teal (`colorPrimary`) on first render but blue after any navigation.
The ListView row only overrode the highlight for `LV_STATE_FOCUSED`; keypad navigation also adds
`LV_STATE_FOCUS_KEY`, which the default theme paints blue, so it took over once the user moved. Now
the row sets the teal fill for **both** `LV_STATE_FOCUSED` and `LV_STATE_FOCUS_KEY` (new FFI constant),
so the highlight stays teal throughout. **Verified:** teal on first render and after navigating.

---

## What works

- **Home hub:** A/B move the highlight, wrap-around (Settings↓→Live, Live↑→Settings), X opens each
  destination, Y exits the app (Android launcher behavior).
- **Live (first nav):** all 5 tiles populate with live values (e.g. Temp 22.07C, Humidity 45.04 %,
  Pressure 1013.27 hPa, 205 IAQ, 301 lx).
- **Settings (first nav):** focus traversal with A/B across all controls; X opens the keyboard; Y
  dismisses the keyboard via the back-chain (staying on screen); the Switch toggles visually; Save →
  "Saved" Toast → `finish()` → Home.
- **Back navigation:** returns to the parent Activity and correctly unbinds + destroys the bound
  service (when not in the post-GC broken state).

## Reproduction / methodology notes

- Drive input headless via the control FIFO printed by `sim-remote.sh`, e.g.
  `echo 'tap B' > /tmp/picodroid-sim-remote-<display>-ctrl` (verbs `tap|down|up|press`, keys
  `A|B|X|Y|PREV|NEXT|ENTER|ESC`).
- Capture frames with `DISPLAY=:<n> scrot --window "$(xdotool search --name picodroid)" out.png`.
- The app's `println!` output and lifecycle logs land in the `sim-remote` log
  (`/tmp/sim-remote.log` when launched as shown above).
- **Cleanup gotcha:** kill the sim by exact process name (`pkill -x picodroid`). Do **not**
  `pkill -f sim-remote.sh` — that pattern also matches the launching shell and self-kills it.

## Related

- Fix tracked in this repo's GC-root work; see memory
  `project_switch_gc_root_gap_nosuchmethod` and the prior
  `project_gc_collects_unfielded_callback_views`.
