# QA results: UI rendering across the apps, in the simulator — 2026-10-22

**For:** the session that fixes what this round found. **Against:** `main` at `e91d85cd` (the
tree after the ListView recycling merge), nothing committed on top; the 3 AM nightly cron was
commented out for the night so the simulator had the machine (restore it by removing the
`# PAUSED 2026-10-22 …:` prefix in `crontab -e`). **Where:** the simulator only, four panel
geometries: `testbench_rp2350` (320×240, touch, soft nav), `pico_enviro_mon` (240×240, four keys),
`pico_touch_kit` (320×480, touch and two buttons), `pico_display2_w` (320×240, four keys). Nothing
here touched a board. **Looking for:** rendering defects — text cut or overlapping, widgets
clipped at an edge, spacing that is not what the layout asked for — not behaviour.

Every capture and its simulator log are under `build/qa/ui-render-2026-10-22/shots/`
(`<name>.png` and `<name>.log`, gitignored; the batch scripts and their logs under `_scripts/`).
Captures at 240×240 and 320×240 are at 2× (a coordinate read off them is half of what you see),
the 320×480 touch kit at 1×; `z-*.png` are enlarged crops. Names: `p1-tb-<app>` the first
screen on the testbench, `p2-tb-*` the testbench after input, `p3-em-*` / `p3-tk-*` / `p3-d2w-*`
the other three panels, `p4-*` the fix-up captures.

## Summary

Forty-two apps on the testbench (every example and system app that puts a View up), then the
ones with the richest screens driven through their dialogs, menus, keyboard, pickers, lists,
snackbars and child Activities, then the same set on the three other panels: 160 captures
read. The stock widgets, the fonts at every size the apps use, the keyboard, the lists, the
pickers, the launcher and the settings screens render cleanly on all four geometries, and
nothing overlaps that the app did not ask to overlap. The round found ten things, five of them
worth fixing before the next release:

| # | Severity | What |
|---|---|---|
| U1 | **medium** | A `wrap_content` `TextView` wider than its parent is cut at the parent's edge instead of wrapping to the parent's width as Android does: gesturedemo's hint on every panel, claudeusage's status lines on the 240 panel |
| U2 | **medium** | `CheckBox`, `RadioButton` and the `SeekBar` thumb draw past the left edge of their own bounds: at a parent's left edge the box, the ring and the knob lose their first pixels (callbacktest, every panel; displaydemo's second bar on the touch kit) |
| U3 | **medium** | A `ScrollView` that scrolls paints its scrollbar track opaque over the rightmost 6 px of its content: Settings → Apps with six apps loses the last glyph of every `v1.0`, and the keys-board focus border loses its right side under the bar (fine with one app, when nothing scrolls) |
| U4 | **medium** | A `ViewGroup` created without a size inside a container keeps LVGL's 160×160 default: fragmentdemo's detail text is cut at 160 px, mid-glyph, with no wrap and no ellipsis (the B1 fix covered content roots only) |
| U5 | **medium** | A three-button `AlertDialog` on a 320- or 240-wide panel draws a horizontal scrollbar across the bottoms of its buttons: `54 × 3 + 2 × 10` of buttons and gaps in a 176 px row |
| U6 | low | List dialogs and the options menu reserve the 50 px button row they never show, and the three-button dialog keeps about 90 px of blank card under its buttons: the card is sized by formula, not by its content |
| U7 | low | A `GridLayout` column comes out narrower than its widest cell plus the margins the cells ask for: picoenvmon's Live screen puts `Air quality` 3 px from `210 IAQ` where the layout asks for 8 |
| U8 | low | A focused `TimePicker` on a keys board shows a 2 px blue bar at the left of each roller and nothing on the other three sides |
| U9 | note | Choice dialogs draw a checked row as a filled blue button and an unchecked one dark, with no check box or radio mark: not what Android's `CheckedTextView` rows look like; the compatibility matrix does not say so |
| U10 | note | The `[layout]` line misses two cuts: a custom `ViewGroup` keeps LVGL's SCROLLABLE flag so the walk stops at it (layoutdemo's `FlowLayout` is cut on the 240 panel under `fit ok`), and an ellipsized `Button`'s inner label reports its full text width (ellipsizedemo) |

