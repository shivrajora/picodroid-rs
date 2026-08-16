// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import java.io.IOException;
import java.net.SocketTimeoutException;
import picodroid.net.ServerSocket;
import picodroid.net.Socket;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picoenvmon.data.LatestReadings;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.util.Formatter;

/**
 * Minimal HTTP/1.0 dashboard server, driven from the NetworkManager thread. One connection at a
 * time by design: the native listen backlog is 1, the page is ~1 KB, and the browser's 2 s
 * meta-refresh keeps concurrency at ~1 — serial serving is the architecture, not a shortcut. The 1
 * s accept timeout is the caller's housekeeping tick.
 *
 * <p>Every per-connection failure is caught and logged: this thread is the app's entire network
 * stack and must never die (a dead JvmChild task does not respawn on device).
 */
@SuppressWarnings("DefaultCharset") // byte-backed ASCII strings; no Charset class in the SDK
public class HttpServer {
  private static final String TAG = EnvAppComponent.TAG;
  private static final int ACCEPT_TIMEOUT_MS = 1000;
  private static final int CLIENT_TIMEOUT_MS = 2000;
  private static final int REQUEST_BUF_BYTES = 512;

  // Constant page framing, cached as bytes once — the page rebuilds every 2 s
  // forever, so per-request churn matters (GC pacing is a known sore point in
  // this app). Dark palette matches the on-device theme.
  private static final byte[] PAGE_HEAD =
      ("<!DOCTYPE html><html><head><meta http-equiv=\"refresh\" content=\"2\">"
              + "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
              + "<title>PicoEnvMon</title><style>"
              + "body{font-family:monospace;background:#0e1418;color:#f0f0f0;margin:2em}"
              + "h1{color:#26a69a}td{padding:.2em 1em .2em 0}"
              + ".s{color:#a0b4bc}</style></head><body><h1>PicoEnvMon</h1><table>")
          .getBytes();
  private static final byte[] PAGE_TAIL = "</body></html>".getBytes();

  private final EnvAppComponent app;
  private final NetworkManager net;
  private final byte[] reqBuf = new byte[REQUEST_BUF_BYTES];
  private ServerSocket server;

  public HttpServer(EnvAppComponent app, NetworkManager net) {
    this.app = app;
    this.net = net;
  }

  /** Bind + listen if not already open. Returns false on bind failure (caller backs off). */
  public boolean ensureOpen() {
    if (server != null) {
      return true;
    }
    try {
      server = new ServerSocket(NetworkManager.HTTP_PORT);
      server.setSoTimeout(ACCEPT_TIMEOUT_MS);
      Log.i(TAG, "http: serving on port " + NetworkManager.HTTP_PORT);
      return true;
    } catch (IOException e) {
      Log.i(TAG, "http: bind failed: " + e.getMessage());
      server = null;
      return false;
    }
  }

  /**
   * Accept and serve at most one request, blocking up to the 1 s accept timeout. Total against
   * per-connection failures; only the accept timeout is silent (it is the normal idle path).
   */
  public void serveOnce() {
    if (server == null) {
      return;
    }
    Socket client = null;
    try {
      client = server.accept();
      client.setTimeout(CLIENT_TIMEOUT_MS);
      String requestLine = readRequestLine(client);
      if (requestLine != null
          && (requestLine.startsWith("GET / ") || requestLine.startsWith("GET /index.html "))) {
        byte[] body = buildBody();
        sendResponse(client, "200 OK", body);
      } else {
        Log.i(TAG, "http: 404 for " + (requestLine == null ? "(bad request)" : requestLine));
        sendResponse(client, "404 Not Found", "not found\n".getBytes());
      }
    } catch (SocketTimeoutException e) {
      // No client this tick, or a client stalled mid-request — both routine.
    } catch (IOException e) {
      Log.i(TAG, "http: connection error: " + e.getMessage());
    } catch (RuntimeException e) {
      Log.i(TAG, "http: unexpected: " + e);
    } finally {
      if (client != null) {
        client.close();
      }
    }
  }

  public void close() {
    if (server != null) {
      server.close();
      server = null;
    }
  }

