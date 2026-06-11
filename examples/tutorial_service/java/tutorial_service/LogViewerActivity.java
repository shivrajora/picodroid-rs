// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Activity;
import picodroid.app.IBinder;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;

/**
 * Binds {@link UptimeLogService}, reads a snapshot of its ring buffer, and lists the samples. The
 * Activity itself {@code implements ServiceConnection}: it binds in {@link #onCreate} and unbinds
 * in {@link #onDestroy}. Because the Service is also <em>started</em> from the Application, the
 * same running instance is reused on every visit — so the snapshot grows over time instead of
 * resetting.
 *
 * <p>BACK (the default {@link #onBackPressed} → {@code finish()}) pops this screen back to Home;
 * finishing auto-unbinds this connection, and the still-started Service keeps sampling.
 */
public class LogViewerActivity extends Activity implements ServiceConnection {
  private static final String TAG = "LogViewerActivity";

  // Field-held so the GC roots these listener-bearing views through this Activity.
  private ListView list;
  private TextView statusLine;
  private Button refreshButton;

  // The bound Service handle, captured in onServiceConnected. null until connected and after
  // disconnect; Refresh guards on it.
  private UptimeLogService service;

  private final long[] samples = new long[UptimeLogService.CAPACITY];

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Uptime Log");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    statusLine = new TextView();
    // ASCII "..." — the bundled font has no ellipsis (U+2026) glyph.
    statusLine.setText("Connecting...");
    statusLine.setTextColor(Color.CYAN);
    root.addView(statusLine);

    list = new ListView();
    list.setSize(200, 130);
    root.addView(list);

    refreshButton = new Button("Refresh");
    refreshButton.setSize(200, 36);
    refreshButton.setOnClickListener(v -> refresh());
    root.addView(refreshButton);

    setContentView(root);

    // Bind in onCreate (the proven picoenvmon pattern). onServiceConnected then delivers the
    // binder between frames, and the framework auto-unbinds this connection when the Activity
    // finishes — but we still unbind explicitly in onDestroy below.
    Log.i(TAG, "bindService");
    bindService(new Intent(UptimeLogService.class), this);
  }

  @Override
  public void onResume() {
    Log.i(TAG, "onResume");
  }

  @Override
  public void onPause() {
    Log.i(TAG, "onPause");
  }

  @Override
  public void onDestroy() {
    Log.i(TAG, "onDestroy, unbindService");
    try {
      unbindService(this);
    } catch (Throwable t) {
      Log.i(TAG, "unbind ignored: " + t);
    }
  }

  @Override
  public void onServiceConnected(IBinder binder) {
    service = ((UptimeLogService.LocalBinder) binder).service;
    Log.i(TAG, "onServiceConnected");
    refresh();
  }

  @Override
  public void onServiceDisconnected() {
    Log.i(TAG, "onServiceDisconnected");
    service = null;
  }

  /** Re-read the Service's snapshot and rebuild the list. Wired to the Refresh button. */
  private void refresh() {
    if (service == null) {
      Log.i(TAG, "refresh skipped (not connected)");
      return;
    }
    int n = service.snapshot(samples);
    Log.i(TAG, "refresh, samples=" + n);

    if (n == 0) {
      statusLine.setText("No samples yet");
    } else {
      statusLine.setText(n + " samples (ms)");
    }

    ArrayAdapter<String> adapter = new ArrayAdapter<String>();
    for (int i = 0; i < n; i++) {
      adapter.add("[" + i + "] " + samples[i] + " ms");
    }
    list.setAdapter(adapter);
  }
}