Known and not re-reported: the soft-nav BACK control covers the bottom-left corner of app
content on the testbench (round 1, F10, by design), and it covers the first word of a
`Snackbar` too (`Item deleted` behind the circle, `p2-tb-snack-undo.png`); the system's own
component could sit above the control. Seven demos are 240 wide by their own `setSize` and show
a blank strip on a 320 panel (snackbardemo, pickerdemo, swipedemo, gesturedemo, ellipsizedemo,
animdemo, tutorial_screens); keyboarddemo's 240×100 root cuts its second field on every panel
(`[layout] overflow … cut 60 past the bottom`). Those are the demos' choices, not the
framework's. One false alarm retracted on the way: the `Button` labels that seemed to vanish
behind a dialog's scrim were under the card (`p4-tb-dialog-plain.png` against
`p1-tb-dialogdemo.png`); the scrim dims text and leaves it readable.

## Defects

### U1 — a wrap_content TextView does not wrap to its parent

gesturedemo (`p1-tb-gesturedemo.png`, the same on every panel): the hint `Tap, long-press, or
swipe the box.` is a `TextView` with no size set, in a 240-wide `LinearLayout` with 10 px of
padding. The text runs to 251 px and is cut at the root's edge: `…swipe the bo`. The log agrees:
`[layout] overflow 240x240 in 320x240: content is cut 11 past the right edge, 0 past the bottom
(by a 240x240 layout)`. Android measures a `wrap_content` child with an AT_MOST constraint of the
parent's remaining width, so the label wraps onto a second line; here the label is content-sized
(`LV_SIZE_CONTENT`) and the solver places it as measured.

claudeusage on `pico_enviro_mon` (`p3-em-claudeusage.png`): `Waiting for the network` and `Looking
for the bridge on the LAN` are centred lines wider than the 240 card and are cut at the panel's
edge (`…the networ`, `…bridge on th`). On the 320 panels they fit. menudemo's hint wraps because
round 2's R5 gave it `MATCH_PARENT` by hand; the default should do the same.

Fix shape: when a content-sized label's measured width exceeds the space its parent offers, set
its width to that space (a `LONG_WRAP` label wraps on its own once it has a width).

**Fixed 2026-10-22.** `LinearLayout`'s adapter (`graphics/lvgl/layouts/mod.rs`,
`cap_content_kids`) gives every content-sized child a `max_width` (a row: `max_height`) of the
cross-axis room less its margins, Android's AT_MOST: a label measures its text within it and
wraps, and shrinks back when its text does, where a pixel width would have stuck. An app's own
`setMaxWidth` wins when tighter. gesturedemo's hint is now 207×32 and the walk says `fit ok`;
`hil-tests.conf` has a `gesturedemo` sim row on that line, which failed before the fix. The
claudeusage lines on the Enviro are not this: the app declares a 320×240 design window
(`<supports-screens>`), and the 240 panel shows 240 of it (`window 320x240 @ (0,0) on the
240x240 panel`); the card itself is 320 wide.

### U2 — CheckBox, RadioButton and the SeekBar thumb draw past their left edge

callbacktest (`p1-tb-callbacktest.png`, `z-cbt-left.png`; `p3-em-callbacktest.png`,
`p3-tk-callbacktest.png`, `p3-d2w-callbacktest.png`): the column is `MATCH_PARENT` with no
padding, every child at x = 0. The two Buttons and the Switch at x = 0 show their full rounded
left corners. The CheckBox's box, both RadioButton rings and the SeekBar's knob are cut flat at
x = 0. Measured on the 240 panel's capture: the radio ring is 22 px wide for 24 px tall, so about
2 px of each is drawn left of the view's x. The SeekBar is the larger case: LVGL centres the knob
on the bar's end, so half the knob (about 10 px) lies outside the view on each side (displaydemo's
second bar on the touch kit, `p3-tk-displaydemo.png`, shows the same half knob at x = 10);
Android reserves the thumb's offset inside the view's bounds, so a `SeekBar` at x = 0 shows its
whole thumb.

