// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.history;

import picodroid.app.IBinder;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.util.Log;
import picodroid.widget.AlertDialog;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.Formatter;

/**
 * Temperature history (reached from the Home hub). Binds {@link SensorLoggerService}, snapshots its
 * temperature ring buffer, and renders one focusable {@link ListView} row per sample. Under the
 * standardized model A/B move the row highlight, X opens an info dialog for the highlighted sample
 * (BACK/Y dismisses it), and Y returns to the hub. The info dialog is now an explicit on-demand
 * action rather than firing unconditionally on connect.
 */
public class HistoryActivity extends NavActivity implements ServiceConnection {

  /**
   * Max focusable rows rendered. Each {@code lv_list} button row consumes the board's small (48 KB)
   * LVGL render pool, and the full ring (60) leaves too little headroom for the draw tasks needed
   * to render them — so we show only the most recent window. Comfortably within the safe bound on
   * this board; raising it risks a render-pool stall.
   */
  private static final int MAX_ROWS = 12;

  private EnvActivityComponent comp;
  private ListView list;
  private TextView statusLine;
  private final float[] samples = new float[SensorLoggerService.RING_CAPACITY];
  private int sampleCount;
  private int firstShown;

  @Override
  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "History.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = makeScreenRoot();

    TextView title = new TextView();
    title.setText("Temp history");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    statusLine = new TextView();
    // ASCII "..." — the bundled font has no ellipsis (U+2026) glyph.
    statusLine.setText("Connecting...");
    statusLine.setTextColor(Theme.colorTextSecondary);
    root.addView(statusLine);

    list = new ListView();
    list.setSize(224, 170);
    list.setOnItemClickListener((parent, view, position, id) -> showSampleDialog(position));
    root.addView(list);

    installHintBar(root, "A:Up  B:Down  X:Info  Y:Back");

    setContentView(root);

    bindService(new Intent(SensorLoggerService.class), this);
  }

  @Override
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
    sampleCount = svc.snapshot(SensorLoggerService.IDX_TEMPERATURE, samples);
    Log.i(EnvAppComponent.TAG, "History bound, samples=" + sampleCount);

    // Render the most-recent window (see MAX_ROWS). Rows are labelled with their real ring index.
    firstShown = sampleCount > MAX_ROWS ? sampleCount - MAX_ROWS : 0;
    if (sampleCount == 0) {
      // The ring fills only while the SensorLoggerService runs, and it survives
      // screen changes only as a started/foreground service, so point the user at
      // the Logger toggle in Live. Kept short to fit the status line width.
      statusLine.setText("No data - enable Logger");
    } else if (firstShown > 0) {
      statusLine.setText(sampleCount + " samples (recent " + MAX_ROWS + ")");
    } else {
      statusLine.setText(sampleCount + " samples");
    }

    Formatter f = comp.formatter();
    ArrayAdapter<String> adapter = new ArrayAdapter<String>();
    for (int i = firstShown; i < sampleCount; i++) {
      adapter.add("[" + i + "] " + f.formatTemp(samples[i]));
    }
    list.setAdapter(adapter);
  }

  @Override
  public void onServiceDisconnected() {
    Log.i(EnvAppComponent.TAG, "History service disconnected");
  }

  private void showSampleDialog(int position) {
    // position is the row index within the rendered window; map back to the real sample index.
    int idx = firstShown + position;
    if (idx < 0 || idx >= sampleCount) {
      return;
    }
    Formatter f = comp.formatter();
    new AlertDialog.Builder()
        .setTitle("Sample " + idx)
        .setMessage("Temperature: " + f.formatTemp(samples[idx]))
        .setPositiveButton("OK", (dialog, which) -> {})
        .show();
  }
}
