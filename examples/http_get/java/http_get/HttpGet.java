// SPDX-License-Identifier: GPL-3.0-only
package http_get;

import java.io.IOException;
import picodroid.app.Application;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpOutputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.NetworkInfo;
import picodroid.net.URL;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * HTTP GET + POST demo.
 *
 * <p>Sim testing:
 *
 * <pre>
 *   # Terminal 1 — a tiny HTTP server that echoes GET/POST bodies.
 *   python3 -m http.server 8000
 *   # Terminal 2
 *   ./scripts/sim.sh --app http_get --board testbench_rp2350w
 * </pre>
 */
public class HttpGet extends Application {
  private static final String TAG = "HttpGet";

  /** How long to wait for the network before giving up (WiFi join + DHCP). */
  private static final int NETWORK_WAIT_MS = 30000;

  /**
   * Server base URL. The host is baked at build time (default loopback) — for hardware testing,
   * {@code PICODROID_NET_TEST_HOST=<ip> ./scripts/build-apk.sh --app http_get}.
   */
  private static final String BASE_URL = "http://" + NetTestConfig.HOST + ":8000/";

  @Override
  public void onCreate() {
    Log.i(TAG, "--- picodroid http demo ---");

    // On hardware the app starts before WiFi association and DHCP finish;
    // poll until the network is up instead of checking once.
    int waited = 0;
    while (!NetworkInfo.isConnected() && waited < NETWORK_WAIT_MS) {
      SystemClock.sleep(500);
      waited += 500;
    }
    if (!NetworkInfo.isConnected()) {
      Log.i(TAG, "No network. Aborting.");
      return;
    }

    doGet();
    doPost();
  }

  private void doGet() {
    Log.i(TAG, "GET " + BASE_URL);
    HttpURLConnection c = new URL(BASE_URL).openConnection();
    try {
      c.connect();
      int code = c.getResponseCode();
      Log.i(TAG, "  status=" + code + " content-length=" + c.getContentLength());

      HttpInputStream in = c.getInputStream();
      byte[] buf = new byte[128];
      int total = 0;
      int n;
      while ((n = in.read(buf)) > 0) {
        total += n;
      }
      Log.i(TAG, "  read " + total + " body bytes");
    } catch (IOException e) {
      // Expected when no HTTP server is listening at BASE_URL — see the
      // class javadoc for how to run one.
      Log.i(TAG, "  GET failed: " + e.getMessage());
    } finally {
      c.disconnect();
    }
  }

  private void doPost() {
    byte[] body = new byte[] {'h', 'e', 'l', 'l', 'o'};
    Log.i(TAG, "POST " + BASE_URL + " body=" + body.length + "B");
    HttpURLConnection c = new URL(BASE_URL).openConnection();
    try {
      c.setRequestMethod("POST");
      c.setDoOutput(true);
      c.setFixedLengthStreamingMode(body.length);
      c.connect();

      HttpOutputStream out = c.getOutputStream();
      out.write(body);

      int code = c.getResponseCode();
      Log.i(TAG, "  status=" + code);
    } catch (IOException e) {
      Log.i(TAG, "  POST failed: " + e.getMessage());
    } finally {
      c.disconnect();
    }
  }
}
