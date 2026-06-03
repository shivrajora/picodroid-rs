---
title: "Graphics and UI"
description: "Activity lifecycle, widgets, theming, gestures, and animations."
---

Picodroid's Android-inspired UI toolkit. Packages: `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget`. See [Java API overview](/api/) for the full API index.

The toolkit is backed by [LVGL](https://lvgl.io). Apps create an `Application`, start an `Activity`, and build a widget tree. On hardware, the display is driven via SPI (ST7789) with touch input (XPT2046). In the simulator, a graphical window (minifb) renders the UI with mouse-as-touch input.

## `picodroid.app.Application`

Base class for all apps. Subclass it and override `onCreate()`.

```java
import picodroid.app.Application;
import picodroid.content.Intent;

public class MyApp extends Application {
    public void onCreate() {
        // Console app: do work here
        // Display app: start an Activity
        startActivity(new Intent(MyActivity.class));
    }
}
```

| Method | Description |
|--------|-------------|
| `onCreate()` | Called by the runtime after instantiation. Override to initialize your app. |
| `startActivity(Intent intent)` | Launches the Activity named by the Intent's target class (`new Intent(MyActivity.class)`). The Activity's `onCreate()` is called after the display is ready. |

## `picodroid.app.Activity`

Base class for display screens. Subclass it, override `onCreate()`, build a widget tree, and call `setContentView()`.

```java
import picodroid.app.Activity;
import picodroid.view.View;
import picodroid.debug.DisplayDebug;

public class MyActivity extends Activity {
    public void onCreate() {
        DisplayDebug.calibrate();     // optional: run touch calibration (debug helper)
        // ... build widget tree ...
        setContentView(rootView);     // render the widget tree
    }
}
```

### Lifecycle

The full Android-style lifecycle is dispatched by the runtime. Override only the callbacks you need.

| Callback | When |
|----------|------|
| `onCreate()` | Once, after instantiation. Build the UI tree here. |
| `onStart()` | After `onCreate`, and on every return to the foreground. |
| `onResume()` | Immediately after `onStart`; the Activity is now interactive. |
| `onPause()` | When another Activity is being launched on top. |
| `onStop()` | After `onPause`, once the new top Activity is fully resumed. |
| `onDestroy()` | Just before this Activity is popped off the stack. |
| `onBackPressed()` | BACK-key default action — calls `finish()`. Override and don't `super.onBackPressed()` to suppress (e.g. show a confirm dialog). |

The content view installed in `onCreate` (or `onResume`) is **preserved across pause** — when this Activity returns to the foreground, the saved widget tree is restored automatically. Rebuilding the tree from `onResume` is still supported; the new root replaces the saved one.

### Back stack

| Method | Description |
|--------|-------------|
| `startActivity(Intent intent)` | Push the Activity named by `new Intent(TargetActivity.class)` onto the stack. Triggers this.onPause → newActivity.{onCreate,onStart,onResume} → this.onStop. |
| `finish()` | Pop this Activity. Triggers onPause → onStop → onDestroy on this Activity, and onStart/onResume on the one below. If the stack is empty after the pop, the app exits. |
| `setContentView(View root)` | Sets the root of the widget tree and renders it to the display. |
| `getDisplay()` | Returns the `Display` singleton. |

See [`examples/navdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/navdemo) for a multi-Activity back-stack demo and [`examples/dialogdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/dialogdemo) for an `onBackPressed` override pattern.

## `picodroid.graphics.Display`

Singleton representing the physical display. Typically accessed via `Activity.getDisplay()`.

```java
import picodroid.graphics.Display;

Display display = Display.getInstance();
int w = display.getWidth();      // e.g. 320
int h = display.getHeight();     // e.g. 240

display.setContentView(root);    // set root widget
display.update();                // refresh the display
```

The `Display` surface is intentionally minimal and Android-shaped. Picodroid-only helpers
(touch calibration, the FPS overlay, pull-mode touch polling) live on
[`picodroid.debug.DisplayDebug`](#picodroiddebugdisplaydebug), not on `Display`.

## `picodroid.debug.DisplayDebug`

Picodroid-specific debug helpers that have no Android equivalent — kept off `Display` so its
surface stays close to `android.view.Display`. All methods are `static`.

```java
import picodroid.debug.DisplayDebug;
import picodroid.view.MotionEvent;

DisplayDebug.calibrate();    // interactive 4-point touch calibration (embedded targets; blocks)
DisplayDebug.showFps();      // toggle the live LVGL FPS overlay (call once in onCreate)
MotionEvent touch = DisplayDebug.pollTouch();  // pull one raw touch sample (null if the queue is empty)
```

| Method | Description |
|--------|-------------|
| `static void calibrate()` | Run the interactive 4-point touch calibration. Blocks until the user finishes. |
| `static void showFps()` | Show the live FPS overlay. Idempotent after the first call. |
| `static MotionEvent pollTouch()` | Poll one raw touch sample; `null` if the queue is empty. The primary touch path is a per-View `OnTouchListener` (below) — `pollTouch` is the pull-mode alternative. |

## `picodroid.graphics.Color`

Color constants and factory methods. All colors are ARGB integers.

```java
import picodroid.graphics.Color;

int white = Color.WHITE;          // 0xFFFFFFFF
int red   = Color.RED;            // 0xFFFF0000
int custom = Color.rgb(128, 0, 255);       // 0xFF8000FF
int semi   = Color.argb(128, 255, 0, 0);   // 0x80FF0000 (50% transparent red)
```

| Constant | Value |
|----------|-------|
| `Color.BLACK` | `0xFF000000` |
| `Color.WHITE` | `0xFFFFFFFF` |
| `Color.RED` | `0xFFFF0000` |
| `Color.GREEN` | `0xFF00FF00` |
| `Color.BLUE` | `0xFF0000FF` |
| `Color.YELLOW` | `0xFFFFFF00` |
| `Color.CYAN` | `0xFF00FFFF` |
| `Color.MAGENTA` | `0xFFFF00FF` |
| `Color.TRANSPARENT` | `0x00000000` |

| Method | Description |
|--------|-------------|
| `Color.rgb(int r, int g, int b)` | Returns an ARGB int with full opacity (alpha=255) |
| `Color.argb(int a, int r, int g, int b)` | Returns an ARGB int with the specified alpha |

## `picodroid.graphics.Theme`

App-wide color palette — static fields apps read at view-construction time. Customise by assigning to these fields **before any UI is built** (typically in `Application.onCreate`):

```java
import picodroid.graphics.Color;
import picodroid.graphics.Theme;

Theme.colorPrimary    = Color.argb(255,  80, 180, 120);
Theme.colorBackground = Color.argb(255,  24,  24,  28);
```

| Field | Default | Use |
|-------|---------|-----|
| `colorPrimary` | bluish accent | button fill, focused outlines, slider track |
| `colorOnPrimary` | white | text/icons on top of `colorPrimary` |
| `colorBackground` | near-black | page background |
| `colorSurface` | dark grey | card / surface background |
| `colorText` | near-white | primary body text |
| `colorTextSecondary` | muted grey | secondary / muted body text |
| `colorOutline` | dark grey | subtle separator / divider line |

picodroid is single-app, so the palette is process-global rather than per-Activity. Views still need to read these values explicitly (`view.setBackgroundColor(Theme.colorBackground)`); there is no automatic cascading.

See the themed-widgets section of [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo) for a worked example.

## `picodroid.graphics.drawable.GradientDrawable`

A configurable shape drawable: solid fill (or two-color linear gradient), optional corner radius, optional stroke. Mirrors the most-used subset of Android's `GradientDrawable`.

```java
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;

GradientDrawable bg = new GradientDrawable()
    .setColor(Color.argb(255, 32, 32, 40))
    .setCornerRadius(16)
    .setStroke(2, Color.WHITE);
view.setBackground(bg);

// Or a vertical gradient
GradientDrawable g = new GradientDrawable()
    .setGradient(Color.BLUE, Color.MAGENTA, GradientDrawable.Orientation.TOP_BOTTOM)
    .setCornerRadius(20);
view.setBackground(g);
```

| Method | Description |
|--------|-------------|
| `setColor(int argb)` | Solid fill. Replaces any previously set gradient. |
| `setCornerRadius(int px)` | Corner radius. Half the smaller dimension renders a pill. |
| `setStroke(int width, int color)` | Border outline. `width = 0` removes. |
| `setGradient(int start, int end, int orientation)` | Two-color linear gradient. Replaces any previously set solid color. |

| Constant | Value |
|----------|-------|
| `GradientDrawable.Orientation.TOP_BOTTOM` | 1 |
| `GradientDrawable.Orientation.LEFT_RIGHT` | 2 |

Multi-stop gradients, angle-arbitrary orientations, and radial gradients are deferred.

## `picodroid.view.View`

Base class for all UI widgets. Not instantiated directly — use subclasses like `TextView`, `Button`, etc.

```java
import picodroid.view.View;

view.setPosition(10, 20);           // x=10, y=20
view.setSize(200, 50);              // width=200, height=50
view.setBackgroundColor(Color.BLUE);
view.setBackground(drawable);       // or apply a Drawable (see GradientDrawable)
view.setVisibility(View.VISIBLE);   // VISIBLE, INVISIBLE, or GONE
view.setEnabled(false);             // grey out / disable interaction
view.animate().alpha(0f, 1f).setDuration(200).start();   // see ViewPropertyAnimator
view.setOnClickListener(v -> doThing());   // View.OnClickListener (fires on tap or D-pad center)
view.setOnTouchListener(listener);  // per-View touch dispatch
view.close();                        // release the native widget
```

| Constant | Value | Description |
|----------|-------|-------------|
| `View.VISIBLE` | 0 | Widget is visible and takes up layout space |
| `View.INVISIBLE` | 1 | Widget is invisible but still takes up layout space |
| `View.GONE` | 2 | Widget is invisible and takes no layout space |
| `View.MATCH_PARENT` | -1 | `LayoutParams` size: fill the parent |
| `View.WRAP_CONTENT` | -2 | `LayoutParams` size: size to content |

### Focus navigation

On button-only devices (no touchscreen), key events route to whichever view is **focused**. Make a
view focusable and give it focus so it receives D-pad / hardware-key input. Mirrors
`android.view.View`.

```java
button.setFocusable(true);          // opt this view into the focus group
button.requestFocus();              // take input focus now (returns false if not focusable)
boolean f = button.isFocused();

button.setOnFocusChangeListener(new View.OnFocusChangeListener() {
    public void onFocusChange(View v, boolean hasFocus) {
        v.setBackgroundColor(hasFocus ? Color.YELLOW : Color.GRAY);
    }
});
```

| Method | Description |
|--------|-------------|
| `setFocusable(boolean)` | Opt this view into the per-Activity LVGL focus group. |
| `requestFocus()` | Request input focus. Returns `false` if the view is not focusable. |
| `isFocusable()` / `isFocused()` / `hasFocus()` | Focus-state queries. |
| `setOnFocusChangeListener(OnFocusChangeListener)` | `onFocusChange(View v, boolean hasFocus)` fires on focus gain/loss. |

See [Key events](#key-events) for handling the keys themselves, and
[`examples/keydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keydemo).

## `picodroid.view.ViewGroup`

Abstract base for every container that holds child views — `LinearLayout`, `FrameLayout`,
`ScrollView`, `SwipeRefreshLayout`, and the adapter views. Extends `View`, so containers also have
position/size/background/visibility. Mirrors `android.view.ViewGroup`.

```java
group.addView(child);                 // append a child
group.addView(child, params);         // append with explicit LayoutParams
int n = group.getChildCount();
View first = group.getChildAt(0);
group.removeView(child);
group.removeAllViews();
```

`ViewGroup.LayoutParams` carries a child's requested `width`/`height`, using `MATCH_PARENT` (-1) or
`WRAP_CONTENT` (-2):

```java
view.setLayoutParams(new ViewGroup.LayoutParams(
    ViewGroup.LayoutParams.MATCH_PARENT,
    ViewGroup.LayoutParams.WRAP_CONTENT));
```

## `picodroid.view.MotionEvent`

Represents a touch event from the display. Delivered to per-View `OnTouchListener`s (the primary path), or pulled via `DisplayDebug.pollTouch()`.

```java
import picodroid.debug.DisplayDebug;
import picodroid.view.MotionEvent;

MotionEvent event = DisplayDebug.pollTouch();
if (event != null) {
    int action = event.getAction();   // ACTION_DOWN, ACTION_UP, ACTION_MOVE, ACTION_LONG_PRESS
    int x = event.getX();
    int y = event.getY();
    long t = event.getEventTime();    // ms timestamp (boot-elapsed)
}
```

| Constant | Value |
|----------|-------|
| `MotionEvent.ACTION_DOWN` | 0 |
| `MotionEvent.ACTION_UP` | 1 |
| `MotionEvent.ACTION_MOVE` | 2 |
| `MotionEvent.ACTION_LONG_PRESS` | 3 (picodroid extension; LVGL long-press) |

## `picodroid.view.OnTouchListener` and `GestureDetector`

Install an `OnTouchListener` to receive raw touch events on a single `View`:

```java
import picodroid.view.MotionEvent;
import picodroid.view.OnTouchListener;
import picodroid.view.View;

view.setOnTouchListener(new OnTouchListener() {
    public boolean onTouch(View v, MotionEvent e) {
        if (e.getAction() == MotionEvent.ACTION_DOWN) {
            // ...
        }
        return true;   // event consumed
    }
});
```

For tap / long-press / fling recognition, wrap an `OnGestureListener` in `GestureDetector` (which itself implements `OnTouchListener`):

```java
import picodroid.view.GestureDetector;
import picodroid.view.MotionEvent;
import picodroid.view.View;

view.setOnTouchListener(new GestureDetector(new GestureDetector.OnGestureListener() {
    public void onSingleTap(MotionEvent e) { Log.i("UI", "tap @ " + e.getX()); }
    public void onLongPress(MotionEvent e) { Log.i("UI", "long press"); }
    public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) {
        Log.i("UI", "fling vx=" + vx + " vy=" + vy);
    }
}));
```

| Constant | Value | Meaning |
|----------|-------|---------|
| `GestureDetector.TAP_SLOP_PX` | 12 | Max DOWN→UP displacement to count as a tap. |
| `GestureDetector.FLING_MIN_PX` | 24 | Min DOWN→UP displacement to count as a fling. |

Velocities are pixels/second; positive `vx` is rightward, positive `vy` is downward. v1 caveats: no `ACTION_MOVE` / scroll callbacks; multi-touch is not supported. See [`examples/gesturedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/gesturedemo).

## `picodroid.view.ViewPropertyAnimator`

Fluent builder for short interpolated property animations on a single `View`. Obtain via `view.animate()`.

```java
view.animate()
    .alpha(0f, 1f)        // fade in
    .x(20, 60)            // and slide right by 40 px
    .setDuration(250)     // both in 250 ms (default 300 ms)
    .start();
```

| Method | Description |
|--------|-------------|
| `alpha(float from, float to)` | Animate alpha (0.0–1.0). |
| `x(int from, int to)` | Animate horizontal position in pixels. |
| `y(int from, int to)` | Animate vertical position in pixels. |
| `setDuration(int ms)` | Total duration; applies to every queued property. |
| `start()` | Begin every queued property animation. |
| `cancel()` | Cancel every property animation targeting this view. Properties stay at the last interpolated frame. |

v1 caveats: linear interpolation only (no easing curves), no completion listener, both `from` and `to` are required. Multiple property calls in the same chain run concurrently. See [`examples/animdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/animdemo).

## Key events

Hardware buttons declared in [`board.toml`](/reference/porting-guide/#boardtoml-reference) are surfaced through Android-style `KeyEvent`s. Events route to the currently **LVGL-focused** widget — whichever widget holds the focus at the moment of the press. If no widget has focus, the event is dropped.

To receive keys, install an `OnKeyListener` on a focusable widget (a `Button` is the easiest):

```java
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;

Button focus = new Button("Focus me");
focus.setOnKeyListener(new OnKeyListener() {
    public boolean onKey(View v, KeyEvent event) {
        if (event.getAction() == KeyEvent.ACTION_DOWN
                && event.getKeyCode() == KeyEvent.KEYCODE_DPAD_CENTER) {
            // handled
            return true;
        }
        return false;  // let LVGL default nav run
    }
});
```

Return `true` from `onKey` to consume the event; `false` lets LVGL keep processing it (e.g. for default focus navigation).

| Constant | Value |
|----------|-------|
| `KeyEvent.ACTION_DOWN` | 0 |
| `KeyEvent.ACTION_UP` | 1 |
| `KeyEvent.KEYCODE_BACK` | 4 |
| `KeyEvent.KEYCODE_DPAD_UP` | 19 |
| `KeyEvent.KEYCODE_DPAD_DOWN` | 20 |
| `KeyEvent.KEYCODE_DPAD_LEFT` | 21 |
| `KeyEvent.KEYCODE_DPAD_RIGHT` | 22 |
| `KeyEvent.KEYCODE_DPAD_CENTER` | 23 |

The `keycode` each pin emits is declared in `board.toml` — see [Porting Guide → board.toml reference](/reference/porting-guide/#boardtoml-reference) for the full schema. On boards with no buttons (touch-only), `setOnKeyListener` is a no-op.

> **Idle wake:** if the display has gone to sleep (60 s with no input), the first button press wakes the display and is **not** delivered to listeners. Subsequent presses route normally.

See [`examples/keydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keydemo) for a complete example.

## Widgets

All widget classes live in `picodroid.widget.*` and extend `View`. They inherit `setPosition()`, `setSize()`, `setBackgroundColor()`, `setVisibility()`, and `close()` from `View`.

### `picodroid.widget.TextView`

Displays a text label.

```java
import picodroid.widget.TextView;

TextView label = new TextView();
label.setText("Hello, World!");
label.setTextColor(Color.WHITE);
```

### `picodroid.widget.Button`

A clickable button with a text label.

```java
import picodroid.view.View;
import picodroid.widget.Button;

Button btn = new Button("Tap Me!");
btn.setSize(200, 50);
btn.setText("New Label");

// Event-driven click handling — View.OnClickListener (onClick(View v))
btn.setOnClickListener(new View.OnClickListener() {
    public void onClick(View v) {
        Log.i("UI", "Button clicked!");
    }
});
// ...or a lambda, since OnClickListener is a single-method interface:
btn.setOnClickListener(v -> Log.i("UI", "Button clicked!"));

// Or poll-based
boolean clicked = btn.wasClicked();
```

> **Typed listeners (since v0.10.0):** widget callbacks are Android-style single-method
> interfaces, not bare `Runnable`s. Each widget below names its interface — e.g.
> `View.OnClickListener`, `CompoundButton.OnCheckedChangeListener`,
> `SeekBar.OnSeekBarChangeListener`, `AdapterView.OnItemClickListener`,
> `Spinner.OnItemSelectedListener`. Because they are single-method interfaces, a lambda works
> wherever an anonymous class does.

### `picodroid.widget.LinearLayout`

A container that arranges child widgets horizontally or vertically.

```java
import picodroid.widget.LinearLayout;

LinearLayout layout = new LinearLayout();             // default: VERTICAL
layout.setOrientation(LinearLayout.HORIZONTAL);       // or VERTICAL
layout.setSize(320, 240);
layout.addView(textView);
layout.addView(button);
```

| Constant | Value |
|----------|-------|
| `LinearLayout.HORIZONTAL` | 0 |
| `LinearLayout.VERTICAL` | 1 |

### `picodroid.widget.CompoundButton`

Abstract base for the two-state widgets below — `Switch`, `ToggleButton`, and `CheckBox`. Mirrors
`android.widget.CompoundButton`. You don't instantiate it directly; it provides the shared checked
API and listener that each subclass inherits:

| Member | Description |
|--------|-------------|
| `boolean isChecked()` | Current checked state. |
| `void setChecked(boolean)` | Set checked state (does not fire the listener). |
| `void toggle()` | Flip the checked state. |
| `setOnCheckedChangeListener(OnCheckedChangeListener)` | `onCheckedChanged(CompoundButton buttonView, boolean isChecked)` fires on user toggle. |

### `picodroid.widget.Switch`

An on/off toggle switch.

```java
import picodroid.widget.CompoundButton;
import picodroid.widget.Switch;

Switch sw = new Switch();
sw.setSize(60, 30);

boolean on = sw.isChecked();
sw.setChecked(true);
sw.toggle();

sw.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "Switch is now " + isChecked);
    }
});
```

`Switch` extends [`CompoundButton`](#picodroidwidgetcompoundbutton) (the shared base for
`Switch` / `ToggleButton` / `CheckBox`), which is where `isChecked()` / `setChecked()` /
`toggle()` / `setOnCheckedChangeListener()` come from.

### `picodroid.widget.ToggleButton`

A button that toggles between two states with configurable text labels.

```java
import picodroid.widget.CompoundButton;
import picodroid.widget.ToggleButton;

ToggleButton toggle = new ToggleButton("ON", "OFF");  // or new ToggleButton()
toggle.setSize(200, 50);

boolean on = toggle.isChecked();
toggle.setChecked(true);
toggle.toggle();
toggle.setTextOn("Enabled");
toggle.setTextOff("Disabled");

toggle.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "Toggle is now " + isChecked);
    }
});
```

### `picodroid.widget.ImageView`

Displays an image bundled in the PAPK's `assets/` directory. Asset names are resolved against the PAPK ASSETS section at boot — see [Bundled image assets](/guides/assets/) for the manifest format and pipeline.

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView();
img.setImageSource("icon.png");
```

Scale, tint, and aspect controls (Tier C):

```java
img.setScaleType(ImageView.SCALE_FIT_CENTER);  // or SCALE_FIT_XY, SCALE_CENTER
img.setScale(150);          // 100 = 1.0× — uses LVGL transforms
img.setTint(Color.RED);     // multiplies the source by the given color
img.clearTint();
```

Anti-aliased scale and rotation rendering depends on LVGL 9.5.0's `LV_DRAW_SW_SUPPORT_RGB565A8` (enabled in `lv_conf.h`). Without it scaled images render aliased — see [Advanced configuration → lv_conf.h](/reference/advanced-config/#lv_confh).

### `picodroid.widget.ProgressBar`

A horizontal progress bar.

```java
import picodroid.widget.ProgressBar;

ProgressBar bar = new ProgressBar();
bar.setSize(200, 20);
bar.setProgress(75);   // 0–100
```

For an **indeterminate** spinner (no progress value, just an animation while work is happening), use the static factory:

```java
ProgressBar spinner = ProgressBar.indeterminate();
spinner.setSize(48, 48);
// Add to layout; remove or hide when work completes.
```

`indeterminate()` returns a `ProgressBar` backed by `lv_spinner` and ignores `setProgress`.

### `picodroid.widget.ListView`

A scrollable list. Add plain text items directly, or back it with an `Adapter` for data-driven
lists with stable item IDs and D-pad item selection.

```java
import picodroid.widget.ListView;

ListView list = new ListView();
list.setSize(200, 150);
list.addItem("Item 1");
list.addItem("Item 2");
list.addItem("Item 3");
```

**Adapter-backed (the Tier 2 `Adapter` pattern, since v0.10.0).** Bind an `ArrayAdapter` and
receive typed click callbacks. Each item renders via its `toString()`:

```java
import picodroid.widget.ArrayAdapter;
import picodroid.widget.ListView;

String[] rows = { "Live", "History", "Settings" };
ListView list = new ListView();
list.setAdapter(new ArrayAdapter<String>(rows));

// onItemClick(AdapterView<?> parent, View view, int position, long id) — usable as a lambda:
list.setOnItemClickListener((parent, view, position, id) -> open(position));
```

> **The `Adapter` family.** `ListView` extends `AdapterView<Adapter>`. The pieces:
> - `Adapter` — interface: `getCount()`, `getItem(int)`, `getItemId(int)`.
> - `BaseAdapter` — abstract base with `notifyDataSetChanged()` (call after mutating data).
> - `ArrayAdapter<T>` — concrete `BaseAdapter` over a `T[]`, or built incrementally with
>   `add(T)` / `clear()`; renders each item's `toString()`. Constructors take an optional
>   `Context` and/or `T[]`.
> - `AdapterView<T>.setOnItemClickListener(OnItemClickListener)` — the 4-arg `onItemClick` above.
>
> On memory-constrained boards the LVGL renderer makes very long focusable lists expensive — keep
> adapter-backed data lists modest in length.

### `picodroid.widget.CheckBox`

A labelled checkable box.

```java
import picodroid.widget.CheckBox;
import picodroid.widget.CompoundButton;

CheckBox cb = new CheckBox();
cb.setText("Enable WiFi");
cb.setChecked(true);
boolean on = cb.isChecked();

cb.setOnCheckedChangeListener(new CompoundButton.OnCheckedChangeListener() {
    public void onCheckedChanged(CompoundButton buttonView, boolean isChecked) {
        Log.i("UI", "checked=" + isChecked);
    }
});
```

### `picodroid.widget.SeekBar`

A horizontal slider (0–`max`).

```java
import picodroid.widget.SeekBar;

SeekBar bar = new SeekBar(100);   // or `new SeekBar()` for default max
bar.setProgress(25);
int p = bar.getProgress();

bar.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
    public void onProgressChanged(SeekBar seekBar, int progress, boolean fromUser) {
        Log.i("UI", "progress=" + progress);
    }
    // onStartTrackingTouch(SeekBar) / onStopTrackingTouch(SeekBar) have default no-op bodies
});
```

### `picodroid.widget.Spinner`

A drop-down list. Items are passed as a single newline-separated string.

```java
import picodroid.widget.Spinner;

