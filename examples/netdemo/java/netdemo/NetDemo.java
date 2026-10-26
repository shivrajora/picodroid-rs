// SPDX-License-Identifier: GPL-3.0-only
package netdemo;

import java.io.IOException;
import picodroid.app.Application;
import picodroid.content.Context;
import picodroid.net.ConnectivityManager;
import picodroid.net.InetAddress;
import picodroid.net.Socket;
import picodroid.net.wifi.WifiManager;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Network demo — checks connectivity, connects to a TCP echo server, sends a message, and logs the
 * response.
 *
 * <p>Sim testing: Terminal 1: nc -l -p 7000 -c 'cat' (or: socat TCP-LISTEN:7000,fork EXEC:cat)
 * Terminal 2: ./scripts/sim.sh --app netdemo --board testbench_rp2350w
 *
 * <p>Hardware testing: Run an echo server on a machine reachable from the Pico 2 W's network and
 * bake its address in at build time — {@code PICODROID_NET_TEST_HOST=<ip> ./scripts/build-apk.sh
 * --app netdemo} (generated into {@link NetTestConfig#HOST}; default is loopback).
 */
public class NetDemo extends Application {
  private static final String TAG = "NetDemo";

  /** How long to wait for the network before giving up (WiFi join + DHCP). */
  private static final int NETWORK_WAIT_MS = 30000;

  @Override
  public void onCreate() {
    Log.i(TAG, "--- picodroid network demo ---");

    // On hardware the app starts before WiFi association and DHCP finish.
    // An app with an Activity registers a NetworkCallback and connects in
    // onAvailable; this one has no Activity, so no event loop delivers
    // callbacks to it, and it asks ConnectivityManager until a network is up.
    ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
    int waited = 0;
    while (cm.getActiveNetwork() == null && waited < NETWORK_WAIT_MS) {
      SystemClock.sleep(500);
      waited += 500;
    }
    boolean connected = cm.getActiveNetwork() != null;
    Log.i(TAG, "Network connected: " + connected + " (waited " + waited + " ms)");

    if (!connected) {
      Log.i(TAG, "Network not available.");
      return;
    }

    WifiManager wm = (WifiManager) getSystemService(Context.WIFI_SERVICE);
    int rawIp = wm.getConnectionInfo().getIpAddress();
    InetAddress myAddr = new InetAddress(rawIp);
    Log.i(TAG, "My IP: " + myAddr.getHostAddress());

    // Connect to a TCP echo server on the build-time test host (see the
    // class javadoc), port 7000.
    int port = 7000;
    Socket sock = new Socket();
    try {
      InetAddress server = InetAddress.getByName(NetTestConfig.HOST);
      Log.i(TAG, "Connecting to " + server.getHostAddress() + ":" + port);
      sock.connect(server.getRawAddress(), port);
      sock.setTimeout(5000);

      // Send a message.
      byte[] msg = new byte[] {'H', 'e', 'l', 'l', 'o'};
      int sent = sock.send(msg, 0, msg.length);
      Log.i(TAG, "Sent " + sent + " bytes");

      // Receive the echo.
      byte[] buf = new byte[64];
      int n = sock.recv(buf, 0, buf.length);
      Log.i(TAG, "Received " + n + " bytes");

      // Log received bytes as characters.
      for (int i = 0; i < n; i++) {
        Log.i(TAG, "  [" + i + "] = " + (char) buf[i]);
      }

      Log.i(TAG, "Done.");
    } catch (IOException e) {
      // Expected when no echo server is listening at the address above —
      // see the class javadoc for how to run one.
      Log.i(TAG, "Echo connection failed: " + e.getMessage());
      Log.i(TAG, "(is an echo server running and reachable? see NetDemo javadoc)");
    } finally {
      sock.close();
    }
  }
}
