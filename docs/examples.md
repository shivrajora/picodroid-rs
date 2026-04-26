# Examples

Fifty examples are included under `examples/`, organized by category.

## Getting Started

Simple apps to verify your setup and get familiar with the build/flash workflow.

| Example | Class | Description |
|---------|-------|-------------|
| `helloworld` | `helloworld.HelloWorld` | Prints "Hello, World!" via `Log.i()` |
| `blinky` | `blinky.LedBlink` | Blinks the onboard LED on GP25 every 500 ms |

## Peripherals

Hardware interaction through the `picodroid.pio.PeripheralManager` API. Reference: [api/peripherals.md](api/peripherals.md).

| Example | Class | Description |
|---------|-------|-------------|
| `uart` | `uart.UartEcho` | Configures UART0 at 115200 baud and echoes received bytes |
| `i2cdemo` | `i2cdemo.I2cDemo` | Scans the I2C0 bus (SDA=GP4, SCL=GP5) and logs the 7-bit address of every ACKing device |
| `spidemo` | `spidemo.SpiDemo` | Full-duplex loopback over SPI0 (SCK=GP2, MOSI=GP3, MISO=GP0): sends 0x00-0x0F and logs received bytes |
| `adcdemo` | `adcdemo.AdcDemo` | Opens the ADC on GP26 and takes 5 voltage readings, logging each value |
| `pwmdemo` | `pwmdemo.PwmDemo` | Fades the onboard LED on GP25 using PWM at 1 kHz -- duty cycle sweeps 0%->100%->0% three times |

## Filesystem and Preferences

On-device persistent storage via LittleFS (`picodroid.io`) and the DataStore-style key-value API (`picodroid.content.Preferences`). Reference: [api/storage.md](api/storage.md).

| Example | Class | Description |
|---------|-------|-------------|
| `bootcount` | `bootcount.BootCount` | Persists a boot counter across reboots using `picodroid.io.File` / `FileInputStream` / `FileOutputStream` |
| `prefs_demo` | `prefsdemo.PrefsDemo` | Stores typed key/value settings (`String`, `int`, `long`, `boolean`) via `Preferences.open()` / `edit().commit()` |

## Networking

TCP/UDP sockets via `picodroid.net`. On hardware these require a Pico 2 W (`--board testbench_rp2350w`); under the simulator they hit the host network stack. Reference: [api/networking.md](api/networking.md).

| Example | Class | Description |
|---------|-------|-------------|
| `netdemo` | `netdemo.NetDemo` | Checks `NetworkInfo`, opens a TCP `Socket`, sends "Hello" to a localhost echo server on port 7000, and logs the response |
| `http_get` | `http_get.HttpGet` | Android-style `HttpUrlConnection` demo: performs a GET and a POST against a localhost HTTP/1.1 server, reading the response body through `HttpInputStream` |

## Sensors

Environmental / hardware sensors exposed through the Android-compatible `SensorManager`. Reference: [api/sensors.md](api/sensors.md).

| Example | Class | Description |
|---------|-------|-------------|
| `sensordemo` | `sensordemo.SensorDemoActivity` | Registers a `SensorEventListener` on the default ambient-temperature sensor (BME688) and logs each reading; requires a `[[sensor]]` entry in `board.toml` |

## Concurrency