Spinner sp = new Spinner();
sp.setItems("Red\nGreen\nBlue");
int sel = sp.getSelectedItemPosition();

sp.setOnItemSelectedListener(new Spinner.OnItemSelectedListener() {
    public void onItemSelected(Spinner parent, int position) {
        Log.i("UI", "sel=" + position);
    }
});
```

### `picodroid.widget.EditText`

A single-line text input field. Tapping it pops up the system soft keyboard at the bottom of the screen by default.

```java
import picodroid.widget.EditText;

EditText input = new EditText();
input.setHint("device name");
input.setText("pico-01");
String value = input.getText();
input.setShowKeyboardOnTouch(false);   // disable system keyboard for this field
```

#### `OnEditorActionListener`

Fires when the user presses the keyboard's Done / Send key. Lets you commit the value without the user having to tap elsewhere first.

```java
import picodroid.widget.EditText;
import picodroid.widget.OnEditorActionListener;
import picodroid.view.KeyEvent;

input.setOnEditorActionListener(new OnEditorActionListener() {
    public boolean onEditorAction(EditText v, int actionId, KeyEvent event) {
        save(v.getText());   // event is null for the synthesized soft-keyboard OK
        return true;         // true = consume; false = let default handler run too
    }
});
```

#### `EditorInfo` hints

Tell the soft keyboard which character set to start with by passing an `EditorInfo` constant:

```java
import picodroid.view.inputmethod.EditorInfo;

