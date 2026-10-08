---
title: "Writing a Java App"
description: "Scaffold a Picodroid app, wire its lifecycle, and ship a PAPK."
---

## Quickest path — scaffold with Gradle

```bash
./gradlew newApp -Pname=myapp
```

This creates `examples/myapp/` with a starter `java/myapp/Myapp.java` (the class is the app name with its first letter capitalised), `PicodroidManifest.xml`, and `build.gradle.kts`. Add `-Plang=kotlin` for a [Kotlin](/guides/kotlin/) starter. The name must be lower case: letters, digits and `_`.

Run it in the [simulator](/get-started/simulator/), no hardware needed:

```bash
./scripts/sim.sh --app myapp
```

Or build and flash:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp
```

## Manual layout

1. Create a Java source under `examples/myapp/`:

```java
// examples/myapp/java/myapp/MyApp.java
package myapp;

import picodroid.app.Application;
import picodroid.util.Log;

public class MyApp extends Application {
    public void onCreate() {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

2. Create a `PicodroidManifest.xml` in the app directory:

```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest package="myapp" version="1.0">
    <application application="myapp/MyApp" />
</manifest>
```

`package` is the app's identity on a device: a board that holds several apps keeps one install per package, and `pdb uninstall` names it. `<application>` also takes a `label` and an `icon` for the launcher to show — see the [manifest reference](/reference/manifest/#app-identity).

3. Create a one-line `build.gradle.kts` in the app directory:

```kotlin
plugins {
    id("picodroid-papk")
}
```

4. Build and flash — no changes to `settings.gradle.kts`, `Cargo.toml`, or `platforms/rp/src/app.rs` needed:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp

# Or for Pico (RP2040)
./scripts/build.sh --app myapp --board testbench_rp2040
./scripts/flash.sh --app myapp --board testbench_rp2040
```

The build pipeline is a Gradle multi-project with a custom `picodroid-papk` plugin (see [buildSrc/](https://github.com/shivrajora/picodroid-rs/tree/main/buildSrc)). `settings.gradle.kts` auto-discovers every `examples/<name>/` that has a `PicodroidManifest.xml`. The plugin compiles Java against the framework in `:sdk`, optionally rewrites framework references via the active shrink map, then packages the result into `examples/myapp/build/papk/myapp.papk`. `scripts/build-apk.sh` is a thin wrapper that invokes Gradle and copies the artifact to `build/apks/myapp.papk` where the firmware build embeds it.

## IDE support

The build files in `settings.gradle.kts` and `examples/*/build.gradle.kts` are standard Gradle projects. Opening the repo in IntelliJ IDEA ("Open as Gradle project") or VS Code (with the Red Hat Java + Gradle extensions) gives autocomplete, jump-to-def, and inline error reporting for all framework and app sources.

Pass `--shrink` (off by default) to apply the active release shrink map — `build-apk.sh` will rewrite framework class and member references inside your `.class` files (e.g. `Lpicodroid/app/Application;` → `La/B;`). Your own names stay as they are, so the `application=` value in the manifest remains valid; the one exception is a name of yours that spells one of the map's short names (a field `p`, a class in the default package), which is renamed so it cannot alias the framework's. Add `--shrink-app` to rename your own classes and private members as well; the manifest still names `myapp/MyApp` and `papk-pack` maps the entry class for you. See [Shrinker](/reference/shrinker/) for details.

## Application Lifecycle

The apps on this page extend `picodroid.app.Application` and override `onCreate()`. The runtime instantiates the class the manifest's `application=` names and calls `onCreate()` as the entry point. (The manifest can name an Activity or a `main` class instead — see [entry-point styles](/reference/manifest/#entry-point-styles).)

### Console app

For apps that only use logging and peripherals:

```java
package myapp;

import picodroid.app.Application;
import picodroid.util.Log;

public class MyApp extends Application {
    public void onCreate() {
        Log.i("MyApp", "Hello from MyApp!");
    }
}
```

### Display app

For graphical apps, create an `Activity` subclass and launch it with `startActivity()`:

```java
// MyApp.java — Application entry point
package myapp;

import picodroid.app.Application;
import picodroid.content.Intent;

public class MyApp extends Application {
    public void onCreate() {
        startActivity(new Intent(MyActivity.class));
    }
}
```

```java
// MyActivity.java — builds the UI
package myapp;

import picodroid.app.Activity;
import picodroid.debug.DisplayDebug;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class MyActivity extends Activity {
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        DisplayDebug.calibrate();

        LinearLayout root = new LinearLayout();
        root.setOrientation(LinearLayout.VERTICAL);
        root.setSize(320, 240);

        TextView label = new TextView();
        label.setText("Hello, Display!");
        label.setTextColor(Color.WHITE);
        root.addView(label);

        setContentView(root);
    }
}
```

The Activity's `onCreate()` is called after the display is initialized. Build a widget tree, then call `setContentView()` to render it. See [Graphics & UI](/api/ui/) for the full graphics and widget API.

### Activity lifecycle and back stack

Beyond `onCreate()`, `Activity` exposes the full Android lifecycle: `onStart` / `onResume` / `onPause` / `onStop` / `onRestart` / `onDestroy` / `onBackPressed`. The runtime also maintains a back stack — push a new screen with `startActivity(new Intent(DetailActivity.class))` and pop it with `finish()`:

```java
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.view.View;
import picodroid.widget.Button;

public class HomeActivity extends Activity {
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Button btn = new Button("Open detail");
        btn.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) { startActivity(new Intent(DetailActivity.class)); }
        });
        setContentView(btn);
    }
}
```

The widget tree set via `setContentView()` is **preserved across pause** — when control returns from a popped Activity, the saved tree is restored automatically. See [api/ui.md → Lifecycle](/api/ui/#lifecycle), [api/ui.md → Back stack](/api/ui/#back-stack), and [`examples/navdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/navdemo).

BACK finishes the top Activity. From the app's only Activity that ends the app, and on a board with a launcher the launcher comes back — so leave `onBackPressed` alone on the root screen. A HOME key (`pico_touch_kit` has one) goes to the launcher from anywhere and never reaches the app. See the [launcher guide](/guides/launcher/#coming-back).

### Saved instance state

The argument of `onCreate(Bundle)` is `null` on a fresh launch. It is the Bundle your `onSaveInstanceState` filled when the framework is re-creating the Activity: after `recreate()`, or when a covered Activity was destroyed to get its memory back and the user returns to it. Fields you did not save are gone in the new instance:

```java
@Override
protected void onSaveInstanceState(Bundle outState) {
    outState.putInt("count", count);
}

@Override
protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    count = savedInstanceState == null ? 0 : savedInstanceState.getInt("count");
}
```

Unlike Android, the default `onSaveInstanceState` saves nothing, an `EditText`'s text included. See [api/ui.md → Saved instance state](/api/ui/#saved-instance-state) and [`examples/bundledemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/bundledemo).

### Resources and XML layouts

The display app above builds its widgets in Java. An app can also carry an Android-style `res/` directory — strings, colours, dimensions, layouts, PNG drawables — and use the generated `R` class:

```java
setContentView(R.layout.activity_main);
TextView title = findViewById(R.id.title);
title.setText(getString(R.string.greeting));
```

Layouts are compiled at build time; no XML parser runs on the device. See [Resources, R and XML layouts](/guides/resources/) and [`examples/resdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/resdemo).

### Fragments

`picodroid.app.Fragment`, with `FragmentManager` and `FragmentTransaction`, has the shape of `androidx.fragment.app`: `getSupportFragmentManager().beginTransaction().replace(R.id.container, new DetailFragment()).addToBackStack(null).commit()`, and BACK pops the fragment back stack before it finishes the Activity. `ViewPager2` pages between fragments. They are on every RP2350 board; `testbench_rp2040` leaves the classes out. See [api/ui.md → Fragment](/api/ui/#picodroidappfragment) and [`examples/fragmentdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/fragmentdemo).

### Toasts and dialogs

```java
import picodroid.content.DialogInterface;
import picodroid.app.AlertDialog;
import picodroid.widget.Toast;

Toast.makeText(this, "Saved.", Toast.LENGTH_SHORT).show();   // first arg is a Context

new AlertDialog.Builder()
    .setTitle("Erase data?")
    .setMessage("This cannot be undone.")
    .setPositiveButton("Erase", new DialogInterface.OnClickListener() {
        public void onClick(DialogInterface dialog, int which) { eraseAll(); }
    })
    .setNegativeButton("Cancel", null)
    .show();
```

See [api/ui.md → Toast](/api/ui/#picodroidwidgettoast), [→ AlertDialog](/api/ui/#picodroidappalertdialog), and [`examples/dialogdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/dialogdemo).

### Theme and drawables

Override the global palette before any UI is built (typically in `Application.onCreate`). Apply `GradientDrawable` for rounded fills, gradients, and strokes:

```java
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;

Theme.colorPrimary    = Color.argb(255, 80, 180, 120);
Theme.colorBackground = Color.argb(255, 24,  24,  28);

view.setBackground(new GradientDrawable()
    .setColor(Theme.colorSurface)
    .setCornerRadius(16)
    .setStroke(1, Theme.colorOutline));
```

See [api/ui.md → Theme](/api/ui/#picodroidgraphicstheme), [→ GradientDrawable](/api/ui/#picodroidgraphicsdrawablegradientdrawable), and the themed-widgets section of [`examples/displaydemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/displaydemo).

### Gestures and animations

Wire a `GestureDetector` to recognize taps, long presses, and flings, and use `view.animate()` for short property animations:

```java
import picodroid.view.GestureDetector;
import picodroid.view.MotionEvent;

view.setOnTouchListener(new GestureDetector(new GestureDetector.OnGestureListener() {
    public void onSingleTap(MotionEvent e) { view.animate().alpha(0.3f).setDuration(120).start(); }
    public void onLongPress(MotionEvent e) { showContextMenu(); }
    public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) { /* ... */ }
}));
```

See [api/ui.md → GestureDetector](/api/ui/#picodroidviewontouchlistener-and-gesturedetector), [→ ViewPropertyAnimator](/api/ui/#picodroidviewviewpropertyanimator), and [`examples/gesturedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/gesturedemo) / [`examples/animdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/animdemo).

### Soft keyboard

Tapping any `EditText` pops up a system soft keyboard at the screen bottom by default — no setup needed. For custom placement or styling, instantiate `Keyboard` explicitly and pair with `EditText.setShowKeyboardOnTouch(false)`. See [api/ui.md → Keyboard](/api/ui/#picodroidwidgetkeyboard) and [`examples/keyboarddemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/keyboarddemo).

