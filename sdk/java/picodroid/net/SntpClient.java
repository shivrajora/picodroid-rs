// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import picodroid.os.SystemClock;

/**
 * A simple SNTP (RFC 4330) client, the shape of Android's {@code android.net.SntpClient}: one
 * request to an NTP server yields the server's time and the monotonic reference it was read
 * against. Nothing here touches the system clock; a caller anchors it, as Android's network time
 * service does:
 *
 * <pre>{@code
 * SntpClient client = new SntpClient();
 * if (client.requestTime("pool.ntp.org", 3000)) {
 *   long now = client.getNtpTime() + SystemClock.elapsedRealtime() - client.getNtpTimeReference();
 *   SystemClock.setCurrentTimeMillis(now);
 * }
 * }</pre>
 *
 * <p>The wall clock counts from boot until it is anchored, so an app that verifies TLS
 * certificates, stamps records or shows a clock does this once the network is up. Boards have no
 * battery-backed clock; the anchor is lost at reset.
 */
public class SntpClient {
  private static final int NTP_PORT = 123;
  private static final int PACKET_BYTES = 48;
  private static final int MODE_CLIENT = 3;
  private static final int VERSION = 4;
  private static final int TRANSMIT_TIME_OFFSET = 40;

  /** Seconds between the NTP era (1900-01-01) and the Unix epoch (1970-01-01). */
  private static final long SECONDS_1900_TO_1970 = 2208988800L;

  private long ntpTime;
  private long ntpTimeReference;
  private long roundTripTime;

  /**
   * Send one request to {@code host} and wait up to {@code timeoutMs} for the reply. On success the
   * getters carry the result; on any failure (resolution, timeout, a malformed or zero reply) they
   * are unchanged and the method returns false.
   */
  public boolean requestTime(String host, int timeoutMs) {
    DatagramSocket socket = null;
    try {
      int server = InetAddress.getByName(host).getRawAddress();
      socket = new DatagramSocket(0);
      socket.setTimeout(timeoutMs);
      byte[] buf = new byte[PACKET_BYTES];
      buf[0] = (byte) ((VERSION << 3) | MODE_CLIENT);
      long requestTicks = SystemClock.elapsedRealtime();
      socket.send(new DatagramPacket(buf, PACKET_BYTES, server, NTP_PORT));
      DatagramPacket reply = new DatagramPacket(buf, PACKET_BYTES);
      socket.receive(reply);
      long responseTicks = SystemClock.elapsedRealtime();
      if (reply.getLength() < PACKET_BYTES) {
        return false;
      }
      long transmit = readTimestamp(buf, TRANSMIT_TIME_OFFSET);
      if (transmit == 0) {
        return false;
      }
      // No server-side processing delay compensation: the reply's receive
      // and transmit stamps differ by microseconds on any real server.
      roundTripTime = responseTicks - requestTicks;
      ntpTimeReference = responseTicks;
      ntpTime = transmit + roundTripTime / 2;
      return true;
    } catch (IOException e) {
      return false;
    } catch (RuntimeException e) {
      return false;
    } finally {
      if (socket != null) {
        socket.close();
      }
    }
  }

  /** The server's time in Unix epoch milliseconds, as of {@link #getNtpTimeReference()}. */
  public long getNtpTime() {
    return ntpTime;
  }

  /** The {@link SystemClock#elapsedRealtime()} at which {@link #getNtpTime()} was true. */
  public long getNtpTimeReference() {
    return ntpTime == 0 ? 0 : ntpTimeReference;
  }

  /** How long the exchange took, in milliseconds. */
  public long getRoundTripTime() {
    return roundTripTime;
  }

  /** A 64-bit NTP timestamp at {@code offset}: seconds since 1900 and a 32-bit fraction. */
  private static long readTimestamp(byte[] buf, int offset) {
    long seconds =
        ((buf[offset] & 0xFFL) << 24)
            | ((buf[offset + 1] & 0xFFL) << 16)
            | ((buf[offset + 2] & 0xFFL) << 8)
            | (buf[offset + 3] & 0xFFL);
    if (seconds == 0) {
      return 0;
    }
    // The top byte of the fraction is ~4 ms of resolution.
    long fractionMs = ((buf[offset + 4] & 0xFFL) * 1000L) >> 8;
    return (seconds - SECONDS_1900_TO_1970) * 1000L + fractionMs;
  }
}