input.setInputType(EditorInfo.TYPE_NUMBER);   // or TYPE_TEXT, TYPE_EMAIL, TYPE_PHONE, TYPE_PASSWORD
```

See [`picodroid.widget.Keyboard`](#picodroidwidgetkeyboard) for the soft keyboard widget.

### `picodroid.widget.ScrollView`

A vertically scrollable container with a single child (typically a `LinearLayout`).

```java
import picodroid.widget.ScrollView;
import picodroid.widget.LinearLayout;

ScrollView scroll = new ScrollView();
scroll.setSize(320, 240);
LinearLayout content = new LinearLayout();
content.setOrientation(LinearLayout.VERTICAL);
// ... addView(...) lots of children ...
scroll.addView(content);
```

### `picodroid.widget.FrameLayout`

A simple container that stacks children (last `addView` is on top). Useful for overlays such as a status badge over an `ImageView`.

```java
import picodroid.widget.FrameLayout;

FrameLayout overlay = new FrameLayout();
overlay.addView(background);
overlay.addView(badge);
```

### `picodroid.widget.Toast`

Brief, non-modal, auto-dismissing message bubble — Android-style.

```java
import picodroid.widget.Toast;

// First arg is a Context — `this` inside an Activity.
Toast.makeText(this, "Saved.", Toast.LENGTH_SHORT).show();
```

| Constant | Value | Default duration |
|----------|-------|-----------------|
| `Toast.LENGTH_SHORT` | 0 | ~2 s |
| `Toast.LENGTH_LONG` | 1 | ~3.5 s |

| Method | Description |
|--------|-------------|
| `Toast.makeText(Context ctx, String text, int duration)` | Static factory. |
| `show()` | Display the toast. |
| `cancel()` | Dismiss before the timeout expires. |

### `picodroid.widget.AlertDialog`

Modal dialog with a title, message, and up to two buttons. Built via the nested `Builder`.

```java
import picodroid.content.DialogInterface;
import picodroid.widget.AlertDialog;

