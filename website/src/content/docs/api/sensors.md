---
title: "Sensors"
description: "SensorManager, SensorEventListener, and the BME688 / LTR559 driver bindings."
---

`picodroid.hardware.*` — Android-compatible `SensorManager` for environmental sensors declared in [`board.toml`](/reference/porting-guide/#boardtoml-reference). Today the supported devices are the Bosch **BME688** (temperature, humidity, pressure, gas resistance) and the Lite-On **LTR559** (ambient light, proximity), both over I2C. See [Java API overview](/api/) for the full API index.

The sensors are read off the UI thread, by a sampler task that owns the I2C traffic and runs only as fast as the fastest registration asks. Listeners are called on the main thread, the same thread as the Activity, so `onSensorChanged` may touch widgets. Up to eight concurrent registrations are supported.

## Quick start

```java
import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.os.Bundle;
import picodroid.util.Log;

public class TempActivity extends Activity implements SensorEventListener {
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        SensorManager mgr = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
        Sensor temp = mgr.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE);
        mgr.registerListener(this, temp, SensorManager.SENSOR_DELAY_NORMAL);
    }

    public void onSensorChanged(SensorEvent event) {
        Log.i("TempDemo", "temp=" + event.values[0] + "C");
    }

    public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
```

See the full [`sensordemo`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/sensordemo) example.

## `picodroid.hardware.SensorManager`

Retrieved via `getSystemService(Context.SENSOR_SERVICE)` on any `Context` — an `Activity`, the `Application`, or a `Service` (Android-style) — or `SensorManager.getInstance()`.

| Method | Description |
|--------|-------------|
| `Sensor getDefaultSensor(int type)` | Returns the sensor of a given type, or `null` for a type picodroid has no driver for (`TYPE_ACCELEROMETER`, `TYPE_MAGNETIC_FIELD`, `TYPE_GYROSCOPE`). The same `Sensor` object is returned on every call. |
| `boolean registerListener(SensorEventListener l, Sensor s, int samplingPeriodUs)` | Registers `l` for events from `s`. Returns `false` if the 8-registration cap is hit. `samplingPeriodUs` is one of the `SENSOR_DELAY_*` constants (below) or a period in microseconds, as on Android. Registering the same listener for the same sensor again changes its rate and takes no new slot. |
| `void unregisterListener(SensorEventListener l)` | Removes every registration owned by `l`. Safe to call if `l` was never registered. |

The lookup is by type, not by wiring: the six supported types answer on every board. Where the board has no chip behind a type, its listener receives zeros at its cadence. The simulator serves synthetic triangle waves for all six.

### Sampling rate constants

The constants map to a tick count on the 16 ms main-loop period (so FASTEST ≈ 60 Hz, NORMAL ≈ 5 Hz). A period in microseconds is rounded to the nearest tick, with a minimum of one: `20_000` is every tick, `1_000_000` is every 63 ticks.

| Constant | Value | Approximate rate |
|----------|-------|------------------|
| `SENSOR_DELAY_FASTEST` | 0 | every tick (~62 Hz) |
| `SENSOR_DELAY_GAME` | 1 | every 2 ticks (~31 Hz) |
| `SENSOR_DELAY_UI` | 2 | every 4 ticks (~15 Hz) |
| `SENSOR_DELAY_NORMAL` | 3 | every 12 ticks (~5 Hz) |

What a listener actually sees is bounded in three ways. At most one `onSensorChanged` is delivered per tick across all registrations, so five sensors registered at `SENSOR_DELAY_FASTEST` each arrive every fifth tick. A reading that was not delivered before its successor came due is replaced by it, as Android drops events under back-pressure. And a BME688 conversion takes about 45 ms, so its four types repeat a reading when asked for more often than that. Nothing is delivered while the display is in idle sleep, and the sampler stops with it.

## `picodroid.hardware.Sensor`

Immutable metadata. Construct via `SensorManager.getDefaultSensor()`.

| Type constant | Value | Units (in `SensorEvent.values[0]`) |
|---------------|-------|------------------------------------|
| `TYPE_LIGHT` | 5 | lux |
| `TYPE_PRESSURE` | 6 | hPa |
| `TYPE_PROXIMITY` | 8 | raw proximity counts (0..2047, higher = closer) |
| `TYPE_RELATIVE_HUMIDITY` | 12 | % RH |
| `TYPE_AMBIENT_TEMPERATURE` | 13 | °C |
| `TYPE_GAS_RESISTANCE` | 0x10001 | Ω (Picodroid extension — Android doesn't define this) |
| `TYPE_ALL` | -1 | sentinel, not a real sensor |
| `TYPE_ACCELEROMETER`, `TYPE_MAGNETIC_FIELD`, `TYPE_GYROSCOPE` | 1, 2, 4 | Android's values, for source compatibility. No board ships these sensors, so `getDefaultSensor` returns `null` for them. |

Getters: `getType()`, `getName()`, `getVendor()`, `getMaximumRange()`, `getResolution()`, `getMinDelay()`.

## `picodroid.hardware.SensorEvent`

Plain data class passed to `onSensorChanged`. All fields are public:

```java
public Sensor sensor;   // sensor that produced this event
public float[] values;  // values[0] is the primary reading; see table above
public int accuracy;    // SensorManager.SENSOR_STATUS_*; always SENSOR_STATUS_ACCURACY_HIGH (3)
public long timestamp;  // nanoseconds since boot (SystemClock.elapsedRealtimeNanos)
```

Each registration has one `SensorEvent` that the framework reuses for every callback, so copy any values you need to keep. `SensorManager` carries Android's accuracy constants — `SENSOR_STATUS_UNRELIABLE` (0), `SENSOR_STATUS_ACCURACY_LOW` (1), `SENSOR_STATUS_ACCURACY_MEDIUM` (2), `SENSOR_STATUS_ACCURACY_HIGH` (3) — but the accuracy never changes, so `onAccuracyChanged` is never called.

## `picodroid.hardware.SensorEventListener`

```java
public interface SensorEventListener {
    void onSensorChanged(SensorEvent event);
    void onAccuracyChanged(Sensor sensor, int accuracy);
}
```

## Hardware wiring

Each sensor must be declared in `board.toml`:

```toml
[[sensor]]
kind = "bme688"     # temperature / humidity / pressure / gas
bus  = "I2C0"       # "I2C0" or "I2C1"
addr = 0x77         # 7-bit I2C address

[[sensor]]
kind = "ltr559"     # ambient light + proximity
bus  = "I2C0"
addr = 0x23         # LTR559 default
```

The BME688 driver ([crates/pd-drivers/src/bme688/](https://github.com/shivrajora/picodroid-rs/tree/main/crates/pd-drivers/src/bme688/)) handles Bosch compensation. Read-only for now — calibration and heater-profile control are not exposed. The LTR559 driver lives at [crates/pd-drivers/src/ltr559.rs](https://github.com/shivrajora/picodroid-rs/blob/main/crates/pd-drivers/src/ltr559.rs) and exposes light (lux) plus raw proximity counts; gain and integration-time control are not yet exposed. See [Porting guide](/reference/porting-guide/#boardtoml-reference) for the full board.toml schema.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads, executors) · [peripherals.md](/api/peripherals/) (direct I2C / SPI access) · [ui.md](/api/ui/) (display, widgets)