The marker is placed at `coords.x1 + pad_left` (`lv_checkbox.c:278`) and the theme's checkbox
style sets no padding, so either the object sits left of where the solver put it or the solver
puts it left of its siblings; a probe that logs `lv_obj_get_x` for the checkbox and the button
above it tells which. The knob wants the slider's `pad_left` / `pad_right` set to the knob's
radius (what Android's `thumbOffset` does), with the bar's length reduced to match.

**Fixed 2026-10-22, SeekBar only; the CheckBox and RadioButton part retracted.** A pixel read of
the round's own captures (`p1-tb-callbacktest.png`, `p3-em-callbacktest.png`, rows 90–111 at
x = 0..21) shows the box with the same 2 px, then 1 px corner inset on both sides, and the radio
ring symmetric at 22×22; a `[tree]` dump of the live objects puts the checkbox at x = 0 with its
marker drawn from its own x1 (`lv_checkbox.c:278`, no main-part padding). The enlarged crop
`z-cbt-left.png` misled: a 3 px corner radius at 2× reads as a flat edge beside the ring's
curve. The SeekBar was real: LVGL centres the 30 px knob (the 20 px bar plus the theme's 5 px
knob pad each side) on the indicator's end, 15 px of it past the track; `seek_bar.rs` now
reserves that overhang as the slider's own left and right padding (`thumb_inset`, re-done on a
size change), so the knob at 0 starts at the view's x. Verified by a pixel read of the knob rows
(a full circle from x = 0) and a unit test on the arithmetic.

### U3 — the scrollbar track paints over the content

Settings → Apps with six apps installed (`p2-tb-settings-apps-many.png`, `z-apps-tail.png`): every
row's `v1.0` reads `v1.(`: the last glyph is under a 6 px opaque column at the right edge, the
scrollbar's track. With one app (`p2-tb-settings-apps.png`) the column does not scroll, no bar is
drawn, and `v1.0` is whole. On `pico_display2_w` the root settings list scrolls (Wi-Fi makes it
six rows) and the focused `About` row's border loses its right side under the same bar
(`p3-d2w-settings.png`, `z-settings-scrollbar-focus.png`). The rows are `row_tail.xml`:
`MATCH_PARENT` wide with 8 px of padding each side, the tail at the right. `scroll_view.rs:94`
sets `bg_opa` to `LV_OPA_COVER` on `LV_PART_SCROLLBAR` so the hardware scroll path can repaint
the bar's ends alone; LVGL draws the bar over the content, so an opaque bar occludes whatever the
row put under it. Android overlays its scrollbar translucently and nothing under it is lost.
Storage's rows have the same shape (`app 13 / data 0 KB` at the right) and will lose their `B`
the day a seventh row makes the screen scroll.

Fix shape: a translucent track with an opaque thumb, or a right inset on the scroller's content
equal to the bar's width when the content overflows (what `scrollbarStyle=outsideInset` does on
Android).

**Fixed 2026-10-22.** LVGL draws no track, only the thumb, and six rows in a 240 px column make a
thumb nearly the column's height. `scroll_view.rs` no longer overrides the theme's 40 % thumb
with an opaque one: the pixels under it show through dimmed, as under Android's overlay scrollbar
(and as under a `ListView`'s, which kept the theme's). The opaque thumb had bought the hardware
scroll path a two-ends repaint after a step; a translucent one is repainted end to end
(`hw_scroll.rs::repaint_thumb` already did so), a 5 px column. Verified by a pixel read of the
same six-app Apps screen: at x = 308..312 the bar is now `393c41` over the background and the
`0`'s last column (`a4a5a4`) shows through where the round's capture had `626162` over it.

### U4 — an unsized ViewGroup inside a container is 160×160

fragmentdemo (`p1-tb-fragmentdemo.png`): `DetailFragment.onCreateView` returns a `FrameLayout`
with no size, holding one `TextView`. The pager it lives in is 200×100; the text `Detail 1, built
in onCreateView` is cut at 160 px, mid-glyph (`…built in onCre`), with neither wrap nor ellipsis:
the FrameLayout kept LVGL's default object size. Round 2's B1 made an unsized *content root*
`MATCH_PARENT`; a group below the root still defaults to 160×160. On Android a view with no
`LayoutParams` added to a `FrameLayout` or a pager gets `MATCH_PARENT` from the container's
`generateDefaultLayoutParams`. The `[layout]` line says `fit ok` here: the pager is a scroll
container, so the walk stops at it (U10).

