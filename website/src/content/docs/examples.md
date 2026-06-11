---
title: "Examples"
description: "The example apps shipped under examples/, grouped by feature area."
---

Fifty-five examples are included under `examples/`, organized by category.

New to Picodroid? Start with the two guided tutorials below — they walk through building a real app step by step. The rest of the catalog is reference material to copy from.

## Tutorials

Companion apps for the step-by-step [Tutorials](/tutorials/multi-screen-app/). Build them with the guide open alongside.

| Example | Class | Description |
|---------|-------|-------------|
| `tutorial_screens` | `tutorial_screens.TutorialScreensApp` | A Home hub that pushes Counter and About screens — Activities, the back stack, lifecycle, and view preservation. Follow [Multi-screen app](/tutorials/multi-screen-app/). |
| `tutorial_service` | `tutorial_service.TutorialServiceApp` | An uptime-logging Service that survives navigation, bound from a viewer screen for live snapshots. Follow [Background service](/tutorials/background-service/). |

## Getting Started

Simple apps to verify your setup and get familiar with the build/flash workflow.

| Example | Class | Description |
|---------|-------|-------------|
| `helloworld` | `helloworld.HelloWorld` | Prints "Hello, World!" via `Log.i()` |
| `blinky` | `blinky.LedBlink` | Blinks the onboard LED on GP25 every 500 ms |

## Peripherals

Hardware interaction through the `picodroid.pio.PeripheralManager` API. Reference: [Peripherals](/api/peripherals/).

| Example | Class | Description |
|---------|-------|-------------|
| `uart` | `uart.UartEcho` | Configures UART0 at 115200 baud and echoes received bytes |
| `i2cdemo` | `i2cdemo.I2cDemo` | Scans the I2C0 bus (SDA=GP4, SCL=GP5) and logs the 7-bit address of every ACKing device |
| `spidemo` | `spidemo.SpiDemo` | Full-duplex loopback over SPI0 (SCK=GP2, MOSI=GP3, MISO=GP0): sends 0x00-0x0F and logs received bytes |
| `adcdemo` | `adcdemo.AdcDemo` | Opens the ADC on GP26 and takes 5 voltage readings, logging each value |
| `pwmdemo` | `pwmdemo.PwmDemo` | Fades the onboard LED on GP25 using PWM at 1 kHz -- duty cycle sweeps 0%->100%->0% three times |

## Filesystem and Preferences

On-device persistent storage via LittleFS (`picodroid.io`) and the DataStore-style key-value API (`picodroid.content.Preferences`). Reference: [Storage](/api/storage/).

| Example | Class | Description |
|---------|-------|-------------|
| `bootcount` | `bootcount.BootCount` | Persists a boot counter across reboots using `picodroid.io.File` / `FileInputStream` / `FileOutputStream` |
| `prefs_demo` | `prefsdemo.PrefsDemo` | Stores typed key/value settings (`String`, `int`, `long`, `boolean`) via `Preferences.open()` / `edit().commit()` |

## Networking

TCP/UDP sockets via `picodroid.net`. On hardware these require a Pico 2 W (`--board testbench_rp2350w`); under the simulator they hit the host network stack. Reference: [Networking](/api/networking/).

| Example | Class | Description |
|---------|-------|-------------|
| `netdemo` | `netdemo.NetDemo` | Checks `NetworkInfo`, opens a TCP `Socket`, sends "Hello" to a localhost echo server on port 7000, and logs the response |
| `http_get` | `http_get.HttpGet` | Android-style `HttpUrlConnection` demo: performs a GET and a POST against a localhost HTTP/1.1 server, reading the response body through `HttpInputStream` |

## Sensors

Environmental / hardware sensors exposed through the Android-compatible `SensorManager`. Reference: [Sensors](/api/sensors/).

| Example | Class | Description |
|---------|-------|-------------|
| `sensordemo` | `sensordemo.SensorDemoActivity` | Registers a `SensorEventListener` on the default ambient-temperature sensor (BME688) and logs each reading; requires a `[[sensor]]` entry in `board.toml` |

## Concurrency