### Input and idle power

On boards with hardware buttons, install `OnKeyListener`s to receive `KeyEvent`s, or override `Activity.onKeyDown` / `onKeyUp` / `onKeyLongPress` for the keys no focused view consumed — see [api/ui.md → Key events](/api/ui/#key-events). On every board, after the screen timeout (`idle_timeout_ms` in `board.toml`, 60 seconds by default; Settings → Display can change it) with no key or touch, the runtime dozes the display (backlight off, `DISPOFF`, `SLPIN`) while your app keeps running. The next button press or touch wakes the panel and is swallowed by the framework — it is not delivered to your listener or as a click. A screen that is watched rather than touched (a clock, a monitor) holds the panel on with `View.setKeepScreenOn(true)` on its root, or `android:keepScreenOn="true"` in its layout. See [Display idle sleep](/reference/limits/#display-idle-sleep).

### Posting work between threads

To run short-lived async work without spawning a dedicated `Thread`, use the executors in [api/system.md → Executors](/api/system/#picodroidconcurrentexecutors):

```java
import picodroid.concurrent.Executors;

Executors.backgroundExecutor().execute(() -> {
    int value = readSensorBlocking();
    Executors.mainExecutor().execute(() -> label.setText("value=" + value));
});
```

`backgroundExecutor()` runs on a shared worker pool (configurable in [`board.toml`](/reference/porting-guide/#boardtoml-reference)); `mainExecutor()` hops back to the UI thread. Both are non-blocking and drop on queue saturation.

For delayed or periodic work on the main thread — what `Handler.postDelayed` does on Android — use `Executors.mainScheduledExecutor()`; see [api/system.md → ScheduledExecutorService](/api/system/#delayed-and-periodic-work-scheduledexecutorservice). It is not on `testbench_rp2040`.

## Porting to a New Platform

The `pico-jvm` crate is hardware-independent (`no_std + alloc` only). To use it on a different platform, implement the HAL modules and `NativeMethodHandler` trait. See [Porting guide](/reference/porting-guide/) for the full guide, including HAL function signatures, FreeRTOS configuration, and build system setup.

## Supported Language Features

The table below is written for Java. Kotlin apps are supported too — same JVM,
same features where the languages overlap; see the [Kotlin guide](/guides/kotlin/)
for the Kotlin-specific subset, divergences, and idioms.

| Feature | Example |
|---------|---------|
| Arrays | `byte[] buf = new byte[16];` — allocation, `.length`, index read/write |
| Inheritance | `class Dog extends Animal` — field inheritance, `@Override`, `super()` constructor chaining, virtual dispatch |
| Interfaces | `interface Speakable` / `implements` — `invokeinterface` polymorphic dispatch |
| Floating-point | `float`/`double` arithmetic, `f2i`/`f2d` casts |
| Long integers | `long` arithmetic, `i2l`/`l2i` type conversions |
| Double precision | `double` arithmetic, `i2d`/`d2i` type conversions |
| Exceptions | `throw new AppException()`, `try`/`catch`, custom exception classes |
| Try-with-resources | `try (Gpio gpio = pm.openGpio("GP25")) { ... }` — `AutoCloseable` peripherals; `close()` called on normal exit and on exception |
| Switch statements | `switch`/`case` on integer and other supported types |
| Static fields | `static` field declarations and access via `getstatic`/`putstatic` |
| Null checks | Null reference detection (`ifnull`/`ifnonnull`) |
| Threading | `new Thread(runnable).start()` — spawns a FreeRTOS task per thread, pinned to core 0; stack reclaimed when `run()` returns; `setPriority(1–10)` is recorded but advisory — every Java thread runs at one JVM tier (see [Thread](/api/system/#picodroidconcurrentthread)) |
| Lambdas | `() -> expr`, `(x) -> expr` — non-capturing and capturing lambdas, method references (`Class::method`, `String::length`, `Integer::valueOf`, `Foo::new`), callbacks; compiled via `invokedynamic` |
| Anonymous classes | `new Interface() { ... }` — anonymous inner classes implementing interfaces, with local variable capture |
| Static initializers | `static { ... }` blocks, `static` field initializers, cross-class `<clinit>` chaining; each class initializer runs exactly once on first use |
| Synchronized blocks | `synchronized (lock) { ... }` — `monitorenter`/`monitorexit` bytecodes, reentrant locking on the same object |
| Arithmetic ops | subtraction, division, remainder, negation for `int`, `long`, `double` |
| Bitwise / shifts | `<<`, `>>`, `>>>`, `\|`, `^` for `int` and `long` |
| Cross-type casts | `i2f`, `i2c`, `i2s`, `l2f`, `l2d`, `f2l`, `f2d`, `d2l`, `d2f` |
| Dense switch | consecutive-case `switch` compiled to `tableswitch` |
| Type checking | `instanceof`, `checkcast` |
| Reference arrays | `new SomeClass[n]`, element store and load (`aastore`, `aaload`) |
| String predicates | `equals`, `equalsIgnoreCase`, `startsWith`, `endsWith`, `contains`, `isEmpty`, `compareTo` |
| String search | `indexOf(char/String)`, `lastIndexOf(char/String)` |
| String transforms | `substring(int)`, `substring(int,int)`, `trim()`, `toUpperCase()`, `toLowerCase()` |
| String factory | `String.valueOf(int/long/boolean/char/float/double)` |
| StringBuilder | `new StringBuilder("seed")`, `append(String/int/long/float/boolean/char)`, `length()`, `charAt(int)`, `toString()` |
| ArrayList | `new ArrayList()`, `add`, `get`, `size`, `isEmpty`, `set`, `remove(int)`, `clear`, `contains` — dynamic list backed by heap |
| HashMap / HashSet | `java.util.HashMap` and `HashSet` — `put`, `get`, `containsKey`, `remove`, `size`, key iteration; works with autoboxed keys |
| Iterator / for-each | `Iterable` / `Iterator` and the enhanced `for (T x : collection)` loop — backed by `ArrayList`, `HashMap`, `HashSet` |
| Enums | Java `enum` declarations — `values()`, `valueOf(String)`, `name()`, `ordinal()`, and `switch` over enum constants |
| Autoboxing | `Integer`, `Boolean`, `Long`, `Float`, `Double` — `valueOf` / `intValue` etc.; enables storing primitives in `ArrayList<Integer>` etc. |
| String (extended) | `split`, `replace`, `concat`, `toCharArray`, `hashCode` in addition to the predicates / search / transform methods listed above |
| Sorting / list utilities | `Arrays.sort` / `Arrays.toString` (stable mergesort over `Comparable[]`), `Collections.sort` / `Collections.reverse` over `java.util.List` (`ArrayList` implements it), `java.lang.Comparable<T>` |
| `System.currentTimeMillis()` | Milliseconds counted from boot until the app sets the wall clock (`SystemClock.setCurrentTimeMillis`, e.g. from `SntpClient`) — the common `long now = System.currentTimeMillis()` Android idiom |

## Garbage Collection

The JVM runs a **stop-the-world mark-sweep collector** automatically. After every 256 heap allocations (the default; `gc_alloc_threshold` in a board's `board.toml` changes it) it scans all roots (frame locals, operand stacks, static fields), marks every reachable object, array, and string, then frees everything unreachable. There is no action needed from app code — GC fires transparently between bytecode instructions.

To introspect GC behavior from Java code, use `picodroid.os.Runtime`:

```java
import picodroid.os.Runtime;

int  count = Runtime.gcCount();      // cycles run since boot (or last reset)
int  freed = Runtime.gcFreed();      // entries freed across all cycles
long ns    = Runtime.gcTimeNanos();  // cumulative time spent in GC
Runtime.resetGcStats();              // zero all counters
```

## Where next

Once your first app runs, work through these in order:

1. [Tutorial: a multi-screen app with a back stack](/tutorials/multi-screen-app/) — Activities, navigation, and lifecycle in a real app.
2. [Tutorial: a background service bound from an Activity](/tutorials/background-service/) — long-lived work that survives screen changes.
3. [Embedded gotchas](/guides/embedded-gotchas/) — the Android idioms that behave differently here. Read this before you write much UI.
4. [System limits & memory budgets](/reference/limits/) — how much your app can do before it runs out of room.
5. [Debugging](/guides/debugging/) — symptom-driven playbooks for when something goes wrong.

For touchless hardware-button boards, also read [Button-only navigation](/guides/button-navigation/). The full manifest schema lives in the [Manifest reference](/reference/manifest/).
