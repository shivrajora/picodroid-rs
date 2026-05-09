---
title: "Theming"
description: "Override the default Picodroid color palette via Display.setTheme and apply GradientDrawable backgrounds for per-widget styling."
---

Apps customize their look by setting a process-wide `Theme` in `Application.onCreate` and by attaching `GradientDrawable` backgrounds to individual views. There is no XML resource system — themes are constructed in Java and applied imperatively.

## Setting the global theme

```java
import picodroid.app.Application;
import picodroid.graphics.Color;
import picodroid.graphics.Display;
import picodroid.graphics.Theme;

public final class MyApp extends Application {
    @Override
    public void onCreate() {
        Theme dark = new Theme.Builder()
            .setColorPrimary(Color.rgb(0x6b, 0x4e, 0xc5))
            .setColorBackground(Color.rgb(0x0e, 0x0e, 0x14))
            .setColorSurface(Color.rgb(0x1a, 0x1a, 0x24))
            .setColorText(Color.WHITE)
            .setColorTextSecondary(Color.rgb(0xc8, 0xb8, 0xee))
            .setColorOutline(Color.rgb(0x44, 0x44, 0x55))
            .setColorOnPrimary(Color.WHITE)
            .build();
        Display.setTheme(dark);
    }
}
```

The theme is applied process-wide. Calling `Display.setTheme` again later in the run propagates to every active widget on the next layout pass.

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

Defaults track an Android-leaning light palette. Apps that don't call `Display.setTheme` get the framework defaults.

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

Two-color gradients:

```java
GradientDrawable g = new GradientDrawable(
    GradientDrawable.Orientation.TOP_BOTTOM,
    Color.rgb(0x6b, 0x4e, 0xc5),
    Color.rgb(0x2e, 0x1a, 0x4a));
g.setCornerRadius(8);
header.setBackground(g);
```

`Orientation` constants: `TOP_BOTTOM`, `BOTTOM_TOP`, `LEFT_RIGHT`, `RIGHT_LEFT`, plus the four diagonals (`TR_BL`, `BR_TL`, `BL_TR`, `TL_BR`).

## Worked example

The `displaydemo` widget sampler ends with a themed-widgets section that walks the full palette + gradient pipeline (gradient header, surface card, pill / ghost buttons). See [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo).

`examples/picoenvmon/` is a more realistic application — it customizes the global theme in `Application.onCreate` and uses gradients sparingly for mood. See [`examples/picoenvmon/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/picoenvmon).
