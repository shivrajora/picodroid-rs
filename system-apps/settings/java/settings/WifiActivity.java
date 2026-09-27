// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.List;
import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.net.NetworkInfo;
import picodroid.net.wifi.ScanResult;
import picodroid.net.wifi.SupplicantState;
import picodroid.net.wifi.WifiConfiguration;
import picodroid.net.wifi.WifiInfo;
import picodroid.net.wifi.WifiManager;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * Wi-Fi (docs/designs/wifi-provisioning-2026-09.md): the connection status, the saved network, a
 * Scan row, then one row per network the last scan found, strongest first, with signal bars and
 * whether it needs a password. A tap on an open network joins it; on a secured one it opens
 * {@link WifiPasswordActivity}. The saved network's row opens a dialog: Connect, Forget, Cancel —
 * Forget is refused for a network compiled into the firmware, which the row marks "Build".
 *
 * <p>The join's outcome is read, not pushed (picodroid has no broadcasts): while the screen shows,
 * a main-thread {@link ScheduledExecutorService} re-reads {@link WifiManager#getConnectionInfo}
 * twice a second and updates the status row — Connecting, Obtaining IP address, Connected, Wrong
 * password, Not found. Every change is logged as {@code wifi status …}; a completed join as {@code
 * wifi connected <ssid>}, which the bench keys on. Scan completion is pushed through a {@link
 * WifiManager.ScanResultsCallback} and rebuilds the rows.
 *
 * <p>Row 0 is the header, 1 the status, 2 the saved network (or "No saved network"), 3 Scan, and
 * the networks follow from 4, so the bench taps the first network at y = 180.
 */
public class WifiActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;
  private static final long POLL_MS = 500;

  private Column column;
  private WifiManager wm;
  /** Held so the rows stay reachable while their click listeners are live. */
  private View[] rows;
  private TextView statusTail;
  private List<ScanResult> results;
  private List<WifiConfiguration> saved;
  private boolean scanning;
  private String lastStatus = "";

  private final ScheduledExecutorService poller = Executors.newSingleThreadScheduledExecutor();
  private ScheduledFuture<?> poll;

  private final WifiManager.ScanResultsCallback onScan =
      new WifiManager.ScanResultsCallback() {
        @Override
        public void onScanResultsAvailable() {
          scanning = false;
          Log.i(TAG, "wifi scan done");
          render();
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    wm = (WifiManager) getSystemService(Context.WIFI_SERVICE);
    wm.registerScanResultsCallback(Executors.mainExecutor(), onScan);
    render();
  }

  /** The screen from scratch: a new column, then the rows one per tick. */
  private void render() {
    if (column != null) {
      column.stop();
    }
    column = new Column(this, "< Wi-Fi", v -> finish());
    column.fill(() -> prepare(), i -> row(i), () -> done());
  }

  private void prepare() {
    results = wm.getScanResults();
    saved = wm.getConfiguredNetworks();
    rows = new View[3 + results.size()];
  }

  private View row(int i) {
    switch (i) {
      case 0:
        statusTail = Screens.tail(this, statusText());
        rows[0] = Screens.info(this, "Status", statusTail);
        return rows[0];
      case 1:
        if (saved.isEmpty()) {
          rows[1] = Screens.info(this, "No saved network");
        } else {
          final WifiConfiguration c = saved.get(0);
          final String ssid = unquote(c.SSID);
          String source = c.status == WifiConfiguration.Status.CURRENT ? "Build" : "Saved";
          rows[1] = Screens.row(this, ssid, source, v -> savedDialog(c, ssid));
        }
        return rows[1];
      case 2:
        rows[2] = Screens.row(this, scanning ? "Scanning..." : "Scan for networks", v -> scan());
        return rows[2];
      default:
        int n = i - 3;
        if (n >= results.size()) {
          return null;
        }
        final ScanResult r = results.get(n);
        rows[i] = Screens.row(this, r.SSID, signal(r), v -> pick(r));
        // One line per network for the log: the bench finds its row by it.
        Log.i(TAG, "wifi net " + n + " " + r.SSID + " " + r.level + (r.isSecured() ? " *" : " open"));
        return rows[i];
    }
  }

  /** The rows are in: focus Scan, say how many networks — the harness keys on this line. */
  private void done() {
    rows[2].requestFocus();
    Log.i(TAG, "wifi " + results.size() + " networks");
  }

  /** Bars for the signal level, then whether a password is needed. */
  private static String signal(ScanResult r) {
    int level = WifiManager.calculateSignalLevel(r.level, 5);
    String bars = "";
    for (int i = 0; i < 4; i++) {
      bars += i < level ? "|" : ".";
    }
    return bars + (r.isSecured() ? "  *" : "  open");
  }

  private void scan() {
    if (wm.startScan()) {
      scanning = true;
      Log.i(TAG, "wifi scan");
      render();
    } else {
      Log.i(TAG, "wifi scan refused");
    }
  }

  private void pick(ScanResult r) {
    Log.i(TAG, "wifi pick " + r.SSID);
    if (r.isSecured()) {
      startActivity(new Intent(WifiPasswordActivity.class).putExtra("ssid", r.SSID));
    } else {
      connect(wm, r.SSID, "");
    }
  }

  /** Save {@code ssid} as the device's network and join it now. Logged; the bench keys on it. */
  static void connect(WifiManager wm, String ssid, String password) {
    WifiConfiguration c = new WifiConfiguration();
    c.SSID = "\"" + ssid + "\"";
    c.preSharedKey = password.length() == 0 ? null : "\"" + password + "\"";
    int id = wm.addNetwork(c);
    if (id < 0) {
      Log.i(TAG, "wifi save failed " + ssid);
      return;
    }
    Log.i(TAG, "wifi connect " + ssid);
    if (!wm.enableNetwork(id, true)) {
      Log.i(TAG, "wifi connect refused " + ssid);
    }
  }

  private void savedDialog(final WifiConfiguration c, final String ssid) {
    boolean build = c.status == WifiConfiguration.Status.CURRENT;
    AlertDialog.Builder b =
        new AlertDialog.Builder(this)
            .setTitle(ssid)
            .setMessage(build ? "Set at build time." : "Saved network.")
            .setPositiveButton("Connect", (dialog, which) -> reconnect(c))
            .setNegativeButton("Cancel", null);
    if (!build) {
      b.setNeutralButton("Forget", (dialog, which) -> forget(c));
    }
    b.show();
  }

  private void reconnect(WifiConfiguration c) {
    Log.i(TAG, "wifi connect " + unquote(c.SSID));
    wm.enableNetwork(c.networkId, true);
  }

  private void forget(WifiConfiguration c) {
    Log.i(TAG, wm.removeNetwork(c.networkId) ? "wifi forgot" : "wifi forget refused");
    // Rebuild once the dialog has closed, not from inside its click.
    Executors.mainExecutor().execute(() -> render());
  }

  /** What the status row says now. */
  private String statusText() {
    WifiInfo info = wm.getConnectionInfo();
    String ssid = unquote(info.getSSID());
    SupplicantState state = info.getSupplicantState();
    if (state == SupplicantState.COMPLETED) {
      return NetworkInfo.isConnected() ? "Connected: " + ssid : "Obtaining IP address...";
    }
    if (state == SupplicantState.ASSOCIATING) {
      return "Connecting: " + ssid;
    }
    switch (wm.getLastError()) {
      case WifiManager.ERROR_AUTHENTICATING:
        return "Wrong password: " + ssid;
      case WifiManager.ERROR_NETWORK_NOT_FOUND:
        return "Not found: " + ssid;
      case WifiManager.ERROR_GENERIC:
        return "Failed: " + ssid;
      default:
        return "Not connected";
    }
  }

  /** Re-read the status; on a change, update the row and say so. */
  private void refreshStatus() {
    String text = statusText();
    if (text.equals(lastStatus)) {
      return;
    }
    lastStatus = text;
    if (statusTail != null) {
      statusTail.setText(text);
    }
    Log.i(TAG, "wifi status " + text);
    if (text.startsWith("Connected: ")) {
      Log.i(TAG, "wifi connected " + text.substring("Connected: ".length()));
    }
  }

  static String unquote(String s) {
    int n = s.length();
    if (n >= 2 && s.charAt(0) == '"' && s.charAt(n - 1) == '"') {
      return s.substring(1, n - 1);
    }
    return s;
  }

  @Override
  public void onResume() {
    super.onResume();
    lastStatus = "";
    refreshStatus();
    poll = poller.scheduleWithFixedDelay(() -> refreshStatus(), POLL_MS, POLL_MS, TimeUnit.MILLISECONDS);
  }

  @Override
  public void onPause() {
    if (poll != null) {
      poll.cancel(false);
      poll = null;
    }
    super.onPause();
  }

  @Override
  public void onDestroy() {
    wm.unregisterScanResultsCallback(onScan);
    poller.shutdown();
    column.stop();
    super.onDestroy();
  }
}
