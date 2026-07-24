// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.service;

import picodroid.app.Notification;
import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picoenvmon.data.SensorRingBuffer;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.hardware.RgbLed;
import picoenvmon.util.Formatter;

/**
 * Foreground Service that streams every sensor reading into a per-type ring buffer, drives the
 * Pack's RGB LED from gas IAQ, and logs threshold breaches. Bind via {@code bindService} to read
 * the ring buffers from a UI activity.
 */
public class SensorLoggerService extends Service implements SensorEventListener {
  private static final String TAG = "SensorLogger";
  private static final int NOTIFICATION_ID = 1;
  public static final int RING_CAPACITY = 60;

  /** Stable indices into the ring-buffer table — match {@link #ringFor(int)}. */
  public static final int IDX_TEMPERATURE = 0;

  public static final int IDX_HUMIDITY = 1;
  public static final int IDX_PRESSURE = 2;
  public static final int IDX_GAS = 3;
  public static final int IDX_LIGHT = 4;

  public static class LocalBinder implements IBinder {
    public SensorLoggerService service;
  }

  private final LocalBinder binder = new LocalBinder();
  private final SensorRingBuffer[] rings = {
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
    new SensorRingBuffer(RING_CAPACITY),
  };

  private SensorManager sensorManager;
  private RgbLed rgbLed;
  private ThresholdConfig thresholds;
  private float lastGas = -1f;
  private boolean started;

  // ── 1 Hz smoothing ───────────────────────────────────────────────────────────
  // Per-type windowed-mean accumulators. Raw callbacks arrive ~5 Hz in a single
  // burst; on the first callback past the 1 s mark we emit averages to every
  // registered SmoothedSensorListener and reset the accumulators.
  private static final long EMIT_INTERVAL_MS = 1000;
  private static final int NUM_SMOOTHED_SENSORS = 5;
  private static final int MAX_SMOOTHED_LISTENERS = 4;
  private static final int[] SMOOTHED_TYPES = {
    Sensor.TYPE_AMBIENT_TEMPERATURE,
    Sensor.TYPE_RELATIVE_HUMIDITY,
    Sensor.TYPE_PRESSURE,
    Sensor.TYPE_GAS_RESISTANCE,
    Sensor.TYPE_LIGHT,
  };

  private final float[] smoothSum = new float[NUM_SMOOTHED_SENSORS];
  private final int[] smoothCount = new int[NUM_SMOOTHED_SENSORS];
  private final SmoothedSensorListener[] smoothedListeners =
      new SmoothedSensorListener[MAX_SMOOTHED_LISTENERS];
  private long lastEmitMs;

  // ── Alert edge detection ─────────────────────────────────────────────────────
  // Raw callbacks re-evaluate thresholds several times per second; logging every
  // breached sample flooded the log and allocated a concat per line forever
  // (~13 allocs/s at idle with default thresholds indoors). Latch per sensor and
  // log only the transitions: once entering breach, once clearing. The latches
  // live on the instance, which survives bind/unbind churn; a service restart
  // re-logs at most one active breach per sensor.
  private static final int ALERT_TEMP = 0;
  private static final int ALERT_HUMIDITY = 1;
  private static final int ALERT_LIGHT = 2;
  private final boolean[] alertActive = new boolean[3];

  @Override
  public void onCreate() {
    binder.service = this;
    EnvAppComponent app = (EnvAppComponent) EnvAppComponent.current();
    rgbLed = app.rgbLed();
    thresholds = app.thresholds();

    sensorManager = SensorManager.getInstance();
    registerAll(sensorManager);
    Log.i(TAG, "onCreate");
  }

  private void registerAll(SensorManager mgr) {
    int[] types = {
      Sensor.TYPE_AMBIENT_TEMPERATURE,
      Sensor.TYPE_RELATIVE_HUMIDITY,
      Sensor.TYPE_PRESSURE,
      Sensor.TYPE_GAS_RESISTANCE,
      Sensor.TYPE_LIGHT,
    };
    for (int t : types) {
      Sensor s = mgr.getDefaultSensor(t);
      if (s != null) {
        mgr.registerListener(this, s, SensorManager.SENSOR_DELAY_NORMAL);
      } else {
        Log.i(TAG, "no default sensor for type=" + t);
      }
    }
  }

  @Override
  public int onStartCommand(Intent intent, int startId) {
    if (!started) {
      started = true;
      Notification n =
          new Notification.Builder()
              .setContentTitle("PicoEnvMon")
              .setContentText("Logging sensors")
              .build();
      startForeground(NOTIFICATION_ID, n);
      Log.i(TAG, "foreground started");
    }
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    return binder;
  }

  @Override
  public void onDestroy() {
    Log.i(TAG, "onDestroy");
    if (sensorManager != null) {
      sensorManager.unregisterListener(this);
    }
    if (rgbLed != null) {
      rgbLed.off();
    }
    stopForeground(true);
  }

