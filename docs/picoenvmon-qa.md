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
| 2 | 🟠 High | Live / Switch | Logger toggle (X) never fires `OnCheckedChangeListener`; the logging service never starts/stops | Open |
| 3 | 🟠 High | History | List never shows data; X→Info dialog unreachable | Open |
| 4 | 🟡 Low | Fonts | Em-dash `—` and ellipsis `…` render as tofu (`□`) | Open |
| 5 | 🟡 Low | Settings / EditText | Field clears its displayed value when edited; QWERTY keyboard on a numeric field | Open |
| 6 | 🟡 Low | Settings | Hint bar overflows: "Y:Back" clipped to "Y:B" | Open |
| 7 | ⚪ Nit | Home | Menu highlight is teal on first render, blue after any navigation | Open |

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

## 2. 🟠 High — Logger toggle never starts/stops the service (Live)

Pressing **X** on the focused Logger `Switch` toggles it *visually* (off→on→off) but the
`OnCheckedChangeListener` **never fires**: across 3 toggles there was no `Logger started` /
`Logger stopped` (from `LiveActivity.setLogger`) and no `foreground started` (from
`SensorLoggerService.onStartCommand`). The foreground logging service is therefore never actually
started. This is *pre-GC* and distinct from bug #1.

The wiring looks correct on paper — `register_listener` → `value_changed_cb` queues on
`LV_EVENT_VALUE_CHANGED` → `dispatch_switch_checked_changes` → `fireCheckedChanged` — and LVGL does
emit `VALUE_CHANGED` on keypad-ENTER for a checkable widget, so the exact break point is unconfirmed.
The Settings units `Switch` uses the same path. **Follow-up:** instrument the
`dispatch_switch_checked_changes` drain / `lookup_sw_checked_change_obj` to find whether the event is
queued, looked up, and dispatched.

## 3. 🟠 High — History never shows data; Info dialog unreachable

History always displays **"No samples yet"** (verified after 14 s with the service running). It
snapshots the ring buffer **once**, in `onServiceConnected`, at the moment it binds — when the
freshly-created service has 0 samples — and never refreshes
([HistoryActivity.java:84](../examples/picoenvmon/java/picoenvmon/ui/history/HistoryActivity.java#L84)).
Because the service is only ever *bound* (never started in the foreground — see bug #2) it is
destroyed on screen-leave and its ring buffer resets, so samples can never accumulate across screens.
Consequently the **X → Info `AlertDialog`** path can't be exercised (no rows to select).

Fixing bug #2 (so the foreground logger can run and persist) plus periodically refreshing the list (or
re-snapshotting on resume) would address this.

## 4. 🟡 Low — Missing-glyph tofu for `—` and `…`

The em-dash `—` (U+2014) and ellipsis `…` (U+2026) render as `□`. Sites: Live tile placeholder
(`valueView.setText("—")`), `Formatter.formatGasIaq` fallback (`"—"`), and History's
`"Connecting…"` status. The bundled font lacks those codepoints (`°` U+00B0 renders fine). Use ASCII
(`-`, `...`) or add the glyphs to the font subset.

## 5. 🟡 Low — EditText clears its value on edit; QWERTY for a numeric field

Pressing **X** on the "Temp Hi" field opens the on-screen keyboard but the field goes blank (its `30`
disappears). On Save the value falls back to the original (`tempHi=3000`), so there's no data loss in
this case, but the displayed/edited value is lost. The keyboard is also a full QWERTY for a numeric
field (no numeric input type).

## 6. 🟡 Low — Settings hint bar clipped

The Settings legend `"A:Up  B:Down  X:Edit/Save  Y:Back"` overflows the 224 px `ButtonHintBar` and
"Y:Back" is clipped to "Y:B". The other screens' shorter legends fit. Shorten the Settings hint or
widen/scale the bar.

## 7. ⚪ Nit — Home highlight color inconsistency

The Home menu's selected row is teal (`colorPrimary`) on first render but blue (the keypad-focus
style) after any navigation. Cosmetic; pick one consistently.

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