new AlertDialog.Builder()
    .setTitle("Erase data?")
    .setMessage("This cannot be undone.")
    .setPositiveButton("Erase", new DialogInterface.OnClickListener() {
        public void onClick(DialogInterface dialog, int which) { eraseAll(); }
    })
    .setNegativeButton("Cancel", null)
    .show();
```

| `Builder` method | Description |
|-----|-------------|
| `setTitle(String)` | Dialog title (top bar). |
| `setMessage(String)` | Body text. |
| `setPositiveButton(String text, DialogInterface.OnClickListener listener)` | Confirm button. `onClick(DialogInterface, int which)`; `listener` may be null. `which` is `DialogInterface.BUTTON_POSITIVE`. |
| `setNegativeButton(String text, DialogInterface.OnClickListener listener)` | Dismiss button. `listener` may be null. `which` is `DialogInterface.BUTTON_NEGATIVE`. |
| `create()` | Returns an `AlertDialog` without showing it. |
| `show()` | Convenience: `create()` + `show()`. |

Either button click runs its listener (if any) and then dismisses the dialog. Call `dialog.dismiss()` to close programmatically. See [`examples/dialogdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/dialogdemo).

### `picodroid.widget.Keyboard`

On-screen soft keyboard, wrapping LVGL's `lv_keyboard`. Two ways to use:

