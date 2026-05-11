// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.home;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.util.Log;
import picodroid.view.GestureDetector;
import picodroid.view.GestureDetector.OnGestureListener;
import picodroid.view.KeyEvent;
import picodroid.view.MotionEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picodroid.widget.Toast;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.ui.history.HistoryActivity;
import picoenvmon.ui.settings.SettingsActivity;
import picoenvmon.util.Formatter;

/**
 * Live dashboard. Sensor listeners feed 5 tiles, button-key navigation (A=Settings, B=History,
 * X=toggle Service, Y=back), long-press toggles °C↔°F.
 */
public class HomeActivity extends Activity
    implements SensorEventListener, OnKeyListener, OnGestureListener {

  // Tile slot indices for the parallel arrays below.
  private static final int IDX_TEMP = 0;
  private static final int IDX_HUM = 1;
  private static final int IDX_PRESS = 2;
  private static final int IDX_IAQ = 3;
  private static final int IDX_LIGHT = 4;
  private static final int NUM_TILES = 5;

  private EnvActivityComponent comp;
  private SensorManager sensorManager;
  private boolean serviceRunning;

  /** Tile root LinearLayout per sensor index (used by flashOnBreach). */
  private final LinearLayout[] tileRoots = new LinearLayout[NUM_TILES];

  /** Tile value TextView per sensor index. */
  private final TextView[] tileValues = new TextView[NUM_TILES];

  private float lastGas = -1f;

  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Home.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(8, 6, 8, 6);
    root.setBackgroundColor(Theme.colorBackground);

    TextView title = new TextView();
    title.setText("PicoEnvMon");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    // One GradientDrawable shared across all 5 tile backgrounds — same color, radius and stroke,
    // no per-tile customization, so the previous 5 instances were redundant.
    GradientDrawable tileBg = new GradientDrawable();
    tileBg.setColor(Theme.colorSurface).setCornerRadius(6).setStroke(1, Theme.colorOutline);

    buildTile(root, IDX_TEMP, "Temp", tileBg);
    buildTile(root, IDX_HUM, "Humidity", tileBg);
    buildTile(root, IDX_PRESS, "Pressure", tileBg);
    buildTile(root, IDX_IAQ, "Air quality", tileBg);
    buildTile(root, IDX_LIGHT, "Light", tileBg);

    TextView footer = new TextView();
    footer.setText("A:Settings B:History X:Logger");
    footer.setTextColor(Theme.colorTextSecondary);
    root.addView(footer);

    setContentView(root);
    root.setOnKeyListener(this);
    root.setOnTouchListener(new GestureDetector(this));

    registerSensors();
  }

  public void onResume() {
    if (sensorManager != null) {
      registerSensors();
    }
  }

  public void onPause() {
    if (sensorManager != null) {
      sensorManager.unregisterListener(this);
    }
  }

  public void onDestroy() {
    if (sensorManager != null) {
      sensorManager.unregisterListener(this);
    }
  }

  private void buildTile(LinearLayout parent, int idx, String label, GradientDrawable bg) {
    LinearLayout tile = new LinearLayout();
    tile.setOrientation(LinearLayout.HORIZONTAL);
    tile.setSize(224, 28);
    tile.setPadding(8, 4, 8, 4);
    tile.setBackground(bg);

    TextView labelView = new TextView();
    labelView.setText(label);
    labelView.setTextColor(Theme.colorTextSecondary);
    labelView.setSize(96, 20);
    tile.addView(labelView);

    TextView valueView = new TextView();
    valueView.setText("—");
    valueView.setTextColor(Theme.colorText);
    valueView.setSize(112, 20);
    tile.addView(valueView);

    parent.addView(tile);
    tileRoots[idx] = tile;
    tileValues[idx] = valueView;
  }

  private void registerSensors() {
    if (sensorManager == null) {
      sensorManager = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
    }
    int[] types = {
      Sensor.TYPE_AMBIENT_TEMPERATURE,
      Sensor.TYPE_RELATIVE_HUMIDITY,
      Sensor.TYPE_PRESSURE,
      Sensor.TYPE_GAS_RESISTANCE,
      Sensor.TYPE_LIGHT,
    };
    for (int t : types) {
      Sensor s = sensorManager.getDefaultSensor(t);
      if (s != null) {
        sensorManager.registerListener(this, s, SensorManager.SENSOR_DELAY_NORMAL);
      }
    }
  }

  @Override
  public void onSensorChanged(SensorEvent event) {
    Formatter f = comp.formatter();
    float v = event.values[0];
    int type = event.sensor.getType();
    switch (type) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        tileValues[IDX_TEMP].setText(f.formatTemp(v));
        flashOnBreach(tileRoots[IDX_TEMP], comp.thresholds().tempBreached(v));
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        tileValues[IDX_HUM].setText(f.formatHumidity(v));
        flashOnBreach(tileRoots[IDX_HUM], comp.thresholds().humidityBreached(v));
        break;
      case Sensor.TYPE_PRESSURE:
        tileValues[IDX_PRESS].setText(f.formatPressure(v));
        break;
      case Sensor.TYPE_GAS_RESISTANCE:
        lastGas = v;
        tileValues[IDX_IAQ].setText(f.formatGasIaq(v));
        break;
      case Sensor.TYPE_LIGHT:
        tileValues[IDX_LIGHT].setText(f.formatLux(v));
        flashOnBreach(tileRoots[IDX_LIGHT], comp.thresholds().luxBreached(v));
        break;
      default:
        break;
    }
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}

  private void flashOnBreach(LinearLayout tile, boolean breached) {
    if (!breached) {
      return;
    }
    // ViewPropertyAnimator has no completion listener; the alpha pulse self-restores.
    tile.animate().alpha(1f, 0.35f).setDuration(180).start();
    tile.animate().alpha(0.35f, 1f).setDuration(360).start();
  }

  @Override
  public boolean onKey(View v, KeyEvent event) {
    if (event.getAction() != KeyEvent.ACTION_UP) {
      return false;
    }
    int code = event.getKeyCode();
    if (code == KeyEvent.KEYCODE_DPAD_UP) {
      startActivity(new Intent(SettingsActivity.class));
      return true;
    }
    if (code == KeyEvent.KEYCODE_DPAD_DOWN) {
      startActivity(new Intent(HistoryActivity.class));
      return true;
    }
    if (code == KeyEvent.KEYCODE_DPAD_CENTER) {
      toggleService();
      return true;
    }
    return false;
  }

  // OnGestureListener — implemented directly on HomeActivity, replacing the anonymous inner class
  // that previously captured `this`. Saves one class load + one instance.
  @Override
  public void onSingleTap(MotionEvent e) {}

  @Override
  public void onLongPress(MotionEvent e) {
    Formatter f = comp.formatter();
    f.toggleUnits();
    Log.i(EnvAppComponent.TAG, "units toggled fahrenheit=" + f.isFahrenheit());
    Toast.makeText(this, f.isFahrenheit() ? "°F" : "°C", Toast.LENGTH_SHORT).show();
  }

  @Override
  public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) {}

  private void toggleService() {
    Intent svc = new Intent(SensorLoggerService.class);
    if (serviceRunning) {
      stopService(svc);
      serviceRunning = false;
      Toast.makeText(this, "Logger stopped", Toast.LENGTH_SHORT).show();
    } else {
      startService(svc);
      serviceRunning = true;
      Toast.makeText(this, "Logger started", Toast.LENGTH_SHORT).show();
    }
  }
}
