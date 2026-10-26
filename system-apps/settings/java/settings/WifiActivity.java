// SPDX-License-Identifier: GPL-3.0-only
package settings;

import java.util.List;
import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.IntentFilter;
import picodroid.net.ConnectivityManager;
import picodroid.net.Network;
import picodroid.net.wifi.ScanResult;
import picodroid.net.wifi.SupplicantState;
import picodroid.net.wifi.WifiConfiguration;
import picodroid.net.wifi.WifiInfo;
import picodroid.net.wifi.WifiManager;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Wi-Fi (docs/designs/wifi-provisioning-2026-09.md): the connection status, the saved network, a
 * Scan row, then one row per network the last scan found, strongest first, with signal bars and
 * whether it needs a password. A tap on an open network joins it; on a secured one it opens {@link
 * WifiPasswordActivity}. The saved network's row opens a dialog: Connect, Forget, Cancel — Forget
 * is refused for a network compiled into the firmware, which the row marks "Build".
 *
 * <p>The join's outcome arrives as Android delivers it: while the screen is resumed a {@link
 * BroadcastReceiver} for {@link WifiManager#SUPPLICANT_STATE_CHANGED_ACTION} keeps the {@link
 * WifiManager#EXTRA_SUPPLICANT_ERROR} of a rejected join, and it and a {@link
 * ConnectivityManager.NetworkCallback} (the link up with an address, or gone) re-read {@link
 * WifiManager#getConnectionInfo} and update the status row — Connecting, Obtaining IP address,
 * Connected, Wrong password, Not found. Every change is logged as {@code wifi status …}; a
 * completed join as {@code wifi connected <ssid>}, which the bench keys on. Scan completion is
 * pushed through a {@link WifiManager.ScanResultsCallback} and rebuilds the rows.
 *
 * <p>Row 0 is the header, 1 the status, 2 the saved network (or "No saved network"), 3 Scan, and
 * the networks follow from 4, so the bench taps the first network at y = 180.
 */
public class WifiActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  private WifiManager wm;
  private ConnectivityManager cm;

  /** The Scan row: takes the focus once the list is in. */
  private View scanRow;

  private TextView statusTail;
  private List<ScanResult> results;
  private List<WifiConfiguration> saved;
  private boolean scanning;
  private String lastStatus = "";

  /**
   * Why the last join failed ({@link WifiManager#ERROR_AUTHENTICATING} and the rest), from the
   * latest supplicant broadcast; 0 while a join is under way or none failed.
   */
  private int lastError;

  /** The join's progress: the station's state, a broadcast with the failure's reason. */
  private final BroadcastReceiver onState =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          lastError = intent.getIntExtra(WifiManager.EXTRA_SUPPLICANT_ERROR, 0);
          refreshStatus();
        }
      };

  /** The link: up with an address ("Connected"), or gone. */
  private final ConnectivityManager.NetworkCallback onLink =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          refreshStatus();
        }

        @Override
        public void onLost(Network network) {
          refreshStatus();
        }
      };

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
    cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
    wm.registerScanResultsCallback(getMainExecutor(), onScan);
    render();
  }

  /** The screen from scratch: a new column and its rows, then the "wifi N networks" line. */
  private void render() {
    LinearLayout column = Screens.screen(this, "< Wi-Fi", v -> finish());
    results = wm.getScanResults();
    saved = wm.getConfiguredNetworks();
    statusTail = Screens.tailOf(Screens.info(this, column, "Status", statusText()));
    if (saved.isEmpty()) {
      Screens.info(this, column, "No saved network");
    } else {
      final WifiConfiguration c = saved.get(0);
      final String ssid = unquote(c.SSID);
      String source = c.status == WifiConfiguration.Status.CURRENT ? "Build" : "Saved";
      Screens.row(this, column, ssid, source, v -> savedDialog(c, ssid));
    }
    scanRow =
        Screens.row(this, column, scanning ? "Scanning..." : "Scan for networks", v -> scan());
    for (int n = 0; n < results.size(); n++) {
      final ScanResult r = results.get(n);
      Screens.row(this, column, r.SSID, signal(r), v -> pick(r));
      // One line per network for the log: the bench finds its row by it.
      Log.i(TAG, "wifi net " + n + " " + r.SSID + " " + r.level + (secured(r) ? " *" : " open"));
    }
    done();
  }

  /** The rows are in: focus Scan, say how many networks — the harness keys on this line. */
  private void done() {
    scanRow.requestFocus();
    Log.i(TAG, "wifi " + results.size() + " networks");
  }

  /** Bars for the signal level, then whether a password is needed. */
  private static String signal(ScanResult r) {
    int level = WifiManager.calculateSignalLevel(r.level, 5);
    String bars = "";
    for (int i = 0; i < 4; i++) {
      bars += i < level ? "|" : ".";
    }
    return bars + (secured(r) ? "  *" : "  open");
  }

  /** Whether joining needs a password: Android's capabilities test (an open network is [ESS]). */
  private static boolean secured(ScanResult r) {
    return r.capabilities.contains("WPA") || r.capabilities.contains("WEP");
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
    if (secured(r)) {
      startActivity(new Intent(this, WifiPasswordActivity.class).putExtra("ssid", r.SSID));
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
    getMainExecutor().execute(() -> render());
  }

  /** What the status row says now. */
  private String statusText() {
    WifiInfo info = wm.getConnectionInfo();
    String ssid = unquote(info.getSSID());
    SupplicantState state = info.getSupplicantState();
    if (state == SupplicantState.COMPLETED) {
      return cm.getActiveNetwork() != null ? "Connected: " + ssid : "Obtaining IP address...";
    }
    if (state == SupplicantState.ASSOCIATING) {
      return "Connecting: " + ssid;
    }
    switch (lastError) {
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
    registerReceiver(onState, new IntentFilter(WifiManager.SUPPLICANT_STATE_CHANGED_ACTION));
    cm.registerDefaultNetworkCallback(onLink);
    lastStatus = "";
    refreshStatus();
  }

  @Override
  public void onPause() {
    unregisterReceiver(onState);
    cm.unregisterNetworkCallback(onLink);
    super.onPause();
  }

  @Override
  public void onDestroy() {
    wm.unregisterScanResultsCallback(onScan);
    super.onDestroy();
  }
}
