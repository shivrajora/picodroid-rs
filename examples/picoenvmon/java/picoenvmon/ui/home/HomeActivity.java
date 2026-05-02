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
 * Live dashboard. Tile fade-in on entry, sensor listeners feeding 5 tiles, button-key navigation
 * (A=Settings, B=History, X=toggle Service, Y=back), long-press toggles °C↔°F.
 */
public class HomeActivity extends Activity implements SensorEventListener, OnKeyListener {

  private static class Tile {
    LinearLayout root;
    TextView label;
    TextView value;
    GradientDrawable bg;
    int defaultColor;
  }

  private EnvActivityComponent comp;
  private SensorManager sensorManager;
  private boolean serviceRunning;

  private Tile tempTile;
  private Tile humTile;
  private Tile pressTile;
  private Tile iaqTile;
  private Tile lightTile;

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

    tempTile = buildTile("Temp", Theme.colorSurface);
    humTile = buildTile("Humidity", Theme.colorSurface);
    pressTile = buildTile("Pressure", Theme.colorSurface);
    iaqTile = buildTile("Air quality", Theme.colorSurface);
    lightTile = buildTile("Light", Theme.colorSurface);

    root.addView(tempTile.root);
    root.addView(humTile.root);
    root.addView(pressTile.root);
    root.addView(iaqTile.root);
    root.addView(lightTile.root);

    TextView footer = new TextView();
    footer.setText("A:Settings B:History X:Logger");
    footer.setTextColor(Theme.colorTextSecondary);
    root.addView(footer);

    setContentView(root);
    root.setOnKeyListener(this);

    GestureDetector gd =
        new GestureDetector(
            new GestureDetector.OnGestureListener() {
              public void onSingleTap(MotionEvent e) {}

              public void onLongPress(MotionEvent e) {
                Formatter f = comp.formatter();
                f.toggleUnits();
                Log.i(EnvAppComponent.TAG, "units toggled fahrenheit=" + f.isFahrenheit());
                Toast.makeText(f.isFahrenheit() ? "°F" : "°C", Toast.LENGTH_SHORT).show();
              }

              public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) {}
            });
    root.setOnTouchListener(gd);

    animateIn();
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

  private void animateIn() {
    Tile[] tiles = {tempTile, humTile, pressTile, iaqTile, lightTile};
    for (int i = 0; i < tiles.length; i++) {
      View v = tiles[i].root;
      v.setAlpha(0f);
      v.animate().alpha(0f, 1f).setDuration(220 + i * 60).start();
    }
  }

  private Tile buildTile(String label, int bgColor) {
    Tile t = new Tile();
    t.defaultColor = bgColor;
    t.root = new LinearLayout();
    t.root.setOrientation(LinearLayout.HORIZONTAL);
    t.root.setSize(224, 28);
    t.root.setPadding(8, 4, 8, 4);

    t.bg = new GradientDrawable();
    t.bg.setColor(bgColor).setCornerRadius(6).setStroke(1, Theme.colorOutline);
    t.root.setBackground(t.bg);

    t.label = new TextView();
    t.label.setText(label);
    t.label.setTextColor(Theme.colorTextSecondary);
    t.label.setSize(96, 20);
    t.root.addView(t.label);

    t.value = new TextView();
    t.value.setText("—");
    t.value.setTextColor(Theme.colorText);
    t.value.setSize(112, 20);
    t.root.addView(t.value);
    return t;
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
        tempTile.value.setText(f.formatTemp(v));
        flashOnBreach(tempTile, comp.thresholds().tempBreached(v));
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        humTile.value.setText(f.formatHumidity(v));
        flashOnBreach(humTile, comp.thresholds().humidityBreached(v));
        break;
      case Sensor.TYPE_PRESSURE:
        pressTile.value.setText(f.formatPressure(v));
        break;
      case Sensor.TYPE_GAS_RESISTANCE:
        lastGas = v;
        iaqTile.value.setText(f.formatGasIaq(v));
        break;
      case Sensor.TYPE_LIGHT:
        lightTile.value.setText(f.formatLux(v));
        flashOnBreach(lightTile, comp.thresholds().luxBreached(v));
        break;
      default:
        break;
    }
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}

  private void flashOnBreach(Tile t, boolean breached) {
    if (!breached) {
      return;
    }
    // ViewPropertyAnimator has no completion listener; the alpha pulse self-restores.
    t.root.animate().alpha(1f, 0.35f).setDuration(180).start();
    t.root.animate().alpha(0.35f, 1f).setDuration(360).start();
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

  private void toggleService() {
    Intent svc = new Intent(SensorLoggerService.class);
    if (serviceRunning) {
      stopService(svc);
      serviceRunning = false;
      Toast.makeText("Logger stopped", Toast.LENGTH_SHORT).show();
    } else {
      startService(svc);
      serviceRunning = true;
      Toast.makeText("Logger started", Toast.LENGTH_SHORT).show();
    }
  }
}