  /** Read until the blank line ends the headers; return the request line, or null on garbage. */
  private String readRequestLine(Socket client) throws IOException {
    int total = 0;
    while (total < reqBuf.length) {
      int n = client.recv(reqBuf, total, reqBuf.length - total);
      if (n < 0) {
        break; // orderly EOF before a full request
      }
      total += n;
      if (indexOfHeaderEnd(reqBuf, total) >= 0) {
        break;
      }
    }
    // First line up to CR or LF.
    int end = 0;
    while (end < total && reqBuf[end] != '\r' && reqBuf[end] != '\n') {
      end++;
    }
    if (end == 0) {
      return null;
    }
    return new String(reqBuf, 0, end);
  }

  private static int indexOfHeaderEnd(byte[] buf, int len) {
    for (int i = 3; i < len; i++) {
      if (buf[i] == '\n' && buf[i - 1] == '\r' && buf[i - 2] == '\n' && buf[i - 3] == '\r') {
        return i;
      }
    }
    return -1;
  }

  private void sendResponse(Socket client, String status, byte[] body) throws IOException {
    String head =
        "HTTP/1.0 "
            + status
            + "\r\nContent-Type: text/html\r\nContent-Length: "
            + body.length
            + "\r\nConnection: close\r\n\r\n";
    byte[] headBytes = head.getBytes();
    sendAll(client, headBytes);
    sendAll(client, body);
  }

  /**
   * {@code Socket.send} writes at most one 256-byte native chunk per call and returns the count
   * (the documented NET-9 staging-buffer limit) — anything page-sized must loop.
   */
  private static void sendAll(Socket client, byte[] buf) throws IOException {
    int off = 0;
    while (off < buf.length) {
      int n = client.send(buf, off, buf.length - off);
      if (n <= 0) {
        throw new IOException("send stalled at " + off + "/" + buf.length);
      }
      off += n;
    }
  }

  private byte[] buildBody() {
    LatestReadings latest = app.latestReadings();
    Formatter f = app.formatter();
    StringBuilder sb = new StringBuilder();
    row(
        sb,
        "Temperature",
        latest.isValid(LatestReadings.IDX_TEMPERATURE)
            ? f.formatTemp(latest.get(LatestReadings.IDX_TEMPERATURE))
            : "--");
    row(
        sb,
        "Humidity",
        latest.isValid(LatestReadings.IDX_HUMIDITY)
            ? f.formatHumidity(latest.get(LatestReadings.IDX_HUMIDITY))
            : "--");
    row(
        sb,
        "Pressure",
        latest.isValid(LatestReadings.IDX_PRESSURE)
            ? f.formatPressure(latest.get(LatestReadings.IDX_PRESSURE))
            : "--");
    row(
        sb,
        "Air quality",
        latest.isValid(LatestReadings.IDX_GAS)
            ? f.formatGasIaq(latest.get(LatestReadings.IDX_GAS))
            : "--");
    row(
        sb,
        "Light",
        latest.isValid(LatestReadings.IDX_LIGHT)
            ? f.formatLux(latest.get(LatestReadings.IDX_LIGHT))
            : "--");
    String w = net.weather();
    row(sb, "Outdoor (" + WeatherFetcher.CITY + ")", w != null ? w : "unavailable");
    sb.append("</table><p class=\"s\">");
    sb.append(net.statusFooter());
    sb.append("</p>");
    byte[] mid = sb.toString().getBytes();

    byte[] body = new byte[PAGE_HEAD.length + mid.length + PAGE_TAIL.length];
    System.arraycopy(PAGE_HEAD, 0, body, 0, PAGE_HEAD.length);
    System.arraycopy(mid, 0, body, PAGE_HEAD.length, mid.length);
    System.arraycopy(PAGE_TAIL, 0, body, PAGE_HEAD.length + mid.length, PAGE_TAIL.length);
    return body;
  }

  private static void row(StringBuilder sb, String label, String value) {
    sb.append("<tr><td class=\"s\">");
    sb.append(label);
    sb.append("</td><td>");
    sb.append(value);
    sb.append("</td></tr>");
  }

  /** "3h 12m 45s" from the monotonic clock. */
  static String uptime() {
    long s = SystemClock.elapsedRealtimeNanos() / 1_000_000_000L;
    return (s / 3600) + "h " + ((s % 3600) / 60) + "m " + (s % 60) + "s";
  }
}
