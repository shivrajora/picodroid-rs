---
title: "Button-only boards: focus & navigation"
description: "Make widgets focusable, build a keypad focus group, and handle A/B/X/Y on touchless hardware-button boards like the Pico Enviro Mon."
---

On a board with a touchscreen, navigation just works: tap a button, tap a list row, the framework dispatches the click. On a board with only hardware buttons and no touch — like the [Pico Enviro Mon](/examples/) — there is no pointer at all. The entire touch HAL is compiled out, so apps receive no pointer events. Navigation is keypad-only: the user moves a focus highlight between widgets with the physical buttons.

That changes two things for you as a Java developer:

- Widgets only receive key events and focus if you make them **focusable** (`View.setFocusable(true)`).
- The board maintains a **focus group** — an ordered ring of focusable widgets that the up/down buttons traverse. Each Activity gets its own group.

Everything else is the same Android API you already know. List rows are focusable for free, and `OnItemClickListener` fires identically whether the row was activated by a tap or by a button press.

## The button map

The Pico Enviro Mon wires the four Pimoroni Enviro+ buttons (A/B/X/Y on GP12–GP15) in its `board.toml`:

```toml
# Pimoroni Enviro+ Pack hardware buttons (A/B/X/Y → GP12-15).
# Semantic mapping: PREV→UP, NEXT→DOWN, ENTER→CENTER, ESC→BACK.
[[button]]
pin = 12
lv_key = "PREV"
keycode = 19

[[button]]
pin = 13
lv_key = "NEXT"
keycode = 20

[[button]]
pin = 14
lv_key = "ENTER"
keycode = 23

[[button]]
pin = 15
lv_key = "ESC"
keycode = 4
```

Each entry binds a GPIO pin to an LVGL keypad key (`lv_key`) and an Android `KeyEvent` keycode. That produces the standard 4-button convention:

| Button | Pin | `lv_key` | `keycode` | `KeyEvent` | Role |
|---|---|---|---|---|---|
| A | 12 | `PREV` | 19 | `KEYCODE_DPAD_UP` | up / previous focusable |
| B | 13 | `NEXT` | 20 | `KEYCODE_DPAD_DOWN` | down / next focusable |
| X | 14 | `ENTER` | 23 | `KEYCODE_DPAD_CENTER` | open / activate focused |
| Y | 15 | `ESC` | 4 | `KEYCODE_BACK` | back |

So **A = up, B = down, X = open (ENTER), Y = back (ESC)**. A single `[[button]]` entry is enough for the board to count as button-capable; the keypad focus-group machinery turns on automatically.

