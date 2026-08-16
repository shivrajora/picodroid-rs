// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.network;

import picodroid.graphics.Theme;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.net.NetworkManager;
import picoenvmon.net.WeatherFetcher;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.TimeFormat;

/**
 * Network status screen (reached from the Home hub). On WiFi-less boards it degrades to a single
 * "not available" message — the hub entry is shown everywhere so the feature is discoverable, and
 * this screen is where the {@code FEATURE_WIFI} probe's answer becomes visible.
 *
 * <p>Callbacks arrive via {@link NetworkManager.Listener} on the main executor, so all widget
 * mutation here happens on the main thread.
 *
 * <p>Sim caveat: the simulator's {@code NetworkInfo.getIpAddress()} is hardcoded to 127.0.0.1, so
 * the URL line reads {@code http://127.0.0.1:8080/} even though the server binds 0.0.0.0 and is
 * LAN-reachable via the host's real address.
 */
public class NetworkActivity extends NavActivity implements NetworkManager.Listener {

  private NetworkManager net;
  private TextView statusLine;
  private TextView ipLine;
  private TextView urlLine;
  private TextView timeLine;
  private TextView weatherLine;

  @Override
  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Network.onCreate");
    net = ((EnvAppComponent) EnvAppComponent.current()).networkManager();
    getDisplay();

    LinearLayout root = makeScreenRoot();

    TextView title = new TextView();
    title.setText("Network");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    if (net.state() == NetworkManager.STATE_NO_WIFI) {
      TextView msg = new TextView();
      msg.setText("WiFi not available on this board");
      msg.setTextColor(Theme.colorTextSecondary);
      root.addView(msg);
      installHintBar(root, "Y:Back");
      setContentView(root);
      return;
    }

    statusLine = new TextView();
    statusLine.setTextColor(Theme.colorText);
    root.addView(statusLine);

    ipLine = new TextView();
    ipLine.setTextColor(Theme.colorText);
    root.addView(ipLine);

    urlLine = new TextView();
    urlLine.setTextColor(Theme.colorText);
    root.addView(urlLine);

    timeLine = new TextView();
    timeLine.setTextColor(Theme.colorText);
    root.addView(timeLine);

    weatherLine = new TextView();
    weatherLine.setTextColor(Theme.colorTextSecondary);
    root.addView(weatherLine);

    // The screen's one focusable widget — auto-focused, X activates.
    Button refresh = new Button("Refresh");
    refresh.setOnClickListener(v -> net.requestRefresh());
    root.addView(refresh);

    installHintBar(root, "X:Refresh  Y:Back");
    setContentView(root);

    net.addListener(this);
    repaint();
  }

  @Override
  public void onDestroy() {
    if (net != null) {
      net.removeListener(this);
    }
  }

  @Override
  public void onNetworkChanged() {
    if (statusLine != null) {
      repaint();
    }
  }

  private void repaint() {
    switch (net.state()) {
      case NetworkManager.STATE_JOINING:
        statusLine.setText("WiFi: joining...");
        break;
      case NetworkManager.STATE_UP:
        statusLine.setText("WiFi: connected");
        break;
      case NetworkManager.STATE_FAILED:
        statusLine.setText("WiFi: not connected (retrying)");
        break;
      default:
        statusLine.setText("WiFi: unavailable");
        break;
    }
    String ip = net.ipAddress();
    ipLine.setText(ip != null ? "IP: " + ip : "IP: -");
    String url = net.url();
    urlLine.setText(url != null ? url : "");
    timeLine.setText(
        net.isTimeSynced()
            ? "Time: " + TimeFormat.hms(System.currentTimeMillis()) + " UTC"
            : "Time: not synced");
    String w = net.weather();
    weatherLine.setText(WeatherFetcher.CITY + ": " + (w != null ? w : "unavailable"));
  }
}