**System keyboard (default).** Any `EditText` pops up a singleton system keyboard at the screen bottom on touch — no setup needed. Dismissed by BACK or the keyboard's OK key.

**Explicit instance** for custom placement or styling:

```java
import picodroid.widget.EditText;
import picodroid.widget.Keyboard;

EditText input = new EditText();
input.setShowKeyboardOnTouch(false);   // disable system keyboard for this field

Keyboard kb = new Keyboard();
kb.setEditText(input);
kb.setMode(Keyboard.MODE_TEXT_LOWER);
kb.setPosition(0, 120);
kb.setSize(320, 120);
kb.setOnReadyListener(new Keyboard.OnReadyListener() {
    public void onReady(Keyboard keyboard) { keyboard.hide(); /* validate input here */ }
});
kb.show();
```

| Constant | Value |
|----------|-------|
| `Keyboard.MODE_TEXT_LOWER` | 0 |
| `Keyboard.MODE_TEXT_UPPER` | 1 |
| `Keyboard.MODE_SPECIAL` | 2 |
| `Keyboard.MODE_NUMBER` | 3 |

LVGL switches modes internally as the user taps the keyboard's "abc"/"ABC"/"123"/"!@#" toggle keys. v1 caveats: US-English layout only; explicit instances do not auto-hide on the OK key (the listener decides).

