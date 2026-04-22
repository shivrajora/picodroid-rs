# Writing a Java App

## Quickest path — scaffold with Gradle

```bash
./gradlew newApp -Pname=myapp
```

This creates `examples/myapp/` with a starter `MyApp.java`, `PicodroidManifest.xml`, and `build.gradle.kts`. Then build and flash:

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

3. Create a one-line `build.gradle.kts` in the app directory:

```kotlin
plugins {
    id("picodroid-papk")
}
```

4. Build and flash — no changes to `settings.gradle.kts`, `Cargo.toml`, or `src/app.rs` needed:

```bash
./scripts/build.sh --app myapp
./scripts/flash.sh --app myapp

# Or for Pico (RP2040)
./scripts/build.sh --app myapp --board testbench_rp2040
./scripts/flash.sh --app myapp --board testbench_rp2040
```

The build pipeline is a Gradle multi-project with a custom `picodroid-papk` plugin (see [buildSrc/](../buildSrc/)). `settings.gradle.kts` auto-discovers every `examples/<name>/` that has a `PicodroidManifest.xml`. The plugin compiles Java against the framework in `:sdk`, optionally rewrites framework references via the active shrink map, then packages the result into `examples/myapp/build/papk/myapp.papk`. `scripts/build-apk.sh` is a thin wrapper that invokes Gradle and copies the artifact to `build/apks/myapp.papk` where the firmware build embeds it.

## IDE support

The build files in `settings.gradle.kts` and `examples/*/build.gradle.kts` are standard Gradle projects. Opening the repo in IntelliJ IDEA ("Open as Gradle project") or VS Code (with the Red Hat Java + Gradle extensions) gives autocomplete, jump-to-def, and inline error reporting for all framework and app sources.

Pass `--shrink` (off by default) to apply the active class-name shrink map — `build-apk.sh` will rewrite framework class references inside your `.class` files (e.g. `Lpicodroid/app/Application;` → `La/B;`). Your own class names stay unchanged, so the `application=` value in the manifest remains valid. See [shrinker.md](shrinker.md) for details.

## Application Lifecycle

All apps extend `picodroid.app.Application` and override `onCreate()`. The runtime instantiates your Application class and calls `onCreate()` as the entry point.

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

public class MyApp extends Application {
    public void onCreate() {
        startActivity(new MyActivity());
    }
}
```

```java
// MyActivity.java — builds the UI
package myapp;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class MyActivity extends Activity {
    public void onCreate() {
        getDisplay().calibrate();

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

The Activity's `onCreate()` is called after the display is initialized. Build a widget tree, then call `setContentView()` to render it. See [api/ui.md](api/ui.md) for the full graphics and widget API.

### Input and idle power

On boards with hardware buttons, install `OnKeyListener`s to receive `KeyEvent`s — see [api/ui.md → Key events](api/ui.md#key-events). After **60 seconds** with no button or touch input, the runtime puts the display panel to sleep (backlight off, `DISPOFF`, `SLPIN`) and blocks on a GPIO semaphore. The next button edge wakes the panel and is swallowed by the framework — it is not delivered to your listener.

### Posting work between threads

To run short-lived async work without spawning a dedicated `Thread`, use the executors in [api/system.md → Executors](api/system.md#picodroidconcurrentexecutors):

```java
import picodroid.concurrent.Executors;

Executors.backgroundExecutor().execute(() -> {
    int value = readSensorBlocking();
    Executors.mainExecutor().execute(() -> label.setText("value=" + value));
});
```

`backgroundExecutor()` runs on a shared worker pool (configurable in [`board.toml`](porting-guide.md#boardtoml-reference)); `mainExecutor()` hops back to the UI thread. Both are non-blocking and drop on queue saturation.

## Porting to a New Platform

The `pico-jvm` crate is hardware-independent (`no_std + alloc` only). To use it on a different platform, implement the HAL modules and `NativeMethodHandler` trait. See [porting-guide.md](porting-guide.md) for the full guide, including HAL function signatures, FreeRTOS configuration, and build system setup.

## Supported Language Features

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
| Threading | `new Thread(runnable).start()` — spawns a FreeRTOS task per thread, pinned to core 0; stack reclaimed when `run()` returns; priority set via `setPriority(1–10)` before `start()` |
| Lambdas | `() -> expr`, `(x) -> expr` — non-capturing and capturing lambdas, method references (`Class::method`), callbacks; compiled via `invokedynamic` |
| Anonymous classes | `new Interface() { ... }` — anonymous inner classes implementing interfaces, with local variable capture |
| Static initializers | `static { ... }` blocks, `static` field initializers, cross-class `<clinit>` chaining; each class initializer runs exactly once on first use |
| Synchronized blocks | `synchronized (lock) { ... }` — `monitorenter`/`monitorexit` bytecodes, reentrant locking on the same object |
| Arithmetic ops | subtraction, division, remainder, negation for `int`, `long`, `double` |
| Bitwise / shifts | `<<`, `>>`, `>>>`, `|`, `^` for `int` and `long` |
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
| Enums | Java `enum` declarations — `values()`, `name()`, `ordinal()`, and `switch` over enum constants |
| Autoboxing | `Integer`, `Boolean`, `Long`, `Float`, `Double` — `valueOf` / `intValue` etc.; enables storing primitives in `ArrayList<Integer>` etc. |
| String (extended) | `split`, `replace`, `concat`, `toCharArray`, `hashCode` in addition to the predicates / search / transform methods listed above |

## Garbage Collection

The JVM runs a **stop-the-world mark-sweep collector** automatically. After every 256 heap allocations it scans all roots (frame locals, operand stacks, static fields), marks every reachable object, array, and string, then frees everything unreachable. There is no action needed from app code — GC fires transparently between bytecode instructions.

To introspect GC behavior from Java code, use `picodroid.os.Runtime`:

```java
import picodroid.os.Runtime;

int  count = Runtime.gcCount();      // cycles run since boot (or last reset)
int  freed = Runtime.gcFreed();      // entries freed across all cycles
long ns    = Runtime.gcTimeNanos();  // cumulative time spent in GC
Runtime.resetGcStats();              // zero all counters
```
