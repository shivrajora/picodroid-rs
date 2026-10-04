// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import java.io.IOException;
import java.net.SocketTimeoutException;
import picodroid.net.DatagramPacket;
import picodroid.net.DatagramSocket;
import picodroid.net.InetAddress;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Finds the bridge on the LAN without being told its address. The display broadcasts {@value
 * #QUERY} to UDP port {@value #PORT}; the bridge answers {@code PICODROID-USAGE <http-port>}, and
 * the reply's source address is the PC. The same job Android would give {@code NsdManager}, cut
 * down to one datagram each way because the device has no multicast DNS.
 *
 * <p>Two probes, {@value #WAIT_MS} ms apart, then give up: a PC that is off cannot answer, and the
 * caller falls back to whatever address it has. Blocks the calling thread for up to about three
 * seconds, so never call it on the main thread.
 */
final class BridgeDiscovery {
  /** The port the bridge listens on for probes: the HTTP port plus one. */
  static final int PORT = 8788;

  private static final String QUERY = "PICODROID-USAGE?";
  private static final String ANSWER = "PICODROID-USAGE ";
  private static final int ATTEMPTS = 2;
  private static final int WAIT_MS = 1500;

  /** 255.255.255.255: every host on the LAN. */
  private static final byte[] BROADCAST = {(byte) 255, (byte) 255, (byte) 255, (byte) 255};

  private BridgeDiscovery() {}

  /** {@code host:port} of the first bridge to answer, or null when none did. */
  static String find() {
    byte[] query = QUERY.getBytes();
    byte[] buf = new byte[64];
    DatagramSocket s = null;
    try {
      s = new DatagramSocket();
      s.setBroadcast(true);
      s.setSoTimeout(WAIT_MS);
      InetAddress everyone = InetAddress.getByAddress(BROADCAST);
      DatagramPacket probe = new DatagramPacket(query, query.length, everyone, PORT);
      for (int attempt = 0; attempt < ATTEMPTS; attempt++) {
        s.send(probe);
        long deadline = SystemClock.elapsedRealtime() + WAIT_MS;
        // Anything else that arrives on the port is somebody else's datagram; keep listening.
        while (SystemClock.elapsedRealtime() < deadline) {
          DatagramPacket in = new DatagramPacket(buf, buf.length);
          try {
            s.receive(in);
          } catch (SocketTimeoutException e) {
            break;
          }
          String found = parse(in);
          if (found != null) {
            Log.i(UsageService.TAG, "discovery: found " + found);
            return found;
          }
        }
      }
      Log.i(UsageService.TAG, "discovery: no reply");
    } catch (IOException e) {
      Log.i(UsageService.TAG, "discovery: failed: " + e.getMessage());
    } finally {
      if (s != null) {
        s.close();
      }
    }
    return null;
  }

  /** {@code host:port} from a well-formed answer, else null. */
  private static String parse(DatagramPacket in) {
    String text = new String(in.getData(), 0, in.getLength());
    if (!text.startsWith(ANSWER)) {
      return null;
    }
    String port = text.substring(ANSWER.length()).trim();
    if (port.length() == 0 || port.length() > 5) {
      return null;
    }
    for (int i = 0; i < port.length(); i++) {
      char c = port.charAt(i);
      if (c < '0' || c > '9') {
        return null;
      }
    }
    return in.getAddress().getHostAddress() + ":" + port;
  }
}
