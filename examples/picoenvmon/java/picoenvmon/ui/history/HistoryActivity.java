// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.history;

import java.util.ArrayList;
import javax.inject.Inject;
import picodroid.app.AlertDialog;
import picodroid.content.ComponentName;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Theme;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;
import picoenvmon.EnvApp;
import picoenvmon.R;
import picoenvmon.service.SensorLoggerService;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.Formatter;
import picoenvmon.util.TimeFormat;

/**
 * Temperature history (reached from the Home hub). Binds {@link SensorLoggerService}, has {@link
 * HistoryViewModel} snapshot its temperature ring, and renders one focusable {@link ListView} row
 * per sample. Under the standardized model A/B move the row highlight, X opens an info dialog for
 * the highlighted sample (BACK/Y dismisses it), and Y returns to the hub.
 */
public class HistoryActivity extends NavActivity implements ServiceConnection {

  @Inject Formatter formatter;
  private HistoryViewModel model;
  private ListView list;
  private TextView statusLine;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(EnvApp.TAG, "History.onCreate");
    getDisplay();

    LinearLayout root = makeScreenRoot();
    addTitle(root, R.string.title_history);

    statusLine = new TextView(this);
    statusLine.setText(getString(R.string.history_connecting));
    statusLine.setTextColor(Theme.colorTextSecondary);
    root.addView(statusLine);

    list = new ListView(this);
    list.setOnItemClickListener((parent, view, position, id) -> showSampleDialog(position));
    // Weighted: the list grows with the panel and the hint bar stays on the bottom edge.
    root.addView(
        list, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));

    installHintBar(root, R.string.hint_history);

    setContentView(root);

    model = new ViewModelProvider(this).get(HistoryViewModel.class);
    model.history().observe(this, this::showHistory);

    bindService(new Intent(this, SensorLoggerService.class), this, BIND_AUTO_CREATE);
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
    model.load(((SensorLoggerService.LocalBinder) binder).service);
  }

  @Override
  public void onServiceDisconnected(ComponentName name) {
    Log.i(EnvApp.TAG, "History service disconnected");
  }

  private void showHistory(HistoryViewModel.History h) {
    int count = h.count;
    Log.i(EnvApp.TAG, "History bound, samples=" + count);
    // The ring fills only while the SensorLoggerService runs, and it survives screen changes
    // only as a started/foreground service, so an empty one points the user at the Logger toggle
    // in Live.
    statusLine.setText(
        count == 0 ? getString(R.string.history_empty) : getString(R.string.history_count, count));

    // Every sample is a row: the list keeps only the rows in view alive and re-binds them as it
    // scrolls, so the whole ring costs what a screenful did.
    Formatter f = formatter;
    ArrayList<String> rows = new ArrayList<>(count);
    for (int i = 0; i < count; i++) {
      String temp = f.formatTemp(h.samples[i]);
      int ts = h.timestamps[i];
      rows.add(
          ts > 0
              ? getString(R.string.history_row_time, TimeFormat.hm(ts * 1000L), temp)
              : getString(R.string.history_row_index, i, temp));
    }
    list.setAdapter(new ArrayAdapter<>(this, picodroid.R.layout.simple_list_item_1, rows));
    // Oldest first, as the ring is; open on the newest sample.
    list.setSelection(count - 1);
    Log.i(EnvApp.TAG, "History rows " + list.getChildCount() + " of " + count);
  }

  private void showSampleDialog(int position) {
    HistoryViewModel.History h = model.history().getValue();
    if (h == null || position < 0 || position >= h.count) {
      return;
    }
    String temp = formatter.formatTemp(h.samples[position]);
    int ts = h.timestamps[position];
    String message =
        ts > 0
            ? getString(R.string.sample_message_time, temp, TimeFormat.dateTime(ts * 1000L))
            : getString(R.string.sample_message, temp);
    new AlertDialog.Builder(this)
        .setTitle(getString(R.string.sample_title, position))
        .setMessage(message)
        .setPositiveButton(getString(R.string.ok), (dialog, which) -> {})
        .show();
  }
}