**Polish pass (since v0.5.0).** The system keyboard:

- Slides up from the bottom edge over ~150 ms when an `EditText` gains focus, and slides back down on dismiss — no instant jump.
- Forwards the OK key to any `OnEditorActionListener` registered on the focused `EditText` before the default close behavior runs.
- Dismisses on tap-outside: tapping anywhere outside the keyboard rectangle (and outside the focused `EditText`) hides it.

See [`examples/keyboarddemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keyboarddemo).

### `picodroid.widget.Snackbar`

Toast-with-an-action. Brief, auto-dismissing message bubble that optionally carries a clickable lozenge ("Undo", "Retry", etc.).

```java
import picodroid.view.View;
import picodroid.widget.Snackbar;

// make(View parent, String text, int duration) — duration is passed here, not via setDuration
Snackbar.make(rootView, "Item deleted", Snackbar.LENGTH_LONG)
    .setAction("Undo", new View.OnClickListener() {
        public void onClick(View v) { restoreItem(); }
    })
    .show();
```

| Constant | Value | Default duration |
|----------|-------|-----------------|
| `Snackbar.LENGTH_SHORT` | 0 | ~2 s |
| `Snackbar.LENGTH_LONG` | 1 | ~3.5 s |
| `Snackbar.LENGTH_INDEFINITE` | -1 | until manually dismissed |

If the user taps the action lozenge, the listener runs and the Snackbar dismisses immediately. Otherwise the Snackbar fades out after `duration`.

See [`examples/snackbardemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/snackbardemo).

