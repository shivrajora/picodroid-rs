---
title: "Theming"
description: "Override the default Picodroid color palette via the Theme fields and apply GradientDrawable backgrounds for per-widget styling."
---

Apps customize their look by assigning the process-wide `Theme` color fields in `Application.onCreate` and by attaching `GradientDrawable` backgrounds to individual views. There are no XML styles or themes (`style=`, `?attr/…`): the palette is configured in Java and applied imperatively. Individual colours can still live in `res/values` and be used from layouts and code — see [resources](/guides/resources/).

## Setting the global theme

`Theme` is a holder of `public static int` color fields. Assign them **before any UI is built**
— typically the first thing in `Application.onCreate`:

```java
import picodroid.app.Application;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;

public final class MyApp extends Application {
    @Override
    public void onCreate() {
        Theme.colorPrimary        = Color.rgb(0x6b, 0x4e, 0xc5);
        Theme.colorBackground     = Color.rgb(0x0e, 0x0e, 0x14);
        Theme.colorSurface        = Color.rgb(0x1a, 0x1a, 0x24);
        Theme.colorText           = Color.WHITE;
        Theme.colorTextSecondary  = Color.rgb(0xc8, 0xb8, 0xee);
        Theme.colorOutline        = Color.rgb(0x44, 0x44, 0x55);
        Theme.colorOnPrimary      = Color.WHITE;

        startActivity(new picodroid.content.Intent(MyActivity.class));
    }
}
```

The palette is process-global — static fields, not per-Activity; one app runs at a time — and
is read when a view is built, so a later assignment does not repaint what is already on screen.
Views don't cascade automatically — a view applies a theme color explicitly, e.g.
`view.setBackgroundColor(Theme.colorBackground)` or `label.setTextColor(Theme.colorPrimary)`.
The widgets that read the palette themselves are the progress indicators: an indeterminate
`ProgressBar` spins in `colorPrimary`, and a `CircularProgressIndicator` draws its indicator in
`colorPrimary` over a `colorOutline` track, until a tint says otherwise.

### Theme color fields

| Field | What it is for |
|---|---|
| `colorPrimary` | Primary accent: button fill, focused outlines, slider track. |
| `colorBackground` | Page background. |
| `colorSurface` | Card / surface background, slightly lighter than `colorBackground`. |
| `colorText` | Primary body text. |
| `colorTextSecondary` | Secondary, muted text. |
| `colorOutline` | Subtle separators and divider lines. |
| `colorOnPrimary` | Text and icons on top of `colorPrimary` (e.g. a button label). |

The fields ship with sensible dark-palette defaults; apps that don't reassign them get those defaults.

## Per-widget styling: `GradientDrawable`

`LinearLayout`, `FrameLayout` and `RadioGroup` are flat, as on Android: no background, border, corner radius or padding until you set one. So a card's look is yours to give. For a plain fill `setBackgroundColor` is enough, and it honours the colour's alpha (`Color.TRANSPARENT` clears a background). For anything else (rounded cards, gradients, stroked borders), build a `GradientDrawable` and attach it as the view's background:

```java
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;

GradientDrawable bg = new GradientDrawable();
bg.setColor(Color.rgb(0x1a, 0x1a, 0x24));
bg.setCornerRadius(12);
bg.setStroke(2, Color.rgb(0x44, 0x44, 0x55));   // 2 px outline

card.setBackground(bg);
```

Two-color gradients — there is no gradient constructor; start from `new GradientDrawable()` and
call `setGradient(startColor, endColor, orientation)` (the setters return the drawable so they
chain):

```java
GradientDrawable g = new GradientDrawable()
    .setGradient(Color.rgb(0x6b, 0x4e, 0xc5),
                 Color.rgb(0x2e, 0x1a, 0x4a),
                 GradientDrawable.Orientation.TOP_BOTTOM)
    .setCornerRadius(8);
header.setBackground(g);
```

`Orientation` has just two constants: `TOP_BOTTOM` (1) and `LEFT_RIGHT` (2). Other angles and
radial gradients are not supported.

## Text size

`TextView.setTextSize(float)` (and `android:textSize` in a layout) picks the size of a label's or a button's text. The faces are bitmaps compiled into the firmware, so the size snaps to the nearest one the board has — `14`, `20`, `28` and `64` px on the RP2350 boards, `14` alone on `testbench_rp2040` — a tie going to the larger; `getLineHeight()` reports the face in use. There is one typeface (Montserrat) and no bold or italic: `textStyle` and `fontFamily` are dropped from a layout with a build warning. The sizes above 14 are ASCII plus `°` and `•`.

```java
title.setTextSize(28);
caption.setTextColor(Theme.colorTextSecondary);
```

## Worked example

The `displaydemo` widget sampler ends with a themed-widgets section that walks the full palette + gradient pipeline (top-to-bottom gradient header, left-to-right gradient bar, surface card, pill / ghost buttons). See [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo).

`examples/picoenvmon/` is a more realistic application — it customizes the global theme in `Application.onCreate` and uses gradients sparingly for mood. See [`examples/picoenvmon/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon).
