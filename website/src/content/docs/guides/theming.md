---
title: "Theming"
description: "Override the default Picodroid color palette via the Theme fields and apply GradientDrawable backgrounds for per-widget styling."
---

Apps customize their look by assigning the process-wide `Theme` color fields in `Application.onCreate` and by attaching `GradientDrawable` backgrounds to individual views. There is no XML resource system — themes are configured in Java and applied imperatively.

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

The palette is process-global (picodroid is single-app), so views read these fields at
construction time. Views don't cascade automatically — a view applies a theme color explicitly,
e.g. `view.setBackgroundColor(Theme.colorBackground)` or `label.setTextColor(Theme.colorPrimary)`.

### Theme color fields

| Field | Where it shows up |
|---|---|
| `colorPrimary` | Active button background, focus rings, accent strokes. |
| `colorBackground` | The root window background under all activities. |
| `colorSurface` | Container fills (cards, dialogs, list rows). |
| `colorText` | Primary text foreground. |
| `colorTextSecondary` | De-emphasized text — captions, hints, disabled text. |
| `colorOutline` | Borders on `EditText`, `Button` outlines, separators. |
| `colorOnPrimary` | Foreground on top of `colorPrimary` (e.g. button label color). |

The fields ship with sensible dark-palette defaults; apps that don't reassign them get those defaults.

## Per-widget styling: `GradientDrawable`

For backgrounds that don't fit the theme palette directly (rounded cards, gradients, stroked borders), build a `GradientDrawable` and attach it as the view's background:

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

## Worked example

The `displaydemo` widget sampler ends with a themed-widgets section that walks the full palette + gradient pipeline (gradient header, surface card, pill / ghost buttons). See [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo).

`examples/picoenvmon/` is a more realistic application — it customizes the global theme in `Application.onCreate` and uses gradients sparingly for mood. See [`examples/picoenvmon/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon).