### `picodroid.widget.DatePicker`

Calendar widget for picking a calendar date, backed by `lv_calendar`.

```java
import picodroid.widget.DatePicker;

DatePicker dp = new DatePicker();
dp.setSize(280, 220);
dp.setDate(2026, 5, 7);   // year, month (1–12), day-of-month

dp.setOnDateChangedListener(new DatePicker.OnDateChangedListener() {
    public void onDateChanged(DatePicker view, int year, int monthOfYear, int dayOfMonth) {
        Log.i("UI", "picked " + year + "-" + monthOfYear + "-" + dayOfMonth);
    }
});
```

`OnDateChangedListener` fires only on user interaction; `setDate` programmatically does not re-trigger the listener.

### `picodroid.widget.TimePicker`

Roller widget for picking a wall-clock time, backed by `lv_roller`. Defaults to 24-hour mode.

```java
import picodroid.widget.TimePicker;

TimePicker tp = new TimePicker();
tp.setSize(220, 180);
tp.setTime(14, 30);   // 2:30 pm in 24-hour mode

tp.setOnTimeChangedListener(new TimePicker.OnTimeChangedListener() {
    public void onTimeChanged(TimePicker view, int hourOfDay, int minute) {
        Log.i("UI", "picked " + hourOfDay + ":" + minute);
    }
});
```