Executors and cross-thread dispatch. Reference: [api/system.md → Executors](/api/system/#picodroidconcurrentexecutors).

| Example | Class | Description |
|---------|-------|-------------|
| `executordemo` | `executordemo.ExecutorDemoActivity` | Posts Runnables via both `Executors.mainExecutor()` and `Executors.backgroundExecutor()`; verifies main-thread FIFO ordering and cross-pool dispatch |

## Services

Background components with lifecycle independent of any Activity. Reference: [Services & DI](/api/services/); for a guided build see the [Background service tutorial](/tutorials/background-service/).

| Example | Class | Description |
|---------|-------|-------------|
| `servicedemo` | `servicedemo.ServiceDemoApp` | Drives a `CounterService` through the full Service v1 lifecycle in one non-UI run: two `startService` calls (one `onCreate`, two `onStartCommand`), `bindService` with a `LocalBinder` peek, `unbindService`, then `stopService` (triggers `onDestroy` and the foreground-notification cancel). Uses `startForeground(id, Notification)` |

## Language Features

Demonstrate Java language features supported by the JVM interpreter. Reference: [Core language](/api/core/).

| Example | Class | Description |
|---------|-------|-------------|
| `inherit` | `inherit.InheritDemo` | Demonstrates class inheritance, field inheritance, method overriding, and `super()` |
| `interfacedemo` | `interfacedemo.InterfaceDemo` | Demonstrates interface dispatch (`invokeinterface`) with `Dog` and `Cat` implementing `Speakable` |
| `floatdemo` | `floatdemo.FloatDemo` | Demonstrates `float`, `long`, and `double` arithmetic and type conversions (`f2i`, `i2l`, `i2d`, etc.) |
| `exceptiondemo` | `exceptiondemo.ExceptionDemo` | Demonstrates `throw`, `try`/`catch`, and custom exception classes |
| `threaddemo` | `threaddemo.ThreadDemo` | Demonstrates spawning concurrent FreeRTOS tasks via `picodroid.concurrent.Thread` |
| `mathsdemo` | `mathsdemo.MathsDemo` | Demonstrates integer/long/double arithmetic, bitwise/shift ops, cross-type conversions, `tableswitch`, `instanceof`, `checkcast`, reference arrays, and `java.lang.Math` |
| `stringdemo` | `stringdemo.StringDemo` | Test-harness coverage of `java.lang.String`, `StringBuilder`, and `String.format`: predicates, search, transforms, `valueOf`, `concat`/`replace`/`split`/`toCharArray`/`hashCode`, plus exhaustive printf-style conversions, flags, widths, and precision |
| `enumdemo` | `enumdemo.EnumDemo` | Demonstrates Java `enum` declarations, `values()`, `name()`, `ordinal()`, and `switch` over enums |
| `trywithresourcesdemo` | `trywithresourcesdemo.TryWithResourcesDemo` | Demonstrates `try`-with-resources (`AutoCloseable`) -- opens an ADC pin in a `try` block and confirms `close()` is called on exit |
| `lambdademo` | `lambdademo.LambdaDemo` | Demonstrates Java lambdas via `invokedynamic`: non-capturing, capturing, callbacks, and static method references |
| `anondemo` | `anondemo.AnonDemo` | Demonstrates anonymous classes implementing interfaces, with local variable capture |
| `clinitdemo` | `clinitdemo.ClinitDemo` | Demonstrates static class initializers (`<clinit>`): field initializers, `static {}` blocks, and cross-class chaining |
| `classlit` | `classlit.ClassLit` | Class literals (`T.class`): demonstrates `getName()` and that repeated `T.class` evaluations return the same `Class` instance |
| `syncdemo` | `syncdemo.SyncDemo` | Demonstrates `synchronized` blocks (`monitorenter`/`monitorexit`) and reentrant locking |
| `collectionsdemo` | `collectionsdemo.CollectionsDemo` | Test-harness coverage of `java.util.*`: `ArrayList` add/get/set/remove/contains/clear plus `Integer`/`Boolean` autoboxing, `Arrays.sort`/`copyOf`/`fill`/`toString`, `Arrays.sort(Object[])` and `Collections.sort`/`reverse` over `Comparable<T>`, explicit `Iterator` and enhanced for-each over lists/maps, `HashMap` and `HashSet` |
| `randomdemo` | `randomdemo.RandomDemo` | Demonstrates `java.util.Random` — `nextInt`, `nextLong`, `nextFloat`, seeded reproducibility |
| `clockdemo` | `clockdemo.ClockDemo` | Demonstrates `System.currentTimeMillis()` for boot-elapsed wallclock-style timing |
| `langsuite` | `langsuite.LangSuite` | Aggregated language-feature test runner — exercises every JVM language feature in one APK |
| `bytecodecoverage` | `bytecodecoverage.BytecodeCoverage` | JVM bytecode coverage harness — exercises long/double arrays, `multianewarray`, `wide`, `goto_w`, and stack-manipulation opcodes |

## Graphics and Display

Full graphical UI with touch input, demonstrating the Activity lifecycle and LVGL widget system. Reference: [Graphics & UI](/api/ui/); see also [Embedded gotchas](/guides/embedded-gotchas/) for the UI pitfalls these apps avoid.

| Example | Class | Description |
|---------|-------|-------------|
| `displaydemo` | `displaydemo.DisplayDemoApp` | Showcases the full widget set on a 320x240 display: `LinearLayout`, `ScrollView`, `TextView`, `Button`, `ToggleButton`, `Switch`, `CheckBox`, `SeekBar`, `Spinner`, `EditText`, touch input, event handlers, a moving-average FPS overlay (`DisplayDebug.showFps()`), and a themed-widgets section using a custom `Theme` palette with `GradientDrawable` (gradient header, surface card, pill / ghost buttons) |
| `keydemo` | `keydemo.KeyDemoActivity` | Hardware-button demo: installs an `OnKeyListener` on a focusable `Button` and displays each `KeyEvent`'s action + keycode; requires `[[button]]` entries in `board.toml` |
| `callbacktest` | `callbacktest.CallbackTestActivity` | Regression harness for widget callback dispatch under both shrink modes — registers a lambda listener on every widget type and synthetically fires its event |
| `dialogdemo` | `dialogdemo.DialogDemoApp` | `Toast.makeText().show()` and `AlertDialog.Builder` with positive / negative listeners; demonstrates `onBackPressed()` confirmation pattern |
| `gesturedemo` | `gesturedemo.GestureDemoApp` | `GestureDetector` with `onSingleTap` / `onLongPress` / `onFling` listeners on a single View |
| `dragdemo` | `dragdemo.DragDemoActivity` | Touch-driven drag using `FrameLayout` + `OnTouchListener`; tracks `MotionEvent.ACTION_DOWN/MOVE/UP` and updates a tile's absolute position via `setPosition()`. The only example using `FrameLayout` for absolute placement (a `LinearLayout` would re-flow on every layout pass) |
| `animdemo` | `animdemo.AnimDemoApp` | `view.animate().alpha(...).x(...).y(...).setDuration(...).start()` — interpolated property animations |
| `keyboarddemo` | `keyboarddemo.KeyboardDemoApp` | Soft keyboard: both system-on-touch (default) and explicit `Keyboard` instances bound to `EditText`. Includes the soft-keyboard polish pass: slide-up animation, `OnEditorActionListener` for the Done key, dismiss-on-outside-tap |
| `navdemo` | `navdemo.NavDemoApp` | Multi-Activity back-stack — `startActivity()` push, `finish()` pop, lifecycle callbacks (`onPause` / `onStop` / `onResume`) |
| `imagedemo` | `imagedemo.ImageDemoApp` | `ImageView.setImageSource("name.png")` resolving against the PAPK ASSETS section (v1.1 bundled images). Demonstrates `setScaleType` / `setTint` / `setScale`. See [Bundled image assets](/guides/assets/) |
| `pickerdemo` | `pickerdemo.PickerDemoApp` | `DatePicker` (lv_calendar binding) and `TimePicker` (lv_roller binding) with 12-hour / AM-PM mode and value-changed listeners |
| `snackbardemo` | `snackbardemo.SnackbarDemoApp` | `Snackbar.make().setAction().show()` — toast with a clickable action lozenge, auto-dismiss, click-through-to-listener |
| `swipedemo` | `swipedemo.SwipeDemoApp` | `OnSwipeListener` (UP / DOWN / LEFT / RIGHT direction constants) on a single view; `SwipeRefreshLayout` pull-to-refresh container |

## Performance and Testing

Benchmarks and stress tests for the JVM runtime and allocator. Reference: [System & concurrency](/api/system/) (`Runtime.gcCount()`, `Runtime.gcTimeNanos()`); for the numbers these probe, see [Limits & memory budgets](/reference/limits/) and [JVM tunables](/reference/jvm-tunables/).

| Example | Class | Description |
|---------|-------|-------------|
| `benchmark` | `benchmark.Benchmark` | JVM performance benchmark: times int/long/float/double arithmetic, method dispatch, interface dispatch, object allocation, array ops, string ops, and control flow; logs per-category and total elapsed time |
| `perfbench` | `perfbench.PerfBench` | Unified speed + memory benchmark — rolls execution-time and heap-usage measurements into a single composite SCORE for tracking runtime regressions |
| `graphicsbench` | `graphicsbench.GraphicsBench` | LVGL render-pipeline benchmark — exercises the draw/refresh path across several test cases and reports a composite SCORE |
| `gcstress` | `gcstress.GcStress` | GC stress test: exercises the mark-sweep collector under object churn, linked chains, circular references, string churn, and array churn; reports cycle count, freed entries, and GC time via `picodroid.os.Runtime` |
| `heapstress` | `heapstress.HeapStress` | Allocation/fragmentation stress test exercising the array arena allocator and emergency-GC path |
| `tracedemo` | `tracedemo.TraceDemo` | Bytecode-level tracing harness — exercises the JVM's diagnostic trace mode for verifying interpreter behavior under controlled inputs |

## Feature Showcase

End-to-end apps that combine multiple subsystems. These are the closest reference for the layout and structure of a "real" picodroid app.

| Example | Class | Description |
|---------|-------|-------------|
| `picoenvmon` | `picoenvmon.EnvApp` | Environmental monitor for the Pimoroni Enviro+ Pack. Multi-Activity (`HomeActivity`, settings, history) with a sub-package layout (`di/`, `ui/`, `service/`, `hardware/`, `data/`, `util/`); customizes the global `Theme` palette in `Application.onCreate()`; runs a `SensorLoggerService` ring-buffering BME688 + LTR559 readings; drives an RGB LED. Demonstrates the manual DI pattern (`EnvAppComponent`, `ActivitySingletonComponent`) |

## Running an Example

```bash
./scripts/build.sh --app <name>
./scripts/flash.sh --app <name>
```

Or test on the host simulator without hardware:

```bash
./scripts/sim.sh --app <name>
```

See [Build & flash](/get-started/build/) for full build and flash options.
