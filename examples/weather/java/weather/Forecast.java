// SPDX-License-Identifier: GPL-3.0-only
package weather;

import picodroid.json.JSONArray;
import picodroid.json.JSONException;
import picodroid.json.JSONObject;

/**
 * One open-meteo forecast reply, reduced to what the screens show: the current conditions, the next
 * {@link #HOURS} hours and the next {@link #DAYS} days. The reply carries unix timestamps ({@code
 * timeformat=unixtime}) and the place's {@code utc_offset_seconds}; every clock value here is
 * already local to the place.
 */
final class Forecast {
  static final int HOURS = 12;
  static final int DAYS = 7;
  private static final String[] DAY_NAMES = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
  private static final long DAY_SECONDS = 86_400L;

  int utcOffsetSeconds;

  // The current conditions.
  int temperature;
  int feelsLike;
  int humidity;
  int weatherCode;
  boolean isDay;
  int pressure;
  int windSpeed;

  // The next hours, from the current one.
  int hours;
  final int[] hourOfDay = new int[HOURS];
  final int[] hourTemp = new int[HOURS];
  final int[] hourCode = new int[HOURS];
  final boolean[] hourIsDay = new boolean[HOURS];

  // The next days, from today.
  int days;
  final String[] dayName = new String[DAYS];
  final int[] dayCode = new int[DAYS];
  final int[] dayHigh = new int[DAYS];
  final int[] dayLow = new int[DAYS];
  String sunrise;
  String sunset;

  private Forecast() {}

  /** The reply's {@code current}, {@code hourly} and {@code daily} objects, rounded to integers. */
  static Forecast parse(String json) throws JSONException {
    JSONObject root = new JSONObject(json);
    Forecast f = new Forecast();
    int off = root.getInt("utc_offset_seconds");
    f.utcOffsetSeconds = off;

    JSONObject current = root.getJSONObject("current");
    f.temperature = round(current.getDouble("temperature_2m"));
    f.feelsLike = round(current.getDouble("apparent_temperature"));
    f.humidity = round(current.getDouble("relative_humidity_2m"));
    f.weatherCode = current.getInt("weather_code");
    f.isDay = current.getInt("is_day") != 0;
    f.pressure = round(current.getDouble("surface_pressure"));
    f.windSpeed = round(current.getDouble("wind_speed_10m"));

    JSONObject hourly = root.getJSONObject("hourly");
    JSONArray times = hourly.getJSONArray("time");
    JSONArray temps = hourly.getJSONArray("temperature_2m");
    JSONArray codes = hourly.getJSONArray("weather_code");
    JSONArray daylight = hourly.getJSONArray("is_day");
    f.hours = Math.min(HOURS, Math.min(times.length(), Math.min(temps.length(), codes.length())));
    for (int i = 0; i < f.hours; i++) {
      f.hourOfDay[i] = (int) (localDaySeconds(times.getLong(i), off) / 3600);
      f.hourTemp[i] = round(temps.getDouble(i));
      f.hourCode[i] = codes.getInt(i);
      f.hourIsDay[i] = i < daylight.length() && daylight.getInt(i) != 0;
    }

    JSONObject daily = root.getJSONObject("daily");
    JSONArray dayTimes = daily.getJSONArray("time");
    JSONArray dayCodes = daily.getJSONArray("weather_code");
    JSONArray highs = daily.getJSONArray("temperature_2m_max");
    JSONArray lows = daily.getJSONArray("temperature_2m_min");
    int n = Math.min(dayTimes.length(), dayCodes.length());
    n = Math.min(n, Math.min(highs.length(), lows.length()));
    f.days = Math.min(DAYS, n);
    for (int i = 0; i < f.days; i++) {
      f.dayName[i] = i == 0 ? "Today" : DAY_NAMES[dayOfWeek(dayTimes.getLong(i), off)];
      f.dayCode[i] = dayCodes.getInt(i);
      f.dayHigh[i] = round(highs.getDouble(i));
      f.dayLow[i] = round(lows.getDouble(i));
    }
    f.sunrise = clock(daily.getJSONArray("sunrise").getLong(0), off);
    f.sunset = clock(daily.getJSONArray("sunset").getLong(0), off);
    return f;
  }

  /** The place's wall clock, "HH:MM", for a unix time. */
  static String clock(long unixSeconds, int utcOffsetSeconds) {
    long daySec = localDaySeconds(unixSeconds, utcOffsetSeconds);
    return String.format("%02d:%02d", (int) (daySec / 3600), (int) ((daySec % 3600) / 60));
  }

  /** 0 = Sunday, in the place's local time. 1970-01-01 was a Thursday. */
  static int dayOfWeek(long unixSeconds, int utcOffsetSeconds) {
    long days = Math.floorDiv(unixSeconds + utcOffsetSeconds, DAY_SECONDS);
    long weekday = (days + 4) % 7;
    return (int) (weekday < 0 ? weekday + 7 : weekday);
  }

  /** Seconds since the local midnight. */
  private static long localDaySeconds(long unixSeconds, int utcOffsetSeconds) {
    long sec = (unixSeconds + utcOffsetSeconds) % DAY_SECONDS;
    return sec < 0 ? sec + DAY_SECONDS : sec;
  }

  /** The nearest integer, halves away from zero. */
  static int round(double v) {
    return (int) (v >= 0 ? v + 0.5 : v - 0.5);
  }
}
