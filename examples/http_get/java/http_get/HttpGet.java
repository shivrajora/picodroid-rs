// SPDX-License-Identifier: GPL-3.0-only
package http_get;

import picodroid.app.Application;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpOutputStream;
import picodroid.net.HttpUrlConnection;
import picodroid.net.NetworkInfo;
import picodroid.net.Url;
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

  @Override
  public void onCreate() {
    Log.i(TAG, "--- picodroid http demo ---");

    if (!NetworkInfo.isConnected()) {
      Log.i(TAG, "No network. Aborting.");
      return;
    }

    doGet();
    doPost();
  }

  private void doGet() {
    Log.i(TAG, "GET http://127.0.0.1:8000/");
    HttpUrlConnection c = new Url("http://127.0.0.1:8000/").openConnection();
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
    } finally {
      c.disconnect();
    }
  }

  private void doPost() {
    byte[] body = new byte[] {'h', 'e', 'l', 'l', 'o'};
    Log.i(TAG, "POST http://127.0.0.1:8000/ body=" + body.length + "B");
    HttpUrlConnection c = new Url("http://127.0.0.1:8000/").openConnection();
    try {
      c.setRequestMethod("POST");
      c.setDoOutput(true);
      c.setFixedLengthStreamingMode(body.length);
      c.connect();

      HttpOutputStream out = c.getOutputStream();
      out.write(body);

      int code = c.getResponseCode();
      Log.i(TAG, "  status=" + code);
    } finally {
      c.disconnect();
    }
  }
}
