// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.live;

import javax.inject.Inject;
import picodroid.content.ComponentName;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.GridLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picoenvmon.EnvApp;
import picoenvmon.R;
import picoenvmon.data.LatestReadings;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.ui.common.NavActivity;

/**
 * Live dashboard (reached from the Home hub). Observes {@link LiveViewModel}'s tiles, which follow
 * the 1 Hz smoothed readings the logging service publishes, and exposes a focusable Logger {@link
 * Switch} that toggles that foreground service — the single interactive control, so under the
 * standardized model X (ENTER) on the focused Switch toggles it and Y returns to the hub. The
 * service is bound for its logging state only. (The old touch-long-press °C↔°F toggle is gone;
 * units now live in Settings.)
 */
public class LiveActivity extends NavActivity implements ServiceConnection {

  private static final int[] TILE_LABELS = {
    R.string.tile_temperature,
    R.string.tile_humidity,
    R.string.tile_pressure,
    R.string.tile_air_quality,
    R.string.tile_light,
  };

  @Inject LiveViewModel.Factory viewModelFactory;
  private SensorLoggerService service;
  private boolean serviceRunning;
  private Switch loggerSwitch;

  private final View[] tileRoots = new View[LatestReadings.COUNT];
  private final TextView[] tileValues = new TextView[LatestReadings.COUNT];

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(EnvApp.TAG, "Live.onCreate");
    getDisplay();

    int gap = dimen(R.dimen.gap);
    LinearLayout root = makeScreenRoot();
    addTitle(root, R.string.menu_live);

    // One GradientDrawable shared across all 5 tile backgrounds (same color/radius/stroke).
    GradientDrawable tileBg = new GradientDrawable();
    tileBg.setColor(Theme.colorSurface);
    tileBg.setCornerRadius(6);
    tileBg.setStroke(1, Theme.colorOutline);

    // The five tiles as one two-column grid: the label column is as wide as its widest label,
    // the value column takes the rest, and the columns line up across the tiles without a
    // fixed width anywhere.
    GridLayout tiles = new GridLayout(this);
    tiles.setColumnCount(2);
    for (int i = 0; i < LatestReadings.COUNT; i++) {
      buildTile(tiles, i, getString(TILE_LABELS[i]), tileBg, gap);
    }
    root.addView(tiles, stacked(LinearLayout.LayoutParams.WRAP_CONTENT, gap));

    // Focusable Logger toggle (the old Home X=toggle-logger action). It is the only focusable
    // widget on the screen, so it auto-focuses; X toggles it.
    root.addView(buildLoggerRow(), stacked(dimen(R.dimen.switch_row_height), gap));

    installHintBar(root, R.string.hint_live, gap);

    setContentView(root);

    LiveViewModel model = new ViewModelProvider(this, viewModelFactory).get(LiveViewModel.class);
    model.tiles().observe(this, this::showTiles);

    bindService(new Intent(this, SensorLoggerService.class), this, BIND_AUTO_CREATE);
  }

  @Override
  public void onDestroy() {
    try {
      unbindService(this);
    } catch (Throwable t) {
      Log.i(EnvApp.TAG, "Live unbind ignored: " + t);
    }
  }

  @Override
  public void onServiceConnected(ComponentName name, IBinder binder) {
    service = ((SensorLoggerService.LocalBinder) binder).service;
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
  public void onServiceDisconnected(ComponentName name) {
    service = null;
  }

  private LinearLayout buildLoggerRow() {
    LinearLayout row = new LinearLayout(this);
    row.setOrientation(LinearLayout.HORIZONTAL);
    // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
    // headroom or it clips top/bottom: zero vertical padding plus the row's height give the full
    // circle room to render.
    row.setPadding(8, 0, 8, 0);

    TextView label = new TextView(this);
    label.setText(getString(R.string.logger));
    label.setTextColor(Theme.colorTextSecondary);
    // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
    // size (android:layout_weight="1"). WRAP_CONTENT height lets the row's cross-axis
    // centering center the glyphs — a fixed-height label draws its text top-aligned.
    row.addView(
        label, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

    Switch toggle = new Switch(this);
    toggle.setOnCheckedChangeListener((buttonView, isChecked) -> setLogger(isChecked));
    row.addView(toggle);
    loggerSwitch = toggle;

    return row;
  }

  private void setLogger(boolean on) {
    if (on == serviceRunning) {
      return;
    }
    Intent svc = new Intent(this, SensorLoggerService.class);
    if (on) {
      startService(svc);
    } else {
      stopService(svc);
    }
    serviceRunning = on;
    Log.i(EnvApp.TAG, "Logger " + (on ? "started" : "stopped"));
  }

  /**
   * One row of the grid: the card spans both columns behind a label in the first and a value in the
   * second, each centred on the row. Cells may overlap, as on Android; the card is added first so
   * it draws under the text. Rows after the first sit {@code gap} below the one above: the card and
   * its two labels carry the same top margin, so the labels stay centred on the card.
   */
  private void buildTile(GridLayout grid, int idx, String label, GradientDrawable bg, int gap) {
    int top = idx == 0 ? 0 : gap;
    int inset = dimen(R.dimen.tile_text_inset);

    FrameLayout card = new FrameLayout(this);
    card.setBackground(bg);
    GridLayout.LayoutParams cardCell =
        new GridLayout.LayoutParams(GridLayout.spec(idx), GridLayout.spec(0, 2, GridLayout.FILL));
    cardCell.height = dimen(R.dimen.tile_height);
    cardCell.topMargin = top;
    grid.addView(card, cardCell);

    TextView labelView = new TextView(this);
    labelView.setText(label);
    labelView.setTextColor(Theme.colorTextSecondary);
    GridLayout.LayoutParams labelCell =
        new GridLayout.LayoutParams(
            GridLayout.spec(idx, GridLayout.CENTER), GridLayout.spec(0, GridLayout.LEFT));
    labelCell.setMargins(inset, top, 0, 0);
    grid.addView(labelView, labelCell);

    TextView valueView = new TextView(this);
    valueView.setText(getString(R.string.no_reading));
    valueView.setTextColor(Theme.colorText);
    GridLayout.LayoutParams valueCell =
        new GridLayout.LayoutParams(
            GridLayout.spec(idx, GridLayout.CENTER), GridLayout.spec(1, GridLayout.LEFT, 1f));
    // The column gap and the text's inset from the card's edge.
    valueCell.setMargins(gap + inset, top, inset, 0);
    grid.addView(valueView, valueCell);

    tileRoots[idx] = card;
    tileValues[idx] = valueView;
  }

  private void showTiles(LiveViewModel.Tiles t) {
    for (int i = 0; i < LatestReadings.COUNT; i++) {
      String v = t.values[i];
      if (v != null) {
        tileValues[i].setText(v);
      }
      if (t.breached[i]) {
        flashOnBreach(tileRoots[i]);
      }
    }
  }

  private void flashOnBreach(View tile) {
    // Dip, then restore once the dip has finished. Two back-to-back alpha()
    // starts on the same view would replace each other; chaining through
    // withEndAction is the Android idiom.
    tile.animate()
        .alpha(0.35f)
        .setDuration(180)
        .withEndAction(() -> tile.animate().alpha(1f).setDuration(360).start())
        .start();
  }
}
