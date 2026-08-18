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

  // Dynamic-middle framing, also cached as bytes: the middle is written
  // straight into pageBuf with the append helpers below, so the serve path
  // allocates nothing at all — every dynamic-string intern here was re-paid
  // per request forever, and that churn is what pinned the board's
  // gc_alloc_threshold at 64 (docs/mem-session-2026-08.md, C2).
  private static final byte[] ROW_OPEN = "<tr><td class=\"s\">".getBytes();
  private static final byte[] ROW_MID = "</td><td>".getBytes();
  private static final byte[] ROW_CLOSE = "</td></tr>".getBytes();
  private static final byte[] LABEL_TEMP = "Temperature".getBytes();
  private static final byte[] LABEL_HUM = "Humidity".getBytes();
  private static final byte[] LABEL_PRES = "Pressure".getBytes();
  private static final byte[] LABEL_AIR = "Air quality".getBytes();
  private static final byte[] LABEL_LIGHT = "Light".getBytes();
  private static final byte[] LABEL_OUTDOOR = ("Outdoor (" + WeatherFetcher.CITY + ")").getBytes();
  private static final byte[] DASHES = "--".getBytes();
  private static final byte[] UNAVAILABLE = "unavailable".getBytes();
  private static final byte[] UNIT_C = "C".getBytes();
  private static final byte[] UNIT_F = "F".getBytes();
  private static final byte[] UNIT_PCT = " %".getBytes();
  private static final byte[] UNIT_HPA = " hPa".getBytes();
  private static final byte[] UNIT_LX = " lx".getBytes();
  private static final byte[] UNIT_IAQ = " IAQ".getBytes();
  private static final byte[] FOOT_OPEN = "</table><p class=\"s\">".getBytes();
  private static final byte[] FOOT_CLOSE = "</p>".getBytes();
  private static final byte[] TIME_UNSYNCED = "time not synced - ".getBytes();
  private static final byte[] UTC_SEP = " UTC - ".getBytes();
  private static final byte[] IP_PREFIX = "IP ".getBytes();
  private static final byte[] UP_PREFIX = " - up ".getBytes();

  // HTTP/1.0 + Connection: close means body length = EOF — no Content-Length,
  // so the response heads are constants too. Per-request garbage matters: the
  // GC threshold counts ALLOCATIONS, and a server allocating few-but-large
  // objects outruns it byte-wise long before it fires (found as an OOM at a
  // 360 KB heap cap: table-growth steps need contiguous KB the accumulated
  // garbage had fragmented away).
  private static final byte[] HEAD_200 =
      "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n".getBytes();
  private static final byte[] HEAD_404 =
      ("HTTP/1.0 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n"
              + "not found\n")
          .getBytes();

  private final EnvAppComponent app;
  private final NetworkManager net;
  private final byte[] reqBuf = new byte[REQUEST_BUF_BYTES];

  /** Persistent page-assembly buffer — only the small dynamic middle allocates per request. */
  private final byte[] pageBuf = new byte[1536];

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
      int lineLen = readRequest(client);
      if (isDashboardGet(lineLen)) {
        int pageLen = buildPage();
        sendAll(client, HEAD_200, HEAD_200.length);
        sendAll(client, pageBuf, pageLen);
      } else {
        // The only per-request allocation on this path is the log line.
        Log.i(TAG, "http: 404 for " + (lineLen > 0 ? new String(reqBuf, 0, lineLen) : "(bad)"));
        sendAll(client, HEAD_404, HEAD_404.length);
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

  /**
   * Read until the blank line ends the headers. Returns the request-line length in {@link #reqBuf}
   * (0 on garbage) — the line stays as bytes; no String is built on the happy path.
   */
  private int readRequest(Socket client) throws IOException {
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
    return end;
  }

  /** Byte-level match for "GET / " / "GET /index.html " — allocation-free. */
  private boolean isDashboardGet(int lineLen) {
    if (lineLen < 6
        || reqBuf[0] != 'G'
        || reqBuf[1] != 'E'
        || reqBuf[2] != 'T'
        || reqBuf[3] != ' '
        || reqBuf[4] != '/') {
      return false;
    }
    if (reqBuf[5] == ' ') {
      return true;
    }
    if (lineLen < 5 + INDEX_HTML.length) {
      return false;
    }
    for (int i = 0; i < INDEX_HTML.length; i++) {
      if (reqBuf[5 + i] != INDEX_HTML[i]) {
        return false;
      }
    }
    return true;
  }

  private static final byte[] INDEX_HTML = "index.html ".getBytes();

  private static int indexOfHeaderEnd(byte[] buf, int len) {
    for (int i = 3; i < len; i++) {
      if (buf[i] == '\n' && buf[i - 1] == '\r' && buf[i - 2] == '\n' && buf[i - 3] == '\r') {
        return i;
      }
    }
    return -1;
  }

  /**
   * {@code Socket.send} writes at most one 256-byte native chunk per call and returns the count
   * (the documented NET-9 staging-buffer limit) — anything page-sized must loop.
   */
  private static void sendAll(Socket client, byte[] buf, int len) throws IOException {
    int off = 0;
    while (off < len) {
      int n = client.send(buf, off, len - off);
      if (n <= 0) {
        throw new IOException("send stalled at " + off + "/" + len);
      }
      off += n;
    }
  }

  /** Assemble the page into {@link #pageBuf}; returns its length. Allocation-free. */
  private int buildPage() {
    LatestReadings latest = app.latestReadings();
    Formatter f = app.formatter();
    int len = 0;
    len = appendClamped(pageBuf, len, PAGE_HEAD);

    len = rowStart(len, LABEL_TEMP);
    if (latest.isValid(LatestReadings.IDX_TEMPERATURE)) {
      len = appendCenti(pageBuf, len, f.tempCenti(latest.get(LatestReadings.IDX_TEMPERATURE)));
      len = appendClamped(pageBuf, len, f.isFahrenheit() ? UNIT_F : UNIT_C);
    } else {
      len = appendClamped(pageBuf, len, DASHES);
    }
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = rowStart(len, LABEL_HUM);
    if (latest.isValid(LatestReadings.IDX_HUMIDITY)) {
      len = appendCenti(pageBuf, len, Formatter.centi(latest.get(LatestReadings.IDX_HUMIDITY)));
      len = appendClamped(pageBuf, len, UNIT_PCT);
    } else {
      len = appendClamped(pageBuf, len, DASHES);
    }
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = rowStart(len, LABEL_PRES);
    if (latest.isValid(LatestReadings.IDX_PRESSURE)) {
      len = appendCenti(pageBuf, len, Formatter.centi(latest.get(LatestReadings.IDX_PRESSURE)));
      len = appendClamped(pageBuf, len, UNIT_HPA);
    } else {
      len = appendClamped(pageBuf, len, DASHES);
    }
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = rowStart(len, LABEL_AIR);
    float gas = latest.isValid(LatestReadings.IDX_GAS) ? latest.get(LatestReadings.IDX_GAS) : 0f;
    if (gas > 0f) {
      len = appendInt(pageBuf, len, Formatter.iaqFromGas(gas));
      len = appendClamped(pageBuf, len, UNIT_IAQ);
    } else {
      len = appendClamped(pageBuf, len, DASHES);
    }
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = rowStart(len, LABEL_LIGHT);
    if (latest.isValid(LatestReadings.IDX_LIGHT)) {
      len = appendInt(pageBuf, len, (int) latest.get(LatestReadings.IDX_LIGHT));
      len = appendClamped(pageBuf, len, UNIT_LX);
    } else {
      len = appendClamped(pageBuf, len, DASHES);
    }
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = rowStart(len, LABEL_OUTDOOR);
    byte[] w = net.weatherBytes();
    len = appendClamped(pageBuf, len, w != null ? w : UNAVAILABLE);
    len = appendClamped(pageBuf, len, ROW_CLOSE);

    len = appendFooter(len);
    len = appendClamped(pageBuf, len, PAGE_TAIL);
    return len;
  }

  /** "HH:MM:SS UTC - IP a.b.c.d - up 3h 12m 45s" — TimeFormat.hms's math, byte-path. */
  private int appendFooter(int off) {
    off = appendClamped(pageBuf, off, FOOT_OPEN);
    if (net.isTimeSynced()) {
      long adjusted =
          System.currentTimeMillis() + picoenvmon.util.TimeFormat.UTC_OFFSET_MINUTES * 60_000L;
      long daySec = (adjusted / 1000L) % 86_400L;
      if (daySec < 0) {
        daySec += 86_400L;
      }
      off = append2(pageBuf, off, (int) (daySec / 3600));
      off = appendByte(pageBuf, off, (byte) ':');
      off = append2(pageBuf, off, (int) ((daySec % 3600) / 60));
      off = appendByte(pageBuf, off, (byte) ':');
      off = append2(pageBuf, off, (int) (daySec % 60));
      off = appendClamped(pageBuf, off, UTC_SEP);
    } else {
      off = appendClamped(pageBuf, off, TIME_UNSYNCED);
    }
    off = appendClamped(pageBuf, off, IP_PREFIX);
    byte[] ip = net.ipBytes();
    if (ip != null) {
      off = appendClamped(pageBuf, off, ip);
    }
    off = appendClamped(pageBuf, off, UP_PREFIX);
    long s = SystemClock.elapsedRealtimeNanos() / 1_000_000_000L;
    off = appendInt(pageBuf, off, (int) (s / 3600));
    off = appendByte(pageBuf, off, (byte) 'h');
    off = appendByte(pageBuf, off, (byte) ' ');
    off = appendInt(pageBuf, off, (int) ((s % 3600) / 60));
    off = appendByte(pageBuf, off, (byte) 'm');
    off = appendByte(pageBuf, off, (byte) ' ');
    off = appendInt(pageBuf, off, (int) (s % 60));
    off = appendByte(pageBuf, off, (byte) 's');
    return appendClamped(pageBuf, off, FOOT_CLOSE);
  }

  private int rowStart(int off, byte[] label) {
    off = appendClamped(pageBuf, off, ROW_OPEN);
    off = appendClamped(pageBuf, off, label);
    return appendClamped(pageBuf, off, ROW_MID);
  }

  private static int appendClamped(byte[] dst, int off, byte[] src) {
    int n = Math.min(src.length, dst.length - off);
    System.arraycopy(src, 0, dst, off, n);
    return off + n;
  }

  private static int appendByte(byte[] dst, int off, byte b) {
    if (off < dst.length) {
      dst[off] = b;
      return off + 1;
    }
    return off;
  }

  /** Decimal int → ASCII digits, clamped like {@link #appendClamped}. */
  private static int appendInt(byte[] dst, int off, int v) {
    if (v < 0) {
      off = appendByte(dst, off, (byte) '-');
    }
    long abs = v < 0 ? -(long) v : v;
    long div = 1;
    while (abs / div >= 10) {
      div *= 10;
    }
    while (div > 0) {
      off = appendByte(dst, off, (byte) ('0' + (int) (abs / div % 10)));
      div /= 10;
    }
    return off;
  }

  /** 1234 → "12.34" — the byte-path twin of Formatter's two-decimal formatting. */
  private static int appendCenti(byte[] dst, int off, int centi) {
    if (centi < 0) {
      off = appendByte(dst, off, (byte) '-');
      centi = -centi;
    }
    off = appendInt(dst, off, centi / 100);
    off = appendByte(dst, off, (byte) '.');
    return append2(dst, off, centi % 100);
  }

  /** Two-digit zero-padded. */
  private static int append2(byte[] dst, int off, int v) {
    off = appendByte(dst, off, (byte) ('0' + (v / 10) % 10));
    return appendByte(dst, off, (byte) ('0' + v % 10));
  }
}
