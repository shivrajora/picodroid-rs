// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Activity;
import picodroid.content.ComponentName;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.view.ViewGroup;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;

/**
 * Binds {@link UptimeLogService}, reads a snapshot of its ring buffer, and lists the samples. The
 * Activity itself {@code implements ServiceConnection}: it binds in {@link #onCreate} and unbinds
 * in {@link #onDestroy}. Because the Service is also <em>started</em> (by {@link HomeActivity}),
 * the same running instance is reused on every visit — so the snapshot grows over time instead of
 * resetting.
 *
 * <p>BACK (the default {@link #onBackPressed} → {@code finish()}) pops this screen back to Home;
 * the still-started Service keeps sampling after this connection is unbound.
 */
public class LogViewerActivity extends Activity implements ServiceConnection {
  private static final String TAG = "LogViewerActivity";

  // The adapter and the status line are fields because Refresh rewrites them; the list and the
  // button are locals — the tree keeps them and the listener alive, as on Android.
  private ArrayAdapter<String> adapter;
  private TextView statusLine;

  // The bound Service handle, captured in onServiceConnected. null until connected and after
  // disconnect; Refresh guards on it.
  private UptimeLogService service;

  private final long[] samples = new long[UptimeLogService.CAPACITY];

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.log_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    statusLine = new TextView(this);
    statusLine.setText(getString(R.string.log_connecting));
    statusLine.setTextColor(Color.CYAN);
    root.addView(statusLine);

    // The list takes whatever height the title, status and button leave (weight 1).
    adapter = new ArrayAdapter<>(this, picodroid.R.layout.simple_list_item_1);
    ListView list = new ListView(this);
    list.setAdapter(adapter);
    root.addView(list, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

    Button refreshButton = new Button(this);
    refreshButton.setText(getString(R.string.button_refresh));
    refreshButton.setOnClickListener(v -> refresh());
    root.addView(refreshButton, HomeActivity.buttonParams());

    setContentView(root);

    // onServiceConnected delivers the binder between frames; onDestroy unbinds.
    Log.i(TAG, "bindService");
    bindService(new Intent(this, UptimeLogService.class), this, BIND_AUTO_CREATE);
  }

  @Override
  public void onResume() {
    super.onResume();
    Log.i(TAG, "onResume");
  }

  @Override
  public void onPause() {
    super.onPause();
    Log.i(TAG, "onPause");
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
    Log.i(TAG, "onDestroy, unbindService");
    unbindService(this);
  }

  @Override
  public void onServiceConnected(ComponentName name, IBinder binder) {
    service = ((UptimeLogService.LocalBinder) binder).getService();
    Log.i(TAG, "onServiceConnected");
    refresh();
  }

  @Override
  public void onServiceDisconnected(ComponentName name) {
    Log.i(TAG, "onServiceDisconnected");
    service = null;
  }

  /** Re-read the Service's snapshot and refill the list. Wired to the Refresh button. */
  private void refresh() {
    if (service == null) {
      Log.i(TAG, "refresh skipped (not connected)");
      return;
    }
    int n = service.snapshot(samples);
    Log.i(TAG, "refresh, samples=" + n);

    if (n == 0) {
      statusLine.setText(getString(R.string.log_empty));
    } else {
      statusLine.setText(getString(R.string.log_count, n));
    }

    adapter.clear();
    for (int i = 0; i < n; i++) {
      adapter.add(getString(R.string.log_row, i, samples[i]));
    }
    adapter.notifyDataSetChanged();
  }
}