12-hour / AM-PM mode (since v0.7.0):

```java
tp.setIs24HourView(false);   // adds an AM/PM column
tp.setTime(2, 30);           // hours run 1..12; AM/PM tracked separately
boolean isAM = tp.isAm();
```

When 12-hour mode is on, `getHour()` returns the displayed hour in `1..12`. Use `isAm()` to disambiguate.

See [`examples/pickerdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/pickerdemo) for both pickers in one screen.

## Swipe gestures

### `picodroid.view.OnSwipeListener`

Per-View swipe primitive — sits beside `OnTouchListener` and fires on a recognized swipe direction.

```java
import picodroid.view.OnSwipeListener;
import picodroid.view.View;

view.setOnSwipeListener(new OnSwipeListener() {
    public void onSwipe(View v, int direction) {
        switch (direction) {
            case View.SWIPE_UP:    /* ... */ break;
            case View.SWIPE_DOWN:  /* ... */ break;
            case View.SWIPE_LEFT:  /* ... */ break;
            case View.SWIPE_RIGHT: /* ... */ break;
        }
    }
});
```

The `SWIPE_*` direction constants live on `View` (`View.SWIPE_LEFT` = 1, `SWIPE_RIGHT` = 2,
`SWIPE_UP` = 4, `SWIPE_DOWN` = 8). Direction is decided from the largest dominant axis with a
configurable minimum delta. Diagonal-only swipes do not fire.

### `picodroid.widget.SwipeRefreshLayout`

Container that triggers a refresh action when the user pulls down from the top of its child.

```java
import picodroid.widget.SwipeRefreshLayout;

SwipeRefreshLayout pull = new SwipeRefreshLayout();
pull.addView(scrollableContent);
pull.setOnRefreshListener(new SwipeRefreshLayout.OnRefreshListener() {
    public void onRefresh() {
        reload();
        pull.setRefreshing(false);   // dismiss the spinner when done
    }
});
```

`setRefreshing(true)` shows the spinner programmatically without firing the listener. See [`examples/swipedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/swipedemo).

## Complete display app example

A minimal app that creates a button and updates a label when tapped:

```java
// CounterApp.java
package counter;

import picodroid.app.Application;
import picodroid.content.Intent;

public class CounterApp extends Application {
    public void onCreate() {
        startActivity(new Intent(CounterActivity.class));
    }
}
```

```java
// CounterActivity.java
package counter;

import picodroid.app.Activity;
import picodroid.debug.DisplayDebug;
import picodroid.graphics.Color;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class CounterActivity extends Activity {
    private int count = 0;

    public void onCreate() {
        DisplayDebug.calibrate();

        LinearLayout root = new LinearLayout();
        root.setOrientation(LinearLayout.VERTICAL);
        root.setSize(320, 240);

        TextView label = new TextView();
        label.setText("Count: 0");
        label.setTextColor(Color.WHITE);
        root.addView(label);

        Button btn = new Button("Increment");
        btn.setSize(200, 50);
        btn.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                count = count + 1;
                label.setText("Count: " + count);
            }
        });
        root.addView(btn);

        setContentView(root);
    }
}
```

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](/api/storage/) (files, preferences) · [networking.md](/api/networking/) (sockets)
