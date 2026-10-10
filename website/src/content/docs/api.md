---
title: "Java System API"
description: "Java system APIs grouped by area: peripherals, storage, networking, sensors, UI, and more."
---

Java system APIs live under `sdk/java/picodroid/` and mirror the Android API surface; the `java.*` classes the SDK ships as source (`java.time`, `java.util.Objects`, `java.util.zip.CRC32`, …) sit beside them under `sdk/java/java/`. Native implementations are in `crates/picodroid-core/src/native_handler/`, and the built-in `java.lang` / `java.util` classes in `crates/jvm/src/native/`.

The reference is split by package family. Pick the area you need.

| Area | Packages | Covers |
|------|----------|--------|
| [Core language](/api/core/) | `java.lang`, `java.util`, `java.util.zip`, `java.time`, `java.io` | `String` (incl. `String.format`, `String.join`), `StringBuilder`, `Math`, wrapper classes, exceptions, `Random`, `ArrayList`, `HashMap` / `HashSet`, `Iterator` / for-each, enums, `Arrays` / `Collections` / `List` / `Comparable` / `Comparator`, `Objects`, `CRC32`, `java.time`, the `java.io` streams (`InputStream` / `OutputStream`, `BufferedReader`, `PrintWriter`, …), `Class`, `AutoCloseable`, `System.arraycopy`, lambdas and method references |
| [System & concurrency](/api/system/) | `picodroid.util`, `picodroid.os`, `picodroid.content.pm`, `picodroid.concurrent` | `Log`, `SystemClock`, `System.currentTimeMillis`, `Runtime` (GC stats), `Build`, `PackageManager` / `PackageInstaller`, `Thread` and `Object.wait` / `notify`, `Executors` (main-thread FIFO + background pool), `ScheduledExecutorService`, `ExecutorService` / `Future` / `Callable`, the atomics, `CountDownLatch`, `TimeUnit` |
| [Services & DI](/api/services/) (Preview) | `picodroid.app`, `picodroid.content`, `javax.inject`, `picodroid.di` | `Service` / `IBinder` / `Notification` / `NotificationManager`, `AlarmManager` / `PendingIntent`, `bindService` / `startService`, `ServiceConnection`, compile-time DI (`@Inject` / `@Singleton`, `@Module` / `@Provides`, `Provider<T>` / `Lazy<T>`, automatic injection of `Application` / `Activity` / `Service`), manual DI components (`ApplicationComponent`, `ActivitySingletonComponent`) |
| [Peripherals](/api/peripherals/) | `picodroid.pio` | `PeripheralManager`, `Gpio` (output and input), `UartDevice`, `I2cDevice`, `SpiDevice`, `Pwm`, `Adc`, `AutoCloseable` idiom |
| [Storage](/api/storage/) | `picodroid.io`, `picodroid.content`, `picodroid.os`, `picodroid.app.usage` | `File` / `FileInputStream` / `FileOutputStream` (LittleFS), the per-app sandbox under `/data/<package>`, the `Context` private-file API, the storage reserve and per-app cap, `StatFs`, `StorageStatsManager`, `SharedPreferences` / `Editor` |
| [Networking](/api/networking/) | `picodroid.net` | `Socket`, `ServerSocket`, `DatagramSocket`, `DatagramPacket`, `InetAddress`, `NetworkInfo`, `ConnectivityManager` + `NetworkCallback` / `Network` / `NetworkCapabilities` / `NetworkRequest`, `HttpURLConnection` + `URL` (Pico 2 W on hardware; sim always works) |
| [JSON](/api/json/) | `picodroid.json` | `JSONObject`, `JSONArray`, `JSONException` with Android's `org.json` surface, over a native node pool (boards with `has_json = true`: every RP2350 board, not `testbench_rp2040`) |
| [Protocol Buffers](/api/protobuf/) | `picodroid.protobuf` | `CodedInputStream`, `CodedOutputStream`, `MessageLite`, `WireFormat`, `InvalidProtocolBufferException` with protobuf-javalite's surface over a native wire codec, plus `protoc-gen-picodroid` for message classes (boards with `has_protobuf = true`: every RP2350 board, not `testbench_rp2040`) |
| [Sensors](/api/sensors/) | `picodroid.hardware` | `SensorManager`, `Sensor`, `SensorEvent`, `SensorEventListener` — BME688 (temperature / humidity / pressure / gas), LTR559 (light / proximity) |
| [Audio](/api/media/) | `picodroid.media` | `ToneGenerator` with Android's tone table, `AudioManager` stream constants — square-wave tones and melodies on a board's piezo buzzer (boards with an `[audio]` section; no sampled audio anywhere) |
| [Graphics & UI](/api/ui/) | `picodroid.app`, `picodroid.graphics`, `picodroid.view`, `picodroid.widget`, `picodroid.debug` | `Application` / `Activity` full lifecycle + back stack, `Display` / `DisplayDebug`, `Color`, `Theme`, `GradientDrawable`, `View` (incl. `animate()`, per-View touch, focus nav), `ViewGroup`, `MotionEvent`, `GestureDetector`, `ViewPropertyAnimator`, `KeyEvent` (with auto-repeat and long-press) / `OnKeyListener` / `ViewConfiguration`, `OnSwipeListener`, typed listener interfaces, the `Adapter` / `ArrayAdapter` pattern, 20+ widgets including `Toast`, `AlertDialog`, `Keyboard`, `DatePicker`, `TimePicker`, `Snackbar`, `SwipeRefreshLayout`, `ImageView` |

Apps can be written in Kotlin against the same classes — see the [Kotlin guide](/guides/kotlin/). What each Android and `java.*` API supports, and what it leaves out, is tabulated in the [compatibility matrix](/reference/compatibility-matrix/).

## Quick example

A complete mini-app that opens a GPIO pin, blinks it, and logs the result. See [Peripherals](/api/peripherals/) for the full PIO surface and [System & concurrency](/api/system/) for `Log` and `SystemClock`.

```java
package myapp;

import picodroid.util.Log;
import picodroid.os.SystemClock;
import picodroid.pio.PeripheralManager;
import picodroid.pio.Gpio;

public class MyApp {
    public static void main(String[] args) {
        PeripheralManager pm = PeripheralManager.getInstance();
        try (Gpio led = pm.openGpio("GP25")) {
            led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
            for (int i = 0; i < 5; i++) {
                led.setValue(true);
                SystemClock.sleep(500);
                led.setValue(false);
                SystemClock.sleep(500);
                Log.i("MyApp", "Blink " + String.valueOf(i + 1));
            }
        }
    }
}
```
