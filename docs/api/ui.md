# Graphics and UI

Picodroid's Android-inspired UI toolkit. Packages: `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget`. See [docs/README.md](../README.md) for the full API index.

The toolkit is backed by [LVGL](https://lvgl.io). Apps create an `Application`, start an `Activity`, and build a widget tree. On hardware, the display is driven via SPI (ST7789) with touch input (XPT2046). In the simulator, a graphical window (minifb) renders the UI with mouse-as-touch input.

## `picodroid.app.Application`

Base class for all apps. Subclass it and override `onCreate()`.

```java
import picodroid.app.Application;

public class MyApp extends Application {
    public void onCreate() {
        // Console app: do work here
        // Display app: start an Activity
        startActivity(new MyActivity());
    }
}
```

| Method | Description |
|--------|-------------|
| `onCreate()` | Called by the runtime after instantiation. Override to initialize your app. |
| `startActivity(Activity activity)` | Launches an Activity (native). The Activity's `onCreate()` is called after the display is ready. |

## `picodroid.app.Activity`

Base class for display screens. Subclass it, override `onCreate()`, build a widget tree, and call `setContentView()`.

```java
import picodroid.app.Activity;
import picodroid.view.View;
import picodroid.graphics.Display;

public class MyActivity extends Activity {
    public void onCreate() {
        getDisplay().calibrate();     // optional: run touch calibration
        // ... build widget tree ...
        setContentView(rootView);     // render the widget tree
    }
}
```

| Method | Description |
|--------|-------------|
| `onCreate()` | Called after the display is initialized. Override to build your UI. |
| `setContentView(View root)` | Sets the root of the widget tree and renders it to the display. |
| `getDisplay()` | Returns the `Display` singleton. |

## `picodroid.graphics.Display`

Singleton representing the physical display. Typically accessed via `Activity.getDisplay()`.

```java
import picodroid.graphics.Display;

Display display = Display.getInstance();
int w = display.getWidth();      // e.g. 320
int h = display.getHeight();     // e.g. 240

display.calibrate();             // run 4-point touch calibration
display.setContentView(root);    // set root widget
display.update();                // refresh the display
display.showFps();               // overlay a moving-average FPS counter (10-frame window)
MotionEvent touch = display.pollTouch();  // poll for touch input (null if none)
```

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

## `picodroid.view.View`

Base class for all UI widgets. Not instantiated directly — use subclasses like `TextView`, `Button`, etc.

```java
import picodroid.view.View;

view.setPosition(10, 20);           // x=10, y=20
view.setSize(200, 50);              // width=200, height=50
view.setBackgroundColor(Color.BLUE);
view.setVisibility(View.VISIBLE);   // VISIBLE, INVISIBLE, or GONE
view.close();                        // release the native widget
```

| Constant | Value | Description |
|----------|-------|-------------|
| `View.VISIBLE` | 0 | Widget is visible and takes up layout space |
| `View.INVISIBLE` | 1 | Widget is invisible but still takes up layout space |
| `View.GONE` | 2 | Widget is invisible and takes no layout space |

## `picodroid.view.MotionEvent`

Represents a touch event from the display. Returned by `Display.pollTouch()`.

```java
import picodroid.view.MotionEvent;

MotionEvent event = display.pollTouch();
if (event != null) {
    int action = event.getAction();   // ACTION_DOWN, ACTION_UP, or ACTION_MOVE
    int x = event.getX();
    int y = event.getY();
}
```

| Constant | Value |
|----------|-------|
| `MotionEvent.ACTION_DOWN` | 0 |
| `MotionEvent.ACTION_UP` | 1 |
| `MotionEvent.ACTION_MOVE` | 2 |

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
import picodroid.widget.Button;

Button btn = new Button("Tap Me!");
btn.setSize(200, 50);
btn.setText("New Label");

// Event-driven click handling
btn.setOnClickListener(new Runnable() {
    public void run() {
        Log.i("UI", "Button clicked!");
    }
});

// Or poll-based
boolean clicked = btn.wasClicked();
```

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

### `picodroid.widget.Switch`

An on/off toggle switch.

```java
import picodroid.widget.Switch;

Switch sw = new Switch();
sw.setSize(60, 30);

boolean on = sw.isChecked();
sw.setChecked(true);
sw.toggle();

sw.setOnCheckedChangeListener(new Runnable() {
    public void run() {
        Log.i("UI", "Switch is now " + sw.isChecked());
    }
});
```

### `picodroid.widget.ToggleButton`

A button that toggles between two states with configurable text labels.

```java
import picodroid.widget.ToggleButton;

ToggleButton toggle = new ToggleButton("ON", "OFF");  // or new ToggleButton()
toggle.setSize(200, 50);

boolean on = toggle.isChecked();
toggle.setChecked(true);
toggle.toggle();
toggle.setTextOn("Enabled");
toggle.setTextOff("Disabled");

toggle.setOnCheckedChangeListener(new Runnable() {
    public void run() {
        Log.i("UI", "Toggle is now " + toggle.isChecked());
    }
});
```

### `picodroid.widget.ImageView`

Displays an image from a file path.

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView();
img.setImageSource("icon.png");
```

### `picodroid.widget.ProgressBar`

A horizontal progress bar.

```java
import picodroid.widget.ProgressBar;

ProgressBar bar = new ProgressBar();
bar.setSize(200, 20);
bar.setProgress(75);   // 0–100
```

### `picodroid.widget.ListView`

A scrollable list of text items.

```java
import picodroid.widget.ListView;

ListView list = new ListView();
list.setSize(200, 150);
list.addItem("Item 1");
list.addItem("Item 2");
list.addItem("Item 3");
```

### `picodroid.widget.CheckBox`

A labelled checkable box.

```java
import picodroid.widget.CheckBox;

CheckBox cb = new CheckBox();
cb.setText("Enable WiFi");
cb.setChecked(true);
boolean on = cb.isChecked();

cb.setOnCheckedChangeListener(new Runnable() {
    public void run() { Log.i("UI", "checked=" + cb.isChecked()); }
});
```

### `picodroid.widget.SeekBar`

A horizontal slider (0–`max`).

```java
import picodroid.widget.SeekBar;

SeekBar bar = new SeekBar(100);   // or `new SeekBar()` for default max
bar.setProgress(25);
int p = bar.getProgress();

bar.setOnSeekBarChangeListener(new Runnable() {
    public void run() { Log.i("UI", "progress=" + bar.getProgress()); }
});
```

### `picodroid.widget.Spinner`

A drop-down list. Items are passed as a single newline-separated string.

```java
import picodroid.widget.Spinner;

Spinner sp = new Spinner();
sp.setItems("Red\nGreen\nBlue");
int sel = sp.getSelectedItemPosition();

sp.setOnItemSelectedListener(new Runnable() {
    public void run() { Log.i("UI", "sel=" + sp.getSelectedItemPosition()); }
});
```

### `picodroid.widget.EditText`

A single-line text input field.

```java
import picodroid.widget.EditText;

EditText input = new EditText();
input.setHint("device name");
input.setText("pico-01");
String value = input.getText();
```

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

## Complete display app example

A minimal app that creates a button and updates a label when tapped:

```java
// CounterApp.java
package counter;

import picodroid.app.Application;

public class CounterApp extends Application {
    public void onCreate() {
        startActivity(new CounterActivity());
    }
}
```

```java
// CounterActivity.java
package counter;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class CounterActivity extends Activity {
    private int count = 0;

    public void onCreate() {
        getDisplay().calibrate();

        LinearLayout root = new LinearLayout();
        root.setOrientation(LinearLayout.VERTICAL);
        root.setSize(320, 240);

        TextView label = new TextView();
        label.setText("Count: 0");
        label.setTextColor(Color.WHITE);
        root.addView(label);

        Button btn = new Button("Increment");
        btn.setSize(200, 50);
        btn.setOnClickListener(new Runnable() {
            public void run() {
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

**See also:** [core.md](core.md) (Java language) · [system.md](system.md) (logging, clock, threads) · [peripherals.md](peripherals.md) (GPIO, UART, I2C, SPI, PWM, ADC) · [storage.md](storage.md) (files, preferences) · [networking.md](networking.md) (sockets)
