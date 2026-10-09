---
title: "Apps on every board"
description: "Write an app once and run it on every picodroid board: the screen floor, layouts that stretch, design sizes, input profiles, the options menu, doze, and how the nightly proves it."
---

A picodroid board is a 240×240, 320×240 or 320×480 panel with either a touch panel or four
buttons. One PAPK runs on all of them: the framework makes the rendering and input decisions, and
an app written against the rules on this page needs no board-specific code. The design is
[app portability](https://github.com/shivrajora/picodroid-rs/blob/main/docs/designs/app-portability-2026-10.md);
this page is what it means for an app.

## What every board guarantees

| | Every board has | Where it comes from |
|---|---|---|
| Screen | at least **240×240** logical pixels; a `dp` is one of them (`DisplayMetrics.density` 1, `densityDpi` 160) | `board.toml` is refused below the floor; `[display] dpi` gives the physical pitch for `xdpi`/`ydpi` |
| BACK | a key, or a tap on the soft-nav control, or a short press of the one button a touch board has | `[[button]] keycode = 4`, `soft_nav = true` |
| HOME | a key, or **holding BACK for a second** (`home_hold_ms`), on a board with a launcher | `board_cfg::input::BACK_HOLD_IS_HOME` |
| Wake | any key or a touch wakes a dozing panel; the waking press reaches no app | `power.rs` |
| Input | **four keys** (`KEYCODE_DPAD_UP` / `DOWN` / `DPAD_CENTER` / `BACK`) or a **touch panel**; every stock widget works with either | `Configuration.navigation`, `touchscreen` |

The boards today: `pico_enviro_mon` and `pico_enviro_mon_w` (240×240, four keys),
`pico_display2_w` (320×240, four keys), `testbench_rp2040` / `rp2350` / `rp2350w` (320×240 touch,
soft-nav control), `pico_touch_kit` (320×480 touch, BACK and HOME buttons).

## Layouts that stretch

Lay out against the 240×240 floor and let the rest take the room a larger panel adds:

- `match_parent` on the root; `layout_weight` on the child that should grow; a weighted
  `<Space>` to push neighbours to the edges.
- `android:minWidth` / `minHeight` floor a `wrap_content` view; `android:maxWidth` caps a label so
  a long line wraps or ellipsizes instead of pushing the row off the panel.
- `android:visibility="invisible"` keeps a view's room while it is hidden (`gone` gives it up).
- `LinearLayout` places children top/start by default, as on Android; a row that wants its
  children centred says `android:gravity="center_vertical"`. Per-child `layout_gravity` inside a
  `LinearLayout` is not applied (the compiler warns): wrap the child in a `FrameLayout`.

A root that is still larger than the panel is not clipped — the screen pans to it, under a drag
or as the focus moves — which keeps the app usable and is the sign to fix the layout. A
`match_parent` root never overflows, but its children can, inside it: a layout does not scroll, so
what it holds past the panel's edge is simply cut, with nothing to pan to. The simulator and a
debug build say which it was after each `setContentView`:

```text
[layout] fit ok 320x240 in 320x240
[layout] overflow 320x480 in 320x240: the screen pans 0 right, 240 down
[layout] overflow 240x240 in 240x240: content is cut 0 past the right edge, 36 past the bottom
```

The cut is measured against the window's edge. A layout that is smaller than the window and
does not scroll — a root left at its default size, a column given a fixed height — clips its own
children just the same, and that is not reported: give such a root `match_parent`, or make it a
`ScrollView`, and read the line again.

The check looks through every layout down to the first `ScrollView` or list, whose content is
reached by scrolling it; the third line means the rows that do not fit a panel need one.

Try the smallest board first: `./scripts/sim.sh --app yourapp --board pico_enviro_mon`.
See [Resources, R and XML layouts](/guides/resources/) for the vocabulary.

## When the layout cannot stretch: a design size

An app drawn in pixels for one panel — a clock face, a dashboard with fixed cards — declares the
size it was laid out against, and the framework shows it in a window of exactly that size on every
board: centred on a larger panel, panned on a smaller one. `Display.getWidth()`, `match_parent`
and every coordinate are the design's.

```xml
<manifest package="picoclock" version="1.0">
    <application application="picoclock/ClockApp" label="Clock" />
    <supports-screens design-width="320" design-height="480" />
</manifest>
```

`picoclock` (320×480) and `claudeusage` (320×240) do this. Leave it out — the default — for an app
whose layout stretches. See the [manifest reference](/reference/manifest/#screens-and-features).

## Resources that vary

Where one layout cannot serve both a 240-wide and a 320-wide window, Android's qualified
directories do, in the subset that can differ here: `values-sw320dp/`, `layout-w320dp/`,
`values-h480dp/`, `layout-land/`, `values-notouch/`, `layout-finger/`. A variant overrides what the
base defines; the runtime picks the matching ones once, when the app starts, against the app's
window and the board's input. See [Configuration variants](/guides/resources/#configuration-variants).

## Asking the board

```java
Configuration c = getResources().getConfiguration();
if (c.screenWidthDp >= 320 && c.orientation == Configuration.ORIENTATION_LANDSCAPE) { … }
boolean keys = c.navigation == Configuration.NAVIGATION_DPAD;      // four keys
boolean finger = c.touchscreen == Configuration.TOUCHSCREEN_FINGER; // a touch panel
boolean hasUp = KeyCharacterMap.deviceHasKey(KeyEvent.KEYCODE_DPAD_UP);
```

Use the answers for what to *show* (a hint such as "A: up", a two-column arrangement), not for
whether to handle a key: an app that handles `KEYCODE_DPAD_UP` works wherever the key exists.
`View.isInTouchMode()` is false where the keys drive the focus; a view with a click listener is
focusable there without a `setFocusable` call, as on Android.

## Actions without buttons: the options menu

An app's actions belong in `onCreateOptionsMenu`, not on hand-mapped buttons. The framework
presents the menu as a list — opened by holding SELECT on a four-key board (when the focused view
has no long press of its own), by the menu control it draws bottom-right on a touch board, by a
MENU key where a board has one — and reports the pick to `onOptionsItemSelected`:

```java
@Override
public boolean onCreateOptionsMenu(Menu menu) {
  menu.add(Menu.NONE, ID_REFRESH, Menu.NONE, "Refresh");
  menu.add(Menu.NONE, ID_UNITS, Menu.NONE, "Toggle units");
  return true;
}

@Override
public boolean onOptionsItemSelected(MenuItem item) {
  if (item.getItemId() == ID_REFRESH) { refresh(); return true; }
  return super.onOptionsItemSelected(item);
}
```

`examples/menudemo` is the whole pattern. See
[Button-only navigation](/guides/button-navigation/#the-options-menu).

## Hardware you need

An app that reads raw touch (a drawing pad) says so, and the installer refuses it on a board
without a panel:

```xml
<uses-feature name="picodroid.hardware.touchscreen" required="true" />
```

The default is not required: the stock widgets work with a finger and with four keys alike.

## The panel dozes; the app does not

After the screen timeout (60 s by default; Settings → Display) the panel goes dark and the app
keeps running. A screen that is watched rather than touched holds the panel on with
`View.setKeepScreenOn(true)` on its root (`android:keepScreenOn="true"`); an alarm that must be
seen calls `Activity.setTurnScreenOn(true)`. The waking press reaches no listener. See
[Display idle sleep](/reference/limits/#display-idle-sleep).

## How it is proved

The nightly runs the same PAPK of each flagship app on a 240×240, a 320×240 and a 320×480 board
and expects `[layout] fit ok` (or the declared window); `layoutdemo` prints the `Configuration`
and the keys on four geometries, `resdemo` its variants, `keynav` drives every stock widget with
four keys, `menudemo` the options menu, `powerdemo` doze and wake. The rows are in
`scripts/hil-tests.conf`; `./scripts/sim-run.sh --app <name>` runs one app's.