For the full `board.toml` schema — required keys, valid `lv_key` values, how `[[button]]` differs from `[touch]` — see the [porting guide](/reference/porting-guide/). The board files live in [`platforms/rp/boards`](https://github.com/shivrajora/picodroid-rs/tree/main/platforms/rp/boards).

## Making widgets reachable

A plain `View` is **not** focusable (the Android default). On a button board that means it never receives key events and the up/down buttons skip over it. Make it focusable and, if it should start selected, request focus:

```java
import picodroid.widget.Button;

Button save = new Button(this);
save.setText("Save");
save.setFocusable(true);     // joins this Activity's keypad focus group
save.requestFocus();         // make it the initially-focused widget
```

The signatures and contract come straight from Android (`sdk/java/picodroid/view/View.java`):

- `public void setFocusable(boolean focusable)` — "Set whether this view can take input focus. On a hardware-button device, only a focusable view receives key events — call this (and `requestFocus()`) on the view that owns the `OnKeyListener`. Focusability is independent of `setOnKeyListener`, exactly as in Android."
- `public boolean requestFocus()` — returns `false` (without effect) if the view is not focusable, otherwise `true` if it became the focused view.
- `public boolean isFocusable()` / `public boolean isFocused()` — `isFocused()` is true iff this view is the active group's focused widget.
- `public boolean hasFocus()` — Picodroid focuses leaf widgets directly, so this is equivalent to `isFocused()`.

You can also observe focus changes, which is how you'd react to the highlight moving:

```java
edit.setOnFocusChangeListener((v, hasFocus) -> {
    if (hasFocus) showHint();
});
```

The `OnFocusChangeListener` "fires when this view gains or loses input focus (on a hardware-button device, as PREV/NEXT move the keypad focus highlight between focusable views)." The view must be focusable (or an adapter row) to ever fire it.

### Focus groups are per-Activity

The board keeps one focus group per Activity, mirroring Android's per-Window focus scope:

- Focusable widgets join the group **in the order they're added**. PREV (A) and NEXT (B) traverse them in that order, wrapping around at the ends.
- A backgrounded Activity's focus is retained untouched and restored when it returns to the top — you don't manage this.
- One Activity can never traverse into another's focus.

The practical takeaway: add your focusable widgets to the layout in the order you want A/B to walk them, and call `requestFocus()` on the one that should be selected first.

## Handling keys

Most apps never write an `OnKeyListener` — the focus group handles A/B navigation and X activation for you, and Y runs the back chain. Reach for `setOnKeyListener` only when a focused widget needs custom key behavior:

```java
import picodroid.view.KeyEvent;

view.setOnKeyListener((v, event) -> {
    if (event.getKeyCode() == KeyEvent.KEYCODE_DPAD_CENTER
            && event.getAction() == KeyEvent.ACTION_UP) {
        activate();
        return true;   // consume — don't fall through to onBackPressed
    }
    return false;
});
```

`OnKeyListener` is the Android single-method interface `boolean onKey(View v, KeyEvent event)`; returning `true` consumes the event. Remember: **only a focusable view receives key events**, and focusability is independent of whether you've set a key listener.

The `KeyEvent` constants match Android exactly (`sdk/java/picodroid/view/KeyEvent.java`):

```java
ACTION_DOWN        = 0;
ACTION_UP          = 1;
KEYCODE_BACK       = 4;
KEYCODE_DPAD_UP    = 19;
KEYCODE_DPAD_DOWN  = 20;
KEYCODE_DPAD_LEFT  = 21;
KEYCODE_DPAD_RIGHT = 22;
KEYCODE_DPAD_CENTER = 23;
```

### BACK (Y) routing order

When Y is released, the framework tries each of these in order and stops at the first that consumes it:

1. **Dismiss the soft keyboard** if it's visible (the only way to close it without touch).
2. **Dismiss a showing `AlertDialog`** — the only way to dismiss a dialog on a keypad-only board.
3. **The focused view's `OnKeyListener`** (if it returns `true`).
4. **`Activity.onBackPressed()`** on the top Activity, whose default body is `finish()`.

Override `onBackPressed()` without calling `super` to intercept Back (see the hub pattern below).

:::caution[Buttons don't fire in the host simulator]
On the host simulator, the hardware GPIO drain always returns `None`, so the Java key dispatcher never fires for real button events — end-to-end button testing needs hardware. The sim instead drives the LVGL keypad indev directly: keyboard keys (and a headless control FIFO) map to button edges, so focus navigation, ENTER, and the ESC back-chain still work for manual testing. See the headless-sim section of the [debugging guide](/guides/debugging/).
:::

## Lists and menus

`ListView` rows are focusable automatically — you don't call `setFocusable` on them. A/B move the row highlight, X (ENTER) activates the highlighted row. The activation path is unified: a row reached by ENTER on a button board and a row tapped in the touch sim both fire the **same** `onItemClick`.

```java
ListView menu = new ListView();
menu.setSize(224, 188);
menu.setAdapter(new ArrayAdapter<String>(LABELS));  // LABELS = {"Live", "History", "Settings"}
menu.setOnItemClickListener(
    (parent, view, position, id) -> startActivity(new Intent(DESTINATIONS[position])));
root.addView(menu);
```

This is the Pico Enviro Mon `HomeActivity` hub pattern: a selectable menu of destinations. A/B move the highlight (with wrap-around — the focus group cycles, so down past the last row returns to the first), X opens the highlighted screen. Two details worth copying:

- **Hold the `ListView` in a field.** The GC then roots the menu through the Activity, in addition to the native item-click listener map — defense-in-depth so an unfielded callback view isn't swept while it's still on screen.
- **Suppress Back on the root hub.** The home screen has nowhere to return to, so override `onBackPressed()` as a no-op (deliberately *not* calling `super`) — otherwise Y would `finish()` the last Activity and exit the app:

```java
@Override
public void onBackPressed() {
    // no-op: the root hub can't be backed out of
}
```

:::caution[Cap data lists at ~12 rows]
Each `lv_list` button row consumes the board's small 48 KB LVGL render pool. Past roughly a dozen focusable rows the renderer runs out of memory. The Pico Enviro Mon caps history at `MAX_ROWS = 12` and shows only the most recent window. Keep data-driven lists capped (~12) on 48 KB-render-pool boards. There's also a per-app limit of 16 ListViews with item-click listeners.
:::

For a worked multi-screen example using this hub + `startActivity` pattern, see the [multi-screen app tutorial](/tutorials/multi-screen-app/).

## Focus styling

The keypad sets **two** state bits on the focused widget, and you must style both:

- `LV_STATE_FOCUSED` — focused (set for both pointer and keypad focus).
- `LV_STATE_FOCUS_KEY` — additionally set when focus arrives via a keypad rather than a pointer.

`ListView` rows handle this for you: the framework fills the focused row with the accent color for both states, so the highlight is unmistakable on dark backgrounds. But if you theme focus yourself on a custom widget and cover only `LV_STATE_FOCUSED`, **keypad focus will show nothing useful** — the default theme repaints the keypad-focused widget blue after the first move (it renders your color on first paint, then the theme's blue once `LV_STATE_FOCUS_KEY` kicks in). Whenever you override the focus highlight, cover the `*_FOCUS_KEY` state too. (This was the QA #7 lesson: a row styled only `LV_STATE_FOCUSED` showed teal on first render and blue after.)

There is no dedicated Java "focus highlight color" setter; list rows get the framework default automatically, and `View.setBackground(Drawable)` dispatches virtually for custom cases. See [theming](/guides/theming/) for palette fields.

A practical tip on a touchless device: tell users what each button does. The Pico Enviro Mon shows an always-visible `ButtonHintBar` legend on every screen, e.g. `"A:Up  B:Down  X:Open"` (note the home hub omits `Y:Back` because Back is suppressed there). The legend bar is `224 px` wide — keep hint strings short enough to fit `224 px` or they clip (the QA #6 lesson).

## Keyboard on button boards

`EditText` is single-line on these boards, and that's load-bearing. On a keypad board the **X (ENTER)** press that opens the soft keyboard would otherwise also insert a newline — the cursor jumps to an empty second line and the field looks cleared (its text becomes e.g. `"30\n"`). The framework sets the textarea one-line so ENTER no longer inserts and the value stays put (QA #5).

The real input API is the Android one:

```java
import picodroid.text.InputType;

EditText field = new EditText(this);
field.setFocusable(true);
field.setInputType(InputType.TYPE_CLASS_NUMBER);  // shows the digit keypad
```

Fields flagged `TYPE_CLASS_NUMBER` get the numeric keypad layout; everything else gets the default text layout. Dismiss the soft keyboard with Y/BACK — on a keypad-only board it's the only way to close it (the keyboard is consumed first in the BACK routing order above, so the user stays on the same screen). See [UI components](/api/ui/) for the full `EditText` / keyboard surface.

:::note[Contributors: pre-commit does not compile this path]
`scripts/pre-commit` does **not** build `board-pico-enviro-mon`. Its board loop only iterates the testbench (touch) boards plus a tdeck and sim build, so the entire `has_buttons` code path — keypad indev, per-Activity focus groups, the phantom-release IRQ filter — is not compiled by the standard suite. The pure-logic unit tests (e.g. the phantom-release filter) do run under `scripts/test.sh`, but they don't exercise the `has_buttons` cfg. To compile-check the button path, run clippy/check against `board-pico-enviro-mon` explicitly.
:::

## See also

- [Embedded gotchas](/guides/embedded-gotchas/) — memory budgets and other constraints behind the row cap.
- [Multi-screen app tutorial](/tutorials/multi-screen-app/) — the hub + `startActivity` pattern end to end.
- [Limits](/reference/limits/) — focus-group depth, listener counts, and other fixed ceilings.
- [UI components](/api/ui/) — `View`, `ListView`, `EditText`, and the focus API reference.
