// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.live;

import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.hardware.Sensor;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.service.SmoothedSensorListener;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.Formatter;

/**
 * Live dashboard (reached from the Home hub). Binds {@link SensorLoggerService} for 1 Hz smoothed
 * sensor callbacks that feed 5 tiles, and exposes a focusable Logger {@link Switch} that toggles
 * the foreground logging service — the single interactive control, so under the standardized model
 * X (ENTER) on the focused Switch toggles it and Y returns to the hub. (The old touch-long-press
 * °C↔°F toggle is gone; units now live in Settings.)
 */
public class LiveActivity extends NavActivity implements ServiceConnection, SmoothedSensorListener {

  private static final int IDX_TEMP = 0;
  private static final int IDX_HUM = 1;
  private static final int IDX_PRESS = 2;
  private static final int IDX_IAQ = 3;
  private static final int IDX_LIGHT = 4;
  private static final int NUM_TILES = 5;

  private EnvActivityComponent comp;
  private SensorLoggerService service;
  private boolean serviceRunning;
  private Switch loggerSwitch;

  private final LinearLayout[] tileRoots = new LinearLayout[NUM_TILES];
  private final TextView[] tileValues = new TextView[NUM_TILES];

  @Override
  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Live.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = makeScreenRoot();
    root.setSpacing(4);

    TextView title = new TextView();
    title.setText("Live");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    // One GradientDrawable shared across all 5 tile backgrounds (same color/radius/stroke).
    GradientDrawable tileBg = new GradientDrawable();
    tileBg.setColor(Theme.colorSurface).setCornerRadius(6).setStroke(1, Theme.colorOutline);

    buildTile(root, IDX_TEMP, "Temp", tileBg);
    buildTile(root, IDX_HUM, "Humidity", tileBg);
    buildTile(root, IDX_PRESS, "Pressure", tileBg);
    buildTile(root, IDX_IAQ, "Air quality", tileBg);
    buildTile(root, IDX_LIGHT, "Light", tileBg);

    // Focusable Logger toggle (the old Home X=toggle-logger action). It is the only group-def
    // widget
    // on the screen, so it auto-focuses; X toggles it.
    root.addView(buildLoggerRow());

    installHintBar(root, "A/B:Move  X:Toggle  Y:Back");

    setContentView(root);

    bindService(new Intent(SensorLoggerService.class), this);
  }

  @Override
  public void onDestroy() {
    if (service != null) {
      service.removeSmoothedListener(this);
    }
    try {
      unbindService(this);
    } catch (Throwable t) {
      Log.i(EnvAppComponent.TAG, "Live unbind ignored: " + t);
    }
  }

  @Override
  public void onServiceConnected(IBinder binder) {
    service = ((SensorLoggerService.LocalBinder) binder).service;
    service.addSmoothedListener(this);
    // The bound Service is the source of truth for the logging state (Android LocalBinder pattern).
    // A *started* logging Service outlives the unbind we do in onDestroy, so re-entering the Live
    // screen reconnects to a service that may already be logging while the freshly built Switch
    // still shows its default (off). Reflect the real state onto the toggle here.
    //
    // Update the model field *before* the widget: android.widget.CompoundButton#setChecked fires
    // OnCheckedChangeListener, so setting serviceRunning first makes setLogger's
    // `if (on == serviceRunning)` guard absorb the re-entrant callback — no spurious start/stop.
    boolean logging = service.isLogging();
    serviceRunning = logging;
    loggerSwitch.setChecked(logging);
  }

  @Override
  public void onServiceDisconnected() {
    service = null;
  }

  private LinearLayout buildLoggerRow() {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
    // headroom or it clips top/bottom. Two things steal that headroom: the row's 2 px theme
    // border and its padding. A borderless background (stroke 0) zeroes the border, and zero
    // vertical padding plus a 34 px height give the full circle room to render.
    row.setSize(224, 34);
    row.setPadding(8, 0, 8, 0);
    row.setBackground(new GradientDrawable().setColor(Theme.colorBackground));

    TextView label = new TextView();
    label.setText("Logger");
    label.setTextColor(Theme.colorTextSecondary);
    // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
    // size (mirrors android:layout_weight="1"), so the switch is no longer clipped.
    // WRAP_CONTENT height lets the row's flex cross-axis centering center the glyphs —
    // a fixed-height label draws its text top-aligned inside its own box.
    row.addView(
        label, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

    Switch toggle = new Switch();
    toggle.setOnCheckedChangeListener((buttonView, isChecked) -> setLogger(isChecked));
    row.addView(toggle);
    loggerSwitch = toggle;

    return row;
  }

  private void setLogger(boolean on) {
    if (on == serviceRunning) {
      return;
    }
    Intent svc = new Intent(SensorLoggerService.class);
    if (on) {
      startService(svc);
    } else {
      stopService(svc);
    }
    serviceRunning = on;
    Log.i(EnvAppComponent.TAG, "Logger " + (on ? "started" : "stopped"));
  }

  private void buildTile(LinearLayout parent, int idx, String label, GradientDrawable bg) {
    LinearLayout tile = new LinearLayout();
    tile.setOrientation(LinearLayout.HORIZONTAL);
    tile.setSize(224, 26);
    tile.setPadding(8, 4, 8, 4);
    tile.setBackground(bg);

    // Fixed widths keep the label/value columns aligned across tiles; WRAP_CONTENT
    // heights let the tile's flex cross-axis centering actually center the glyphs
    // (a fixed-height label draws its text top-aligned inside its own box).
    TextView labelView = new TextView();
    labelView.setText(label);
    labelView.setTextColor(Theme.colorTextSecondary);
    labelView.setSize(96, View.WRAP_CONTENT);
    tile.addView(labelView);

    TextView valueView = new TextView();
    // ASCII "--" placeholder: the bundled LVGL Montserrat subset has no em-dash
    // (U+2014) glyph, so "—" renders as a missing-glyph box.
    valueView.setText("--");
    valueView.setTextColor(Theme.colorText);
    valueView.setSize(112, View.WRAP_CONTENT);
    tile.addView(valueView);

    parent.addView(tile);
    tileRoots[idx] = tile;
    tileValues[idx] = valueView;
  }

  @Override
  public void onSmoothedSensor(int sensorType, float value) {
    Formatter f = comp.formatter();
    switch (sensorType) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        tileValues[IDX_TEMP].setText(f.formatTemp(value));
        flashOnBreach(tileRoots[IDX_TEMP], comp.thresholds().tempBreached(value));
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        tileValues[IDX_HUM].setText(f.formatHumidity(value));
        flashOnBreach(tileRoots[IDX_HUM], comp.thresholds().humidityBreached(value));
        break;
      case Sensor.TYPE_PRESSURE:
        tileValues[IDX_PRESS].setText(f.formatPressure(value));
        break;
      case Sensor.TYPE_GAS_RESISTANCE:
        tileValues[IDX_IAQ].setText(f.formatGasIaq(value));
        break;
      case Sensor.TYPE_LIGHT:
        tileValues[IDX_LIGHT].setText(f.formatLux(value));
        flashOnBreach(tileRoots[IDX_LIGHT], comp.thresholds().luxBreached(value));
        break;
      default:
        break;
    }
  }

  private void flashOnBreach(LinearLayout tile, boolean breached) {
    if (!breached) {
      return;
    }
    // ViewPropertyAnimator has no completion listener; the alpha pulse self-restores.
    tile.animate().alpha(1f, 0.35f).setDuration(180).start();
    tile.animate().alpha(0.35f, 1f).setDuration(360).start();
  }
}
