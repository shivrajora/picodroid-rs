// SPDX-License-Identifier: GPL-3.0-only
package netexception;

import java.io.IOException;
import java.net.ConnectException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import picodroid.app.Application;
import picodroid.net.InetAddress;
import picodroid.net.ServerSocket;
import picodroid.net.Socket;
import picodroid.util.Log;

/**
 * Deterministic assertions for the typed network-exception taxonomy
 * (docs/designs/net-typed-exceptions.md). Every case is fully local — a loopback refusal
 * (kernel-deterministic RST), a listener accept timeout, and a dotted-quad resolve — so the test
 * needs no external server, network, or resolver.
 */
public class NetException extends Application {
  private static final String TAG = "NetException";

  @Override
  public void onCreate() {
    Log.i(TAG, "--- typed network exceptions ---");
    boolean okConnect = connectRefused();
    boolean okAccept = acceptTimeout();
    boolean okRecv = recvTimeout();
    boolean okResolve = resolveLiteral();
    if (okConnect && okAccept && okRecv && okResolve) {
      Log.i(TAG, "ALL PASS");
    } else {
      Log.i(TAG, "FAILURES — see above");
    }
  }

  /** Connecting to a loopback port with no listener must throw ConnectException. */
  private boolean connectRefused() {
    Socket sock = new Socket();
    try {
      // Port 1 requires privileges to bind — nothing listens there.
      sock.connect(InetAddress.getByAddress(127, 0, 0, 1).getRawAddress(), 1);
      Log.i(TAG, "FAIL connect-refused: no exception");
      return false;
    } catch (ConnectException e) {
      Log.i(TAG, "PASS connect-refused: " + e.getMessage());
      return true;
    } catch (IOException e) {
      Log.i(TAG, "FAIL connect-refused: wrong type, message=" + e.getMessage());
      return false;
    } finally {
      sock.close();
    }
  }

  /** accept() past setSoTimeout must throw SocketTimeoutException. */
  private boolean acceptTimeout() {
    ServerSocket server = null;
    try {
      server = new ServerSocket(0); // ephemeral port — nobody will connect
      server.setSoTimeout(200);
      server.accept();
      Log.i(TAG, "FAIL accept-timeout: no exception");
      return false;
    } catch (SocketTimeoutException e) {
      Log.i(TAG, "PASS accept-timeout: " + e.getMessage());
      return true;
    } catch (IOException e) {
      Log.i(TAG, "FAIL accept-timeout: wrong type, message=" + e.getMessage());
      return false;
    } finally {
      if (server != null) {
        server.close();
      }
    }
  }

  /**
   * recv() past setTimeout on an established (self-connected) connection must throw
   * SocketTimeoutException — never read as end-of-stream. Pins the recv-contract fix: the device
   * HAL's raw encoding has timeout and EOF inverted relative to the host's.
   */
  private boolean recvTimeout() {
    ServerSocket server = null;
    Socket client = new Socket();
    Socket accepted = null;
    try {
      // Fixed high port: the SDK has no getLocalPort, so an ephemeral
      // listener couldn't be connected back to.
      server = new ServerSocket(38207);
      client.connect(InetAddress.getByName("127.0.0.1").getRawAddress(), 38207);
      accepted = server.accept();
      client.setTimeout(200);
      byte[] buf = new byte[8];
      int n = client.recv(buf, 0, buf.length);
      Log.i(TAG, "FAIL recv-timeout: no exception (recv returned " + n + ")");
      return false;
    } catch (SocketTimeoutException e) {
      Log.i(TAG, "PASS recv-timeout: " + e.getMessage());
      return true;
    } catch (IOException e) {
      Log.i(TAG, "FAIL recv-timeout: wrong type, message=" + e.getMessage());
      return false;
    } finally {
      if (accepted != null) {
        accepted.close();
      }
      client.close();
      if (server != null) {
        server.close();
      }
    }
  }

  /** getByName on a dotted-quad literal resolves locally and round-trips. */
  private boolean resolveLiteral() {
    try {
      InetAddress addr = InetAddress.getByName("192.0.2.7");
      String s = addr.getHostAddress();
      if ("192.0.2.7".equals(s)) {
        Log.i(TAG, "PASS resolve-literal: " + s);
        return true;
      }
      Log.i(TAG, "FAIL resolve-literal: got " + s);
      return false;
    } catch (UnknownHostException e) {
      Log.i(TAG, "FAIL resolve-literal: " + e.getMessage());
      return false;
    }
  }
}
