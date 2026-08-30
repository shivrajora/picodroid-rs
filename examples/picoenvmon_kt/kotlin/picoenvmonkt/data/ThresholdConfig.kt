// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.data

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.content.SharedPreferences
import picodroid.util.Log
import picoenvmonkt.TAG

private const val KEY_TEMP_HI = "temp_hi_centi_c"
private const val KEY_HUM_LO = "hum_lo_milli_pct"
private const val KEY_LUX_LO = "lux_lo"

/**
 * Alert thresholds — when a sensor reading crosses one of these, LiveActivity flashes the matching
 * tile and SensorLoggerService logs an alert. Persisted to [SharedPreferences] so values survive
 * power-cycle. The thresholds are `@JvmField` so Settings writes them with a plain `putfield`
 * instead of through accessor methods.
 */
@Singleton
class ThresholdConfig @Inject constructor(prefs: SharedPreferences) {
    /** Default: 30 °C. */
    @JvmField var tempHiCentiC = 3000

    /** Default: 20 % relative humidity. */
    @JvmField var humLoMilliPct = 20_000

    /** Default: 10 lux. */
    @JvmField var luxLo = 10

    /** Loads the persisted values once; app-scoped so every screen and the Service share them. */
    init {
        load(prefs)
        Log.i(TAG, "thresholds tempHi=$tempHiCentiC humLo=$humLoMilliPct luxLo=$luxLo")
    }

    fun load(p: SharedPreferences) {
        tempHiCentiC = p.getInt(KEY_TEMP_HI, tempHiCentiC)
        humLoMilliPct = p.getInt(KEY_HUM_LO, humLoMilliPct)
        luxLo = p.getInt(KEY_LUX_LO, luxLo)
    }

    fun save(p: SharedPreferences): Boolean =
        p.edit()
            .putInt(KEY_TEMP_HI, tempHiCentiC)
            .putInt(KEY_HUM_LO, humLoMilliPct)
            .putInt(KEY_LUX_LO, luxLo)
            .commit()

    fun tempBreached(celsius: Float): Boolean = (celsius * 100).toInt() >= tempHiCentiC

    fun humidityBreached(milliPct: Float): Boolean = milliPct.toInt() <= humLoMilliPct

    fun luxBreached(lux: Float): Boolean = lux.toInt() <= luxLo
}
