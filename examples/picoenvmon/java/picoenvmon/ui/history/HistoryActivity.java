// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.history;

import javax.inject.Inject;
import picodroid.app.AlertDialog;
import picodroid.content.ComponentName;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;
import picoenvmon.EnvApp;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.Formatter;
import picoenvmon.util.TimeFormat;

/**
 * Temperature history (reached from the Home hub). Binds {@link SensorLoggerService}, snapshots its
 * temperature ring buffer, and renders one focusable {@link ListView} row per sample. Under the
 * standardized model A/B move the row highlight, X opens an info dialog for the highlighted sample
 * (BACK/Y dismisses it), and Y returns to the hub. The info dialog is now an explicit on-demand
 * action rather than firing unconditionally on connect.
 */
public class HistoryActivity extends NavActivity implements ServiceConnection {

  @Inject Formatter formatter;
  private ListView list;
  private TextView statusLine;
  private final float[] samples = new float[SensorLoggerService.RING_CAPACITY];
  private final int[] sampleTs = new int[SensorLoggerService.RING_CAPACITY];
  private int sampleCount;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(EnvApp.TAG, "History.onCreate");
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
    list.setOnItemClickListener((parent, view, position, id) -> showSampleDialog(position));
    // Weighted: the list grows with the panel and the hint bar stays on the bottom edge.
    root.addView(
        list, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

    installHintBar(root, "A:Up  B:Down  X:Info  Y:Back");

    setContentView(root);

    bindService(new Intent(SensorLoggerService.class), this, BIND_AUTO_CREATE);
  }

  @Override
  public void onDestroy() {
    try {
      unbindService(this);
    } catch (Throwable t) {
      Log.i(EnvApp.TAG, "History unbind ignored: " + t);
    }
  }

  @Override
  public void onServiceConnected(ComponentName name, IBinder binder) {
    SensorLoggerService svc = ((SensorLoggerService.LocalBinder) binder).service;
    sampleCount = svc.snapshot(SensorLoggerService.IDX_TEMPERATURE, samples, sampleTs);
    Log.i(EnvApp.TAG, "History bound, samples=" + sampleCount);

    // Every sample is a row: the list keeps only the rows in view alive and re-binds them as it
    // scrolls, so the whole ring costs what a screenful did. Rows are labelled with their ring
    // index.
    if (sampleCount == 0) {
      // The ring fills only while the SensorLoggerService runs, and it survives
      // screen changes only as a started/foreground service, so point the user at
      // the Logger toggle in Live. Kept short to fit the status line width.
      statusLine.setText("No data - enable Logger");
    } else {
      statusLine.setText(sampleCount + " samples");
    }

    Formatter f = formatter;
    ArrayAdapter<String> adapter = new ArrayAdapter<String>();
    for (int i = 0; i < sampleCount; i++) {
      // "HH:MM 21.3C" once the sample carries an NTP-anchored stamp; the
      // pre-sync "[i]" ring-index form otherwise.
      String label =
          sampleTs[i] > 0
              ? TimeFormat.hm(sampleTs[i] * 1000L) + " " + f.formatTemp(samples[i])
              : "[" + i + "] " + f.formatTemp(samples[i]);
      adapter.add(label);
    }
    list.setAdapter(adapter);
    // Oldest first, as the ring is; open on the newest sample.
    list.setSelection(sampleCount - 1);
    Log.i(EnvApp.TAG, "History rows " + list.getChildCount() + " of " + sampleCount);
  }

  @Override
  public void onServiceDisconnected(ComponentName name) {
    Log.i(EnvApp.TAG, "History service disconnected");
  }

  private void showSampleDialog(int position) {
    int idx = position;
    if (idx < 0 || idx >= sampleCount) {
      return;
    }
    Formatter f = formatter;
    String when = sampleTs[idx] > 0 ? "\nTime: " + TimeFormat.dateTime(sampleTs[idx] * 1000L) : "";
    new AlertDialog.Builder()
        .setTitle("Sample " + idx)
        .setMessage("Temperature: " + f.formatTemp(samples[idx]) + when)
        .setPositiveButton("OK", (dialog, which) -> {})
        .show();
  }
}