**Fixed 2026-10-22.** `ViewGroup.addView(child)` gives a `ViewGroup` child that was never sized the
group's `generateDefaultLayoutParams()`, now mirrored from Android: `match_parent` both ways from a
`FrameLayout` (so from a `ViewPager2` page), `match_parent` × `wrap_content` from a vertical
`LinearLayout`, `wrap_content` both ways from a horizontal one and the base class, `match_parent`
× `wrap_content` from a `ScrollView` (Android measures the child's height unconstrained; a
viewport-tall child here would not scroll). `LinearLayout.setOrientation` is a Java method now
(`nativeSetOrientation` behind it) so the orientation is known; `getOrientation()` came with it.
Leaf views without params keep their size, as before (the matrix says so). fragmentdemo's detail
reads whole; callbacktest's `RadioGroup`, a 160×160 default before, is 320×66. The pager was not
the reason the walk missed it (U10).

### U5 — a three-button dialog's buttons sit under a scrollbar

dialogdemo's `Delete? / Are you sure?` dialog with `Later`, `Cancel` and `OK`
(`p4-tb-dialog-alert.png` on the testbench, `p4-tb-dialog-alert-em.png` on the Enviro): a grey bar
runs across the lower quarter of all three buttons. It is the button row's horizontal scrollbar:
the row is `dialog_content_width()` wide — the card is `w × 5 / 8` clamped to 200 on a 320 or 240
panel, less 24 px of padding, so 176 — and holds three 54 px buttons with the theme's 10 px flex
gaps, 182 px (`alert_dialog.rs:474–506`). The row is a plain `lv_obj` that keeps
`LV_OBJ_FLAG_SCROLLABLE`, so the 6 px of overflow make it scroll and draw its bar over the
buttons. On the 480-wide touch kit the card is 300 and the row fits. Android lays a dialog's
buttons out in a bar that never scrolls (it stacks them when they do not fit).

Fix shape: size the buttons to the row (`(176 - 2 × gap) / 3` = 52) or the gap to the buttons,
and clear the row's SCROLLABLE flag so a future overflow clips rather than scrolls.

### U6 — a dialog card sized by formula, not by its content

dialogdemo's choice dialogs and the options menu (`p1-tb-dialogdemo.png`, `p2-tb-menu-open.png`,
`p3-em-menu-open.png`, `p3-tk-menu-open.png`): three rows of 28 px, then about 48 px of white card
under them. The card is `90 + 28 × n` px tall, capped at 220 (`alert_dialog.rs:584`); the 90 covers
a title and the 50 px button row `build_dialog_shell` makes, which a list dialog hides. The
three-button dialog (`p4-tb-dialog-alert.png`) has about 90 px of blank card under its buttons
the same way. Android's dialog wraps its content. Cosmetic, but it is the one dialog every app
with a menu shows.

### U7 — a GridLayout column is narrower than its cells ask for

