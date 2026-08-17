// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import java.io.IOException;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;
import picodroid.util.Log;
import picoenvmon.di.EnvAppComponent;

/**
 * One-line weather via wttr.in over plain HTTP (no TLS exists on this platform). Strictly
 * fail-soft: this depends on a third-party endpoint and real internet, so every failure — DNS,
 * timeout, non-200, garbage — returns null and the UI renders "unavailable". Nothing in CI ever
 * asserts on weather content.
 */
public final class WeatherFetcher {
  private static final String TAG = EnvAppComponent.TAG;

  /**
   * Display name (screen + dashboard labels). Build-time constant; a Settings entry or gradle
   * property is a documented follow-up.
   */
  public static final String CITY = "San Mateo";

  /** wttr.in location path — '+' for spaces, state suffix disambiguates. */
  private static final String CITY_PATH = "San+Mateo,California";

  /** %25 is a URL-escaped '%': the format params are %C (condition) and %t (temperature). */
  private static final String WEATHER_URL = "http://wttr.in/" + CITY_PATH + "?format=%25C+%25t";

  private static final int MAX_REPLY_BYTES = 128;

  private WeatherFetcher() {}

  /**
   * Fetch the one-liner, e.g. "Partly cloudy +11C". Returns null on any failure. ASCII-sanitized:
   * wttr.in emits UTF-8 condition glyphs and degree signs the LVGL font lacks.
   */
  public static String fetch() {
    HttpURLConnection conn = null;
    try {
      conn = new URL(WEATHER_URL).openConnection();
      conn.connect();
      int code = conn.getResponseCode();
      if (code != 200) {
        Log.i(TAG, "weather: HTTP " + code);
        return null;
      }
      byte[] buf = new byte[MAX_REPLY_BYTES];
      HttpInputStream in = conn.getInputStream();
      int total = 0;
      while (total < buf.length) {
        int n = in.read(buf, total, buf.length - total);
        if (n < 0) {
          break;
        }
        total += n;
      }
      String line = sanitize(buf, total);
      if (line.isEmpty()) {
        return null;
      }
      Log.i(TAG, "weather: " + line);
      return line;
    } catch (IOException e) {
      Log.i(TAG, "weather: fetch failed: " + e.getMessage());
      return null;
    } catch (RuntimeException e) {
      Log.i(TAG, "weather: unexpected: " + e);
      return null;
    } finally {
      // 16 HTTP handles exist in total — a leak per 15-min retry would
      // exhaust them within hours.
      if (conn != null) {
        conn.disconnect();
      }
    }
  }

  /** Printable-ASCII filter: multi-byte glyphs collapse to single spaces, CR/LF end the line. */
  private static String sanitize(byte[] buf, int len) {
    StringBuilder sb = new StringBuilder();
    boolean lastSpace = true;
    for (int i = 0; i < len; i++) {
      int b = buf[i] & 0xFF;
      if (b == '\r' || b == '\n') {
        break;
      }
      if (b >= 0x20 && b < 0x7F) {
        boolean space = b == ' ';
        if (!(space && lastSpace)) {
          sb.append((char) b);
        }
        lastSpace = space;
      } else if (!lastSpace) {
        sb.append(' ');
        lastSpace = true;
      }
    }
    return sb.toString().trim();
  }
}
