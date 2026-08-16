// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import java.io.IOException;
import picodroid.net.DatagramPacket;
import picodroid.net.DatagramSocket;
import picodroid.net.InetAddress;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picoenvmon.di.EnvAppComponent;

/**
 * Minimal SNTP (RFC 4330) client. Android hides its SntpClient as internal API, so this lives in
 * app code. One 48-byte exchange: mode 3 (client) request, read the server's transmit timestamp
 * (seconds since 1900 at offset 40), convert to Unix epoch millis, and anchor the wall clock via
 * {@link SystemClock#setCurrentTimeMillis}. No round-trip-delay compensation — display accuracy on
 * a sensor monitor doesn't need sub-100ms truth.
 */
public final class SntpClient {
  private static final String TAG = EnvAppComponent.TAG;
  private static final String NTP_HOST = "pool.ntp.org";
  private static final int NTP_PORT = 123;
  private static final int PACKET_BYTES = 48;
  private static final int TIMEOUT_MS = 3000;
  private static final int ATTEMPTS = 3;

  /** Seconds between the NTP era (1900-01-01) and the Unix epoch (1970-01-01). */
  private static final long SECONDS_1900_TO_1970 = 2208988800L;

  private SntpClient() {}

  /**
   * Resolve, exchange, and anchor the clock. Returns true on success. Fail-soft: every failure is
   * caught and logged — callers retry from the housekeeping tick.
   */
  public static boolean sync() {
    DatagramSocket socket = null;
    try {
      int server = InetAddress.getByName(NTP_HOST).getRawAddress();
      socket = new DatagramSocket(0);
      socket.setTimeout(TIMEOUT_MS);
      for (int attempt = 1; attempt <= ATTEMPTS; attempt++) {
        long epochMs = exchange(socket, server);
        if (epochMs > 0) {
          SystemClock.setCurrentTimeMillis(epochMs);
          Log.i(TAG, "ntp: synced, epoch=" + epochMs);
          return true;
        }
      }
      Log.i(TAG, "ntp: no valid reply after " + ATTEMPTS + " attempts");
      return false;
    } catch (IOException e) {
      Log.i(TAG, "ntp: sync failed: " + e.getMessage());
      return false;
    } catch (RuntimeException e) {
      Log.i(TAG, "ntp: unexpected: " + e);
      return false;
    } finally {
      if (socket != null) {
        socket.close();
      }
    }
  }

  /** One request/reply. Returns epoch millis, or 0 on timeout/garbage. */
  private static long exchange(DatagramSocket socket, int server) {
    try {
      byte[] buf = new byte[PACKET_BYTES];
      // LI=0, VN=4, Mode=3 (client).
      buf[0] = 0x23;
      socket.send(new DatagramPacket(buf, PACKET_BYTES, server, NTP_PORT));

      DatagramPacket reply = new DatagramPacket(buf, PACKET_BYTES);
      socket.receive(reply);
      if (reply.getLength() < PACKET_BYTES) {
        return 0;
      }
      // Transmit timestamp: 32-bit unsigned seconds at offset 40, then the
      // 32-bit fraction — take the top byte for ~4 ms granularity.
      long ntpSeconds =
          ((buf[40] & 0xFFL) << 24)
              | ((buf[41] & 0xFFL) << 16)
              | ((buf[42] & 0xFFL) << 8)
              | (buf[43] & 0xFFL);
      if (ntpSeconds == 0) {
        return 0;
      }
      long fractionMs = ((buf[44] & 0xFFL) * 1000L) >> 8;
      return (ntpSeconds - SECONDS_1900_TO_1970) * 1000L + fractionMs;
    } catch (IOException e) {
      Log.i(TAG, "ntp: attempt failed: " + e.getMessage());
      return 0;
    }
  }
}
