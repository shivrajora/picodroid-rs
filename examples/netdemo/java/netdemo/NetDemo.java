package netdemo;

import picodroid.app.Application;
import picodroid.net.InetAddress;
import picodroid.net.NetworkInfo;
import picodroid.net.Socket;
import picodroid.util.Log;

/**
 * Network demo — checks connectivity, connects to a TCP echo server, sends a message, and logs the
 * response.
 *
 * <p>Sim testing: Terminal 1: nc -l -p 7000 -c 'cat' (or: socat TCP-LISTEN:7000,fork EXEC:cat)
 * Terminal 2: ./scripts/sim.sh --app netdemo --board testbench_rp2350w
 *
 * <p>Hardware testing: Run an echo server on a machine reachable from the Pico 2 W's network, then
 * adjust the server address below.
 */
public class NetDemo extends Application {
  private static final String TAG = "NetDemo";

  public void onCreate() {
    Log.i(TAG, "--- picodroid network demo ---");

    // Check network status.
    boolean connected = NetworkInfo.isConnected();
    Log.i(TAG, "Network connected: " + connected);

    if (!connected) {
      Log.i(TAG, "Network not available.");
      return;
    }

    int rawIp = NetworkInfo.getIpAddress();
    InetAddress myAddr = new InetAddress(rawIp);
    Log.i(TAG, "My IP: " + myAddr.getHostAddress());

    // Connect to a TCP echo server on localhost:7000.
    InetAddress server = InetAddress.getByAddress(127, 0, 0, 1);
    int port = 7000;
    Log.i(TAG, "Connecting to " + server.getHostAddress() + ":" + port);

    Socket sock = new Socket();
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

    sock.close();
    Log.i(TAG, "Done.");
  }
}