  @Override
  public void onSensorChanged(SensorEvent event) {
    int type = event.sensor.getType();
    float v = event.values[0];
    SensorRingBuffer ring = ringFor(type);
    if (ring != null) {
      ring.add(v);
    }

    switch (type) {
      case Sensor.TYPE_GAS_RESISTANCE:
        lastGas = v;
        applyLedFromIaq(v);
        break;
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        alertEdge(ALERT_TEMP, thresholds.tempBreached(v), "temperature breach", v, " C");
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        alertEdge(
            ALERT_HUMIDITY, thresholds.humidityBreached(v), "humidity below threshold", v, " m%");
        break;
      case Sensor.TYPE_LIGHT:
        alertEdge(ALERT_LIGHT, thresholds.luxBreached(v), "light below threshold", v, " lx");
        break;
      default:
        break;
    }

    int smIdx = smoothedIdxFor(type);
    if (smIdx >= 0) {
      smoothSum[smIdx] += v;
      smoothCount[smIdx]++;
      long now = System.currentTimeMillis();
      if (lastEmitMs == 0) {
        lastEmitMs = now;
      }
      if (now - lastEmitMs >= EMIT_INTERVAL_MS) {
        emitSmoothed();
        lastEmitMs = now;
      }
    }
  }

  /**
   * Log threshold alerts only on state transitions: one line on entering breach, one on clearing.
   */
  private void alertEdge(int idx, boolean breached, String what, float v, String unit) {
    if (breached && !alertActive[idx]) {
      alertActive[idx] = true;
      Log.i(TAG, "ALERT: " + what + ": " + v + unit);
    } else if (!breached && alertActive[idx]) {
      alertActive[idx] = false;
      Log.i(TAG, "ALERT cleared: " + what + ": " + v + unit);
    }
  }

  /** Register for 1 Hz windowed-mean callbacks. Returns false if all slots are full. */
  public boolean addSmoothedListener(SmoothedSensorListener l) {
    for (int i = 0; i < MAX_SMOOTHED_LISTENERS; i++) {
      if (smoothedListeners[i] == null) {
        smoothedListeners[i] = l;
        return true;
      }
    }
    return false;
  }

  /** Idempotent: removing an unregistered listener is a no-op. */
  public void removeSmoothedListener(SmoothedSensorListener l) {
    for (int i = 0; i < MAX_SMOOTHED_LISTENERS; i++) {
      if (smoothedListeners[i] == l) {
        smoothedListeners[i] = null;
      }
    }
  }

  private static int smoothedIdxFor(int sensorType) {
    for (int i = 0; i < SMOOTHED_TYPES.length; i++) {
      if (SMOOTHED_TYPES[i] == sensorType) {
        return i;
      }
    }
    return -1;
  }

  private void emitSmoothed() {
    for (int i = 0; i < NUM_SMOOTHED_SENSORS; i++) {
      if (smoothCount[i] == 0) {
        continue;
      }
      float avg = smoothSum[i] / smoothCount[i];
      smoothSum[i] = 0f;
      smoothCount[i] = 0;
      for (int j = 0; j < MAX_SMOOTHED_LISTENERS; j++) {
        SmoothedSensorListener l = smoothedListeners[j];
        if (l != null) {
          l.onSmoothedSensor(SMOOTHED_TYPES[i], avg);
        }
      }
    }
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}

  private void applyLedFromIaq(float gasOhm) {
    if (rgbLed == null) {
      return;
    }
    int iaq = Formatter.iaqFromGas(gasOhm);
    // 0 (clean) → green; 250 → yellow; 500 → red.
    int r;
    int g;
    if (iaq < 250) {
      r = (int) (iaq * 255f / 250f);
      g = 255;
    } else {
      r = 255;
      g = (int) ((500 - iaq) * 255f / 250f);
      if (g < 0) {
        g = 0;
      }
    }
    rgbLed.setColor(r, g, 0);
  }

  private SensorRingBuffer ringFor(int type) {
    switch (type) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        return rings[IDX_TEMPERATURE];
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        return rings[IDX_HUMIDITY];
      case Sensor.TYPE_PRESSURE:
        return rings[IDX_PRESSURE];
      case Sensor.TYPE_GAS_RESISTANCE:
        return rings[IDX_GAS];
      case Sensor.TYPE_LIGHT:
        return rings[IDX_LIGHT];
      default:
        return null;
    }
  }

  /** Snapshot of one ring buffer. {@code idx} = one of {@link #IDX_TEMPERATURE} … */
  public int snapshot(int idx, float[] out) {
    if (idx < 0 || idx >= rings.length) {
      return 0;
    }
    return rings[idx].snapshot(out);
  }

  public float lastGas() {
    return lastGas;
  }

  /**
   * Whether the foreground logging pass is currently active — i.e. {@code startService} has run and
   * {@code stopService} has not. A started Service outlives the unbind that happens when the Live
   * screen is left, so a re-entering UI queries this to restore the Logger toggle to its true
   * state.
   */
  public boolean isLogging() {
    return started;
  }
}
