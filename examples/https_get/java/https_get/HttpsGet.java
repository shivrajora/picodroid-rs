// SPDX-License-Identifier: GPL-3.0-only
package https_get;

import java.io.IOException;
import javax.net.ssl.SSLHandshakeException;
import picodroid.app.Application;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.NetworkInfo;
import picodroid.net.URL;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * HTTPS demo and the nightly's TLS row: the wall clock from SNTP, then a GET against the test
 * host's TLS listener, three handshakes with that listener that must be refused (an expired leaf, a
 * chain under no known root, a certificate for another name), and two public hosts whose chains end
 * at different roots (an ECDSA chain under GTS Root R4, an RSA chain under ISRG Root X1).
 *
 * <p>Sim testing:
 *
 * <pre>
 *   # Terminal 1 — the test TLS listener (scripts/net-lib.sh starts the same one for the nightly).
 *   ./scripts/tls-listener.sh
 *   # Terminal 2
 *   ./scripts/sim.sh --app https_get --board pico_display2_w
 * </pre>
 */
public class HttpsGet extends Application {
  private static final String TAG = "HttpsGet";

  /** How long to wait for the network before giving up (WiFi join + DHCP). */
  private static final int NETWORK_WAIT_MS = 30000;

  /** The test host's TLS listener; the host is baked at build time (default loopback). */
  private static final String LOCAL = "https://" + NetTestConfig.HOST;

  @Override
  public void onCreate() {
    Log.i(TAG, "--- picodroid https demo ---");

    int waited = 0;
    while (!NetworkInfo.isConnected() && waited < NETWORK_WAIT_MS) {
      SystemClock.sleep(500);
      waited += 500;
    }
    if (!NetworkInfo.isConnected()) {
      Log.i(TAG, "No network. Aborting.");
      return;
    }

    get(LOCAL + ":8443/", "local");
    // The listener's other ports must be refused in the handshake
    // (scripts/tls-listener.py): a leaf past its validity, a chain under a
    // CA the build does not trust, a certificate for another name.
    get(LOCAL + ":8444/", "expired");
    get(LOCAL + ":8445/", "untrusted");
    get(LOCAL + ":8446/", "wrong-name");
    get("https://api.anthropic.com/", "ecdsa/gts");
    get("https://api.open-meteo.com/", "rsa/isrg");
    Log.i(TAG, "Done.");
  }

  private void get(String url, String label) {
    Log.i(TAG, "GET " + url + " (" + label + ")");
    HttpURLConnection c = new URL(url).openConnection();
    c.setConnectTimeout(10000);
    c.setReadTimeout(10000);
    long t0 = SystemClock.elapsedRealtime();
    try {
      c.connect();
      long handshake = SystemClock.elapsedRealtime() - t0;
      int code = c.getResponseCode();
      Log.i(TAG, "  status=" + code + " handshake=" + handshake + "ms");
      HttpInputStream in = c.getInputStream();
      byte[] buf = new byte[256];
      int total = 0;
      int n;
      while ((n = in.read(buf)) > 0) {
        total += n;
      }
      Log.i(TAG, "  read " + total + " body bytes");
    } catch (SSLHandshakeException e) {
      Log.i(TAG, "  [" + label + "] handshake rejected: " + e.getMessage());
    } catch (IOException e) {
      Log.i(TAG, "  " + label + " failed: " + e.getMessage());
    } finally {
      c.disconnect();
    }
  }
}