Executors and cross-thread dispatch. Reference: [api/system.md → Executors](api/system.md#picodroidconcurrentexecutors).

| Example | Class | Description |
|---------|-------|-------------|
| `executordemo` | `executordemo.ExecutorDemoActivity` | Posts Runnables via both `Executors.mainExecutor()` and `Executors.backgroundExecutor()`; verifies main-thread FIFO ordering and cross-pool dispatch |

## Language Features

Demonstrate Java language features supported by the JVM interpreter. Reference: [api/core.md](api/core.md).

| Example | Class | Description |
|---------|-------|-------------|
| `arraydemo` | `arraydemo.ArrayDemo` | Demonstrates byte array allocation, `.length`, and iteration via UART |
| `inherit` | `inherit.InheritDemo` | Demonstrates class inheritance, field inheritance, method overriding, and `super()` |
| `interfacedemo` | `interfacedemo.InterfaceDemo` | Demonstrates interface dispatch (`invokeinterface`) with `Dog` and `Cat` implementing `Speakable` |
| `floatdemo` | `floatdemo.FloatDemo` | Demonstrates `float`, `long`, and `double` arithmetic and type conversions (`f2i`, `i2l`, `i2d`, etc.) |
| `exceptiondemo` | `exceptiondemo.ExceptionDemo` | Demonstrates `throw`, `try`/`catch`, and custom exception classes |
| `threaddemo` | `threaddemo.ThreadDemo` | Demonstrates spawning concurrent FreeRTOS tasks via `picodroid.concurrent.Thread` |
| `mathsdemo` | `mathsdemo.MathsDemo` | Demonstrates integer/long/double arithmetic, bitwise/shift ops, cross-type conversions, `tableswitch`, `instanceof`, `checkcast`, reference arrays, and `java.lang.Math` |
| `stringdemo` | `stringdemo.StringDemo` | Demonstrates `java.lang.String` and `StringBuilder` APIs: predicates, search, transforms, `String.valueOf`, and StringBuilder building |
| `stringtest` | `stringtest.StringTest` | Exercises the newer `String` methods: `split`, `replace`, `concat`, `toCharArray`, `hashCode` |
| `strformat` | `strformat.StrFormat` | Exhaustive `String.format` coverage across every supported conversion, flag, width, and precision |
| `enumdemo` | `enumdemo.EnumDemo` | Demonstrates Java `enum` declarations, `values()`, `name()`, `ordinal()`, and `switch` over enums |
| `listdemo` | `listdemo.ListDemo` | Demonstrates `java.util.ArrayList`: add, get, set, remove, contains, clear, and autoboxing with `Integer` and `Boolean` |
| `hashmaptest` | `hashmaptest.HashMapTest` | Demonstrates `java.util.HashMap` and `HashSet` — put/get/remove, key iteration, autoboxed keys |
| `iteratordemo` | `iteratordemo.IteratorDemo` | Demonstrates `Iterable` / `Iterator` and the enhanced `for (T x : collection)` loop |
| `trywithresourcesdemo` | `trywithresourcesdemo.TryWithResourcesDemo` | Demonstrates `try`-with-resources (`AutoCloseable`) -- opens an ADC pin in a `try` block and confirms `close()` is called on exit |
| `lambdademo` | `lambdademo.LambdaDemo` | Demonstrates Java lambdas via `invokedynamic`: non-capturing, capturing, callbacks, and static method references |
| `anondemo` | `anondemo.AnonDemo` | Demonstrates anonymous classes implementing interfaces, with local variable capture |
| `clinitdemo` | `clinitdemo.ClinitDemo` | Demonstrates static class initializers (`<clinit>`): field initializers, `static {}` blocks, and cross-class chaining |
| `syncdemo` | `syncdemo.SyncDemo` | Demonstrates `synchronized` blocks (`monitorenter`/`monitorexit`) and reentrant locking |
| `arraysdemo` | `arraysdemo.ArraysDemo` | Demonstrates `java.util.Arrays.sort` (stable mergesort over `Comparable[]`) and `Arrays.toString` |
| `collectionsdemo` | `collectionsdemo.CollectionsDemo` | Demonstrates `java.util.Collections.sort` and `Collections.reverse` over an `ArrayList` |
| `randomdemo` | `randomdemo.RandomDemo` | Demonstrates `java.util.Random` — `nextInt`, `nextLong`, `nextFloat`, seeded reproducibility |
| `clockdemo` | `clockdemo.ClockDemo` | Demonstrates `System.currentTimeMillis()` for boot-elapsed wallclock-style timing |
| `langsuite` | `langsuite.LangSuite` | Aggregated language-feature test runner — exercises every JVM language feature in one APK |
| `bytecodecoverage` | `bytecodecoverage.BytecodeCoverage` | JVM bytecode coverage harness — exercises long/double arrays, `multianewarray`, `wide`, `goto_w`, and stack-manipulation opcodes |

## Graphics and Display

Full graphical UI with touch input, demonstrating the Activity lifecycle and LVGL widget system. Reference: [api/ui.md](api/ui.md).

| Example | Class | Description |
|---------|-------|-------------|
| `displaydemo` | `displaydemo.DisplayDemoApp` | Showcases the full widget set on a 320x240 display: `LinearLayout`, `ScrollView`, `TextView`, `Button`, `ToggleButton`, `Switch`, `CheckBox`, `SeekBar`, `Spinner`, `EditText`, touch input, event handlers, and a moving-average FPS overlay (`Display.showFps()`) |
| `keydemo` | `keydemo.KeyDemoActivity` | Hardware-button demo: installs an `OnKeyListener` on a focusable `Button` and displays each `KeyEvent`'s action + keycode; requires `[[button]]` entries in `board.toml` |
| `callbacktest` | `callbacktest.CallbackTestActivity` | Regression harness for widget callback dispatch under both shrink modes — registers a lambda listener on every widget type and synthetically fires its event |
| `dialogdemo` | `dialogdemo.DialogDemoApp` | `Toast.makeText().show()` and `AlertDialog.Builder` with positive / negative listeners; demonstrates `onBackPressed()` confirmation pattern |
| `themedemo` | `themedemo.ThemeDemoApp` | Customizes the `Theme` color palette and applies `GradientDrawable` backgrounds (solid fills, corner radii, two-color gradients) |
| `gesturedemo` | `gesturedemo.GestureDemoApp` | `GestureDetector` with `onSingleTap` / `onLongPress` / `onFling` listeners on a single View |
| `animdemo` | `animdemo.AnimDemoApp` | `view.animate().alpha(...).x(...).y(...).setDuration(...).start()` — interpolated property animations |
| `keyboarddemo` | `keyboarddemo.KeyboardDemoApp` | Soft keyboard: both system-on-touch (default) and explicit `Keyboard` instances bound to `EditText` |
| `navdemo` | `navdemo.NavDemoApp` | Multi-Activity back-stack — `startActivity()` push, `finish()` pop, lifecycle callbacks (`onPause` / `onStop` / `onResume`) |

## Performance and Testing

Benchmarks and stress tests for the JVM runtime and allocator. Reference: [api/system.md](api/system.md) (`Runtime.gcCount()`, `Runtime.gcTimeNanos()`).

| Example | Class | Description |
|---------|-------|-------------|
| `benchmark` | `benchmark.Benchmark` | JVM performance benchmark: times int/long/float/double arithmetic, method dispatch, interface dispatch, object allocation, array ops, string ops, and control flow; logs per-category and total elapsed time |
| `gcstress` | `gcstress.GcStress` | GC stress test: exercises the mark-sweep collector under object churn, linked chains, circular references, string churn, and array churn; reports cycle count, freed entries, and GC time via `picodroid.os.Runtime` |
| `heapstress` | `heapstress.HeapStress` | Allocation/fragmentation stress test exercising the array arena allocator and emergency-GC path |

## Running an Example

```bash
./scripts/build.sh --app <name>
./scripts/flash.sh --app <name>
```

Or test on the host simulator without hardware:

```bash
./scripts/sim.sh --app <name>
```

See [getting-started.md](getting-started.md) for full build and flash options.
