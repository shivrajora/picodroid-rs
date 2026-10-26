// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.data;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.content.SharedPreferences;
import picodroid.util.Log;
import picoenvmon.EnvApp;

/**
 * Alert thresholds — when a sensor reading crosses one of these, LiveActivity flashes the matching
 * tile and SensorLoggerService logs an alert. Persisted to the app's {@link SharedPreferences} so
 * values survive power-cycle; {@code EnvApp} loads them once at start, Settings saves them.
 */
@Singleton
public class ThresholdConfig {
  private static final String KEY_TEMP_HI = "temp_hi_centi_c";
  private static final String KEY_HUM_LO = "hum_lo_milli_pct";
  private static final String KEY_LUX_LO = "lux_lo";

  /** Default: 30 °C. */
  public int tempHiCentiC = 3000;

  /** Default: 20 % relative humidity. */
  public int humLoMilliPct = 20_000;

  /** Default: 10 lux. */
  public int luxLo = 10;

  /** App-scoped so every screen and the Service share one set. */
  @Inject
  public ThresholdConfig() {}

  /** Read the persisted values over the defaults. */
  public void load(SharedPreferences p) {
    tempHiCentiC = p.getInt(KEY_TEMP_HI, tempHiCentiC);
    humLoMilliPct = p.getInt(KEY_HUM_LO, humLoMilliPct);
    luxLo = p.getInt(KEY_LUX_LO, luxLo);
    Log.i(
        EnvApp.TAG,
        "thresholds tempHi=" + tempHiCentiC + " humLo=" + humLoMilliPct + " luxLo=" + luxLo);
  }

  public boolean save(SharedPreferences p) {
    return p.edit()
        .putInt(KEY_TEMP_HI, tempHiCentiC)
        .putInt(KEY_HUM_LO, humLoMilliPct)
        .putInt(KEY_LUX_LO, luxLo)
        .commit();
  }

  public boolean tempBreached(float celsius) {
    return ((int) (celsius * 100)) >= tempHiCentiC;
  }

  public boolean humidityBreached(float milliPct) {
    return ((int) milliPct) <= humLoMilliPct;
  }

  public boolean luxBreached(float lux) {
    return ((int) lux) <= luxLo;
  }
}
