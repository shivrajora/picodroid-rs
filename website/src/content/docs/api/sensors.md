---
title: "Sensors"
description: "SensorManager, SensorEventListener, and the BME688 / LTR559 driver bindings."
---

`picodroid.hardware.*` — Android-compatible `SensorManager` for environmental sensors declared in [`board.toml`](/reference/porting-guide/#boardtoml-reference). Today the supported devices are the Bosch **BME688** (temperature, humidity, pressure, gas resistance) and the Lite-On **LTR559** (ambient light, proximity), both over I2C. See [Java API overview](/api/) for the full API index.

Events are delivered on the main loop (no background task), so listeners run in the same thread as the Activity. Up to eight concurrent registrations are supported.

## Quick start

```java
import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;

public class TempActivity extends Activity implements SensorEventListener {
    public void onCreate() {
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

Retrieved via `Activity.getSystemService(Context.SENSOR_SERVICE)` (Android-style) or `SensorManager.getInstance()`.

| Method | Description |
|--------|-------------|
| `Sensor getDefaultSensor(int type)` | Returns the default sensor of a given type, or `null` if this board doesn't expose one. |
| `boolean registerListener(SensorEventListener l, Sensor s, int samplingPeriodUs)` | Registers `l` for events from `s`. Returns `false` if the 8-registration cap is hit. `samplingPeriodUs` takes one of the `SENSOR_DELAY_*` constants (below); raw microsecond values are not supported yet. |
| `void unregisterListener(SensorEventListener l)` | Removes every registration owned by `l`. Safe to call if `l` was never registered. |

### Sampling rate constants

The constants map to a tick count on the 16 ms main-loop period (so FASTEST ≈ 60 Hz, NORMAL ≈ 5 Hz):

| Constant | Value | Approximate rate |
|----------|-------|------------------|
| `SENSOR_DELAY_FASTEST` | 0 | every tick (~62 Hz) |
| `SENSOR_DELAY_GAME` | 1 | every 2 ticks (~31 Hz) |
| `SENSOR_DELAY_UI` | 2 | every 4 ticks (~15 Hz) |
| `SENSOR_DELAY_NORMAL` | 3 | every 12 ticks (~5 Hz) |

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

Getters: `getType()`, `getName()`, `getVendor()`, `getMaximumRange()`, `getResolution()`, `getMinDelay()`.

## `picodroid.hardware.SensorEvent`

Plain data class passed to `onSensorChanged`. All fields are public:

```java
public Sensor sensor;   // sensor that produced this event
public float[] values;  // values[0] is the primary reading; see table above
public int accuracy;    // 0 (unreliable) .. 3 (high)
public long timestamp;  // nanoseconds since boot (SystemClock.elapsedRealtimeNanos)
```

The framework reuses a single `SensorEvent` instance across callbacks, so copy any values you need to keep.

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

The BME688 driver ([src/drivers/bme688/](https://github.com/shivrajora/picodroid-rs/tree/main/src/drivers/bme688/)) handles Bosch compensation. Read-only for now — calibration and heater-profile control are not exposed. The LTR559 driver lives at [src/drivers/ltr559.rs](https://github.com/shivrajora/picodroid-rs/blob/main/src/drivers/ltr559.rs) and exposes light (lux) plus raw proximity counts; gain and integration-time control are not yet exposed. See [Porting guide](/reference/porting-guide/#boardtoml-reference) for the full board.toml schema.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads, executors) · [peripherals.md](/api/peripherals/) (direct I2C / SPI access) · [ui.md](/api/ui/) (display, widgets)
