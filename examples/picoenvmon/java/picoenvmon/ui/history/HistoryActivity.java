// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.history;

import picodroid.app.Activity;
import picodroid.app.IBinder;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.widget.AlertDialog;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.util.Formatter;

/**
 * Bound-service consumer. Connects to {@link SensorLoggerService} on enter, snapshots its
 * temperature ring buffer, and renders one row per sample inside a ScrollView.
 */
public class HistoryActivity extends Activity implements ServiceConnection {

  private EnvActivityComponent comp;
  private LinearLayout list;
  private TextView statusLine;
  private boolean dialogShown;

  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "History.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(8, 6, 8, 6);
    root.setBackgroundColor(Theme.colorBackground);

    TextView title = new TextView();
    title.setText("Temp history");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    statusLine = new TextView();
    statusLine.setText("Connecting…");
    statusLine.setTextColor(Theme.colorTextSecondary);
    root.addView(statusLine);

    ScrollView scroll = new ScrollView();
    scroll.setSize(224, 188);

    GradientDrawable card = new GradientDrawable();
    card.setColor(Theme.colorSurface).setCornerRadius(6).setStroke(1, Theme.colorOutline);
    scroll.setBackground(card);

    list = new LinearLayout();
    list.setOrientation(LinearLayout.VERTICAL);
    list.setSize(224, 220);
    list.setPadding(6, 4, 6, 4);
    scroll.addView(list);
    root.addView(scroll);

    setContentView(root);

    bindService(new Intent(SensorLoggerService.class), this);
  }

  public void onDestroy() {
    try {
      unbindService(this);
    } catch (Throwable t) {
      Log.i(EnvAppComponent.TAG, "History unbind ignored: " + t);
    }
  }

  @Override
  public void onServiceConnected(IBinder binder) {
    SensorLoggerService svc = ((SensorLoggerService.LocalBinder) binder).service;
    float[] out = new float[SensorLoggerService.RING_CAPACITY];
    int n = svc.snapshot(SensorLoggerService.IDX_TEMPERATURE, out);
    Log.i(EnvAppComponent.TAG, "History bound, samples=" + n);
    statusLine.setText(n == 0 ? "No samples yet" : (n + " samples"));

    Formatter f = comp.formatter();
    for (int i = 0; i < n; i++) {
      TextView row = new TextView();
      row.setText("[" + i + "] " + f.formatTemp(out[i]));
      row.setTextColor(Theme.colorText);
      list.addView(row);
    }

    if (!dialogShown) {
      dialogShown = true;
      new AlertDialog.Builder()
          .setTitle("History")
          .setMessage("Showing last " + n + " temperature samples.")
          .setPositiveButton("OK", () -> Log.i(EnvAppComponent.TAG, "history dialog dismissed"))
          .show();
    }
  }

  @Override
  public void onServiceDisconnected() {
    Log.i(EnvAppComponent.TAG, "History service disconnected");
  }
}
