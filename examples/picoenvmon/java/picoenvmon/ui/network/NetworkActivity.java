// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.network;

import javax.inject.Inject;
import picodroid.graphics.Theme;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picoenvmon.EnvApp;
import picoenvmon.R;
import picoenvmon.net.NetworkManager;
import picoenvmon.net.NetworkStatus;
import picoenvmon.net.WeatherFetcher;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.TimeFormat;

/**
 * Network status screen (reached from the Home hub). On boards without a network link it degrades
 * to a single "not available" message — the hub entry is shown everywhere so the feature is
 * discoverable, and this screen is where the {@code FEATURE_WIFI} / {@code FEATURE_ETHERNET}
 * probe's answer becomes visible.
 *
 * <p>Observes {@link NetworkManager#status()} with this Activity as the lifecycle owner: every
 * change arrives on the main thread while the screen is started, and the observer goes with the
 * screen.
 */
public class NetworkActivity extends NavActivity {

  @Inject NetworkManager net;
  private TextView statusLine;
  private TextView ipLine;
  private TextView urlLine;
  private TextView timeLine;
  private TextView weatherLine;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(EnvApp.TAG, "Network.onCreate");
    getDisplay();

    LinearLayout root = makeScreenRoot();
    addTitle(root, R.string.menu_network);

    NetworkStatus now = net.status().getValue();
    if (now == null || now.state == NetworkManager.STATE_NO_WIFI) {
      TextView msg = new TextView(this);
      msg.setText(getString(R.string.net_not_available));
      msg.setTextColor(Theme.colorTextSecondary);
      root.addView(msg);
      installHintBar(root, R.string.hint_back);
      setContentView(root);
      return;
    }

    statusLine = addLine(root, Theme.colorText);
    ipLine = addLine(root, Theme.colorText);
    urlLine = addLine(root, Theme.colorText);
    timeLine = addLine(root, Theme.colorText);
    weatherLine = addLine(root, Theme.colorTextSecondary);

    // The screen's one focusable widget — auto-focused, X activates.
    Button refresh = new Button(this);
    refresh.setText(getString(R.string.refresh));
    refresh.setOnClickListener(v -> net.requestRefresh());
    root.addView(refresh);

    installHintBar(root, R.string.hint_network);
    setContentView(root);

    net.status().observe(this, this::show);
  }

  private TextView addLine(LinearLayout root, int color) {
    TextView line = new TextView(this);
    line.setTextColor(color);
    root.addView(line);
    return line;
  }

  private void show(NetworkStatus s) {
    switch (s.state) {
      case NetworkManager.STATE_JOINING:
        statusLine.setText(getString(R.string.net_joining));
        break;
      case NetworkManager.STATE_UP:
        statusLine.setText(getString(R.string.net_connected));
        break;
      case NetworkManager.STATE_FAILED:
        statusLine.setText(getString(R.string.net_failed));
        break;
      default:
        statusLine.setText(getString(R.string.net_down));
        break;
    }
    ipLine.setText(
        s.ipAddress != null
            ? getString(R.string.net_ip, s.ipAddress)
            : getString(R.string.net_ip_none));
    urlLine.setText(s.url != null ? s.url : "");
    timeLine.setText(
        s.timeSynced
            ? getString(R.string.net_time, TimeFormat.hms(System.currentTimeMillis()))
            : getString(R.string.net_time_unsynced));
    weatherLine.setText(
        getString(
            R.string.net_weather,
            WeatherFetcher.CITY,
            s.weather != null ? s.weather : getString(R.string.weather_unavailable)));
  }
}