picoenvmon's Live screen (`p2-tb-env-live.png`, `z-env-live-row.png`; the same on
`pico_display2_w` and the Enviro): each tile is a row of a 3-column `GridLayout`, the label in
column 0 with an 8 px left margin, the value in column 1 with 8 px margins and weight 1. The
values all start at x = 93. The widest label, `Air quality`, ends at x = 90, so it and `210 IAQ`
are 3 px apart where the layout asks for 8 (and would give 16 on Android: the label's right edge
plus the value's left margin plus the column boundary). The shorter labels show the intended
gap by accident. The column's width is being taken from the cell's view without one of the
margins.

### U8 — a focused TimePicker shows a blue sliver, not a border

keynav on `pico_display2_w` after two DOWNs (`p3-d2w-keynav-down2.png`, `z-timepicker-focus.png`):
the focus has reached the `TimePicker` and each of its two rollers shows a 2 px blue vertical bar
at its left edge, nothing along the top, bottom or right. Four DOWNs on (`p3-d2w-keynav-down4.png`)
`Pick a colour` carries a full white border, as every other focus stop does. The picker's
container holds the rollers in a flex row with free space at each end, so a border or outline
on the roller is not clipped by the container on its left side; whatever draws the other three
sides is not reaching the panel. The F5 border is meant to be the one thing a keys-board user
has to see where the focus is.

### U9 — choice dialog rows carry no check mark (note)

`setMultiChoiceItems` / `setSingleChoiceItems` rows are LVGL button-matrix cells: checked is a
filled blue cell, unchecked a dark one (`p1-tb-dialogdemo.png`, `p3-em-dialog-single.png`). Android
draws a `CheckedTextView` with a check box or radio at the start of each row. Readable, but a
user who knows Android reads the blue as "selected", not "checked", and the compatibility
matrix's `AlertDialog` row does not mention the look. A note for the matrix, or a glyph
(`LV_SYMBOL_OK`) in the cell text.

**Noted 2026-10-22.** The compatibility matrix's `AlertDialog` row now says the choice rows are
button-matrix cells with no check mark; the glyph is not drawn (a `LV_SYMBOL_OK` in the cell text
would sit in the row's label and stay there when the row is unchecked, since the matrix has one
text per cell).

### U10 — two cuts the `[layout]` line does not see (note)

- layoutdemo on `pico_enviro_mon` (`p3-em-layoutdemo.png`): the `FlowLayout`'s `item0 … item4`
  column sits at x = 200 and is cut by the 240 panel (`m1`, `m3` are what is left), under
  `[layout] fit ok 240x240 in 240x240`. A custom `ViewGroup` is a plain `lv_obj`, which keeps
  `LV_OBJ_FLAG_SCROLLABLE`, and `reach()` stops at a scrollable object (display.rs:339). Clearing
  the flag on a `ViewGroup` that is not a `ScrollView` would let the walk through, and matches
  Android, where a `ViewGroup` does not scroll unless it is one that does.
- ellipsizedemo (`p1-tb-ellipsizedemo.log`): `content is cut 177 past the right edge … (by a
  96x36 layout)` for the 96-wide single-line ellipsized `Button`: its inner label keeps its full
  273 px text width while the dots mode draws it cut, so the walk counts a cut the eye does not
  see. The app's own checks pass.

**Fixed 2026-10-22, the second; the first retracted.** `reach()` no longer descends into a
button: its label is its own content (the `[tree]` dump shows the 96 px button holding a 449 px
label centred under it). ellipsizedemo's line is `fit ok` and its sim row pins that. The
layoutdemo item was not a missed cut: on the Enviro the `FlowLayout` sits at x = 140..239, inside
the panel, and what hides `item1`'s first letters is the demo's own 200 px stage card, a later
sibling drawn over it (the demo lays its stages out for 320); a custom `ViewGroup` is made by
`frame_layout::create`, which clears `SCROLLABLE`, so the walk goes through it.

## Scenario results

What was captured, by panel, and what each showed. "clean" means nothing cut, overlapping or
misplaced beyond the known soft-nav overlap.

### testbench_rp2350 (320×240, touch, soft nav)

| Capture | Result |
|---|---|
| `p1-tb-*` first screens, 42 apps (alarmdemo, bugbash_ui and qa_ui end before a capture) | clean except gesturedemo (U1), callbacktest (U2), fragmentdemo (U4), dialogdemo (U6, U9); keyboarddemo and the 240-wide demos as noted above |
| `p4-tb-dialog-plain`, `-alert`, `p2-tb-dialog-*` | the bare screen, the message dialog (U5, U6), the choice dialogs (U9): titles, rows and labels clean |
| `p2-tb-snack-undo`, `-indef` | text and action button clean at 320 wide; the BACK control covers `Item` |
| `p2-tb-keyboard`, `-typed` | the 320-wide keyboard: all four rows, every key label, the shift state; the field's text follows |
| `p2-tb-menu-open` | the options menu: three rows clean; U6 |
| `p2-tb-picker-time` | the `TimePicker` after the scroll: hour and minute rollers, the toggle button |
| `p2-tb-calc-long`, `-product` | twelve 7s right-aligned, then `77777777 x 999999 =` over `77777699222223`: no clip, no overlap |
| `p2-tb-list-scrolled` | `ListView` after a drag: rows 10–15, the selected row's band, the scrollbar |
| `p2-tb-tut-counter`, `-about` | child Activities: a two-line body, buttons |
| `p2-tb-clock-alarms`, `-settime` | picoclock's child screens: the header band, `+ New alarm` card, the date steppers and the calendar at 320 wide |
| `p2-tb-cbt-spinner` | the `Spinner` dropdown over the rows below it |
| `p2-tb-env-live`, `-history`, `-network`, `-settings` | picoenvmon's four screens: tiles, a scrolled list, a two-line notice, three `NumberPicker`s and a `Switch`; U7 on Live |
| `p2-tb-settings-about`, `-apps`, `-storage`, `-display`, `-datetime` | the settings screens: header band, rows, right-aligned tails clean |
| `p2-tb-launcher-many`, `p2-tb-settings-apps-many` | six installed apps: the launcher's icons and labels clean; U3 in Apps |

### pico_enviro_mon (240×240, four keys)

| Capture | Result |
|---|---|
| `p3-em-*` first screens, 18 apps | clean except claudeusage (U1), callbacktest (U2), layoutdemo (U10), dialogdemo (U6, U9) |
| `p3-em-env-*` | the four picoenvmon screens at 240 wide: clean, the key legend fits |
| `p3-em-menu-open`, `-dialog-focus`, `-dialog-single`, `p4-tb-dialog-alert-em` | the focus ring on the list and its rows; the message dialog (U5) |
| `p3-em-snack-indef` | the two-line snackbar beside its action (R5's wrap) |
| `p3-em-cbt-focus3` | the focused CheckBox: a border on its box and the theme's outline round box and label, both cut at x = 0 (U2) |
| `p3-em-list-down`, `-picker-down` | the ListView walked by keys, the calendar's focused day |
| `p3-em-launcher`, `-settings*` | the launcher and settings rows with the key focus border (the key sequences for About, Display and Date & time landed on Apps, Date & time and the launcher; the screens themselves are the testbench's) |

### pico_touch_kit (320×480, touch, two buttons)

| Capture | Result |
|---|---|
| `p3-tk-*` first screens, 15 apps | clean except callbacktest and displaydemo (U2), dialogdemo (U6, U9); the 320×240 apps (claudeusage, pickerdemo, snackbardemo, tutorial_screens, keyboarddemo) sit in their design window on the taller panel |
| `p3-tk-clock-alarms`, `-settime` | picoclock's child screens at full height: the alarm card, the date steppers, the whole calendar and the time rollers below it |
| `p3-tk-env-live`, `-history` | the tiles before the first sample (`--`), a 29-row history list |
| `p3-tk-dialog-list`, `-alert` | the taps landed on the choice dialog's rows (the app shows two dialogs at start); the rows and the card are clean |
| `p3-tk-keyboard` | the keyboard at the bottom of the 480 panel, the field above it |
| `p3-tk-snack-indef`, `-menu-open` | the snackbar at the panel's bottom; the options menu (U6) |
| `p3-tk-launcher`, `-settings*` | the launcher, the six-row settings root with Wi-Fi, About, Wi-Fi (`Not connected`, `Scan for networks`) and Date & time (`03:25`): clean |

### pico_display2_w (320×240, four keys)

| Capture | Result |
|---|---|
| `p3-d2w-*` first screens, 12 apps | clean except callbacktest (U2), dialogdemo (U6, U9); claudeusage's `Limits` page (two gauges, `5%` and `42%`) clean |
| `p3-d2w-keydemo-down`, `-keynav-down2`, `-down4` | the focus border on `Row 1`; U8 on the TimePicker; the border on `Pick a colour` |
| `p3-d2w-menu-open`, `-dialog-focus`, `-snack-indef` | the menu list, the dialog's focused row, the snackbar with the focused button above it: clean |
| `p3-d2w-cbt-focus3`, `-list-down8` | the focused CheckBox (U2), the ListView walked eight rows down |
| `p3-d2w-env-live` | picoenvmon's tiles with live values; U7 |
| `p3-d2w-launcher`, `-settings`, `-settings-wifi` | the launcher's focused row; the six-row settings root (U3 on the focus border); the Display screen (the key sequence stopped one row short of Wi-Fi) |

## How to continue

The five medium findings are framework fixes with a capture each as the before/after (U1
gesturedemo, U2 callbacktest, U3 the settings Apps screen with six APKs under
`PICODROID_SIM_APPS`, U4 fragmentdemo, U5 dialogdemo after two BACKs and a tap at 110,76); each
wants a `qa_ui` or example-app row that failed before the fix. U6 and U7 are size calculations.
U8 wants a look at what the roller's focus style draws before a fix. The batch scripts under
`_scripts/` re-run any capture by name.
