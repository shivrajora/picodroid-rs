// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.net;

import java.io.IOException;
import picodroid.json.JSONException;
import picodroid.json.JSONObject;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;
import picodroid.util.Log;
import picoenvmon.EnvApp;

/**
 * Current weather from open-meteo over plain HTTP (no TLS exists on this platform), parsed with
 * {@link JSONObject}. Strictly fail-soft: this depends on a third-party endpoint and real internet,
 * so every failure — DNS, timeout, non-200, garbage — returns null and the UI renders
 * "unavailable". Nothing in CI ever asserts on weather content.
 *
 * <p>The fetch runs on the NetworkManager thread, serially with dashboard serving, so it must be
 * time-bounded: a stalled endpoint with no timeouts starved the serve loop for a whole 25 s smoke
 * run (nightly 2026-08-18). Connect and read timeouts bound each blocking network call at {@code
 * TIMEOUT_MS}; the reply is a few hundred bytes, so the read count stays small.
 */
public final class WeatherFetcher {
  private static final String TAG = EnvApp.TAG;

  /** Per-phase (connect, read) bound. Worst-case housekeeping stall must stay well under 25 s. */
  private static final int TIMEOUT_MS = 4000;

  /**
   * Display name (screen + dashboard labels). Build-time constant; a Settings entry or gradle
   * property is a documented follow-up.
   */
  public static final String CITY = "San Mateo";

  /** Coordinates of {@link #CITY}, the same build-time constants. */
  private static final String LAT = "37.56";

  private static final String LON = "-122.32";

  /** Current temperature and WMO weather code only, so the reply stays under 400 bytes. */
  private static final String WEATHER_URL =
      "http://api.open-meteo.com/v1/forecast?latitude="
          + LAT
          + "&longitude="
          + LON
          + "&current=temperature_2m,weather_code";

  private static final int MAX_REPLY_BYTES = 512;

  private WeatherFetcher() {}

  /**
   * Fetch the current conditions as a one-liner, e.g. "Overcast +17C". Returns null on any failure.
   * ASCII by construction: the description comes from the WMO code table below and the reply's only
   * non-ASCII bytes (the degree sign in {@code current_units}) are never displayed.
   */
  public static String fetch() {
    HttpURLConnection conn = null;
    try {
      conn = new URL(WEATHER_URL).openConnection();
      conn.setConnectTimeout(TIMEOUT_MS);
      conn.setReadTimeout(TIMEOUT_MS);
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
      if (total == 0) {
        return null;
      }
      String line = describe(new String(buf, 0, total));
      Log.i(TAG, "weather: " + line);
      return line;
    } catch (JSONException e) {
      Log.i(TAG, "weather: bad reply: " + e.getMessage());
      return null;
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

  /** "Overcast +17C" from the reply's {@code current} object. */
  static String describe(String json) throws JSONException {
    JSONObject current = new JSONObject(json).getJSONObject("current");
    double celsius = current.getDouble("temperature_2m");
    int rounded = (int) (celsius >= 0 ? celsius + 0.5 : celsius - 0.5);
    return wmoText(current.getInt("weather_code"))
        + " "
        + (rounded >= 0 ? "+" : "")
        + rounded
        + "C";
  }

  /** The WMO 4677 weather codes open-meteo reports, in its own wording. */
  static String wmoText(int code) {
    switch (code) {
      case 0:
        return "Clear";
      case 1:
        return "Mainly clear";
      case 2:
        return "Partly cloudy";
      case 3:
        return "Overcast";
      case 45:
      case 48:
        return "Fog";
      case 51:
      case 53:
      case 55:
        return "Drizzle";
      case 56:
      case 57:
        return "Freezing drizzle";
      case 61:
      case 63:
      case 65:
        return "Rain";
      case 66:
      case 67:
        return "Freezing rain";
      case 71:
      case 73:
      case 75:
        return "Snow";
      case 77:
        return "Snow grains";
      case 80:
      case 81:
      case 82:
        return "Showers";
      case 85:
      case 86:
        return "Snow showers";
      case 95:
        return "Thunderstorm";
      case 96:
      case 99:
        return "Thunderstorm with hail";
      default:
        return "Code " + code;
    }
  }
}
