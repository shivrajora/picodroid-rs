// SPDX-License-Identifier: GPL-3.0-only
@file:JvmName("WeatherFetcher")

package picoenvmonkt.net

import java.io.IOException
import picodroid.json.JSONException
import picodroid.json.JSONObject
import picodroid.net.HttpURLConnection
import picodroid.net.URL
import picodroid.util.Log
import picoenvmonkt.TAG

/**
 * Current weather from open-meteo over plain HTTP (no TLS exists on this platform), parsed with
 * [JSONObject]. Strictly fail-soft: this depends on a third-party endpoint and real internet, so
 * every failure — DNS, timeout, non-200, garbage — returns null and the UI renders "unavailable".
 * Nothing in CI ever asserts on weather content.
 *
 * The fetch runs on the NetworkManager thread, serially with dashboard serving, so it must be
 * time-bounded: a stalled endpoint with no timeouts starved the serve loop for a whole 25 s smoke
 * run (nightly 2026-08-18). Connect and read timeouts bound each blocking network call at
 * [TIMEOUT_MS]; the reply is a few hundred bytes, so the read count stays small.
 */

/** Per-phase (connect, read) bound. Worst-case housekeeping stall must stay well under 25 s. */
private const val TIMEOUT_MS = 4000

/** Coordinates of the display city (San Mateo), build-time constants like the Java twin's. */
private const val LAT = "37.56"
private const val LON = "-122.32"

/** Current temperature and WMO weather code only, so the reply stays under 400 bytes. */
private const val WEATHER_URL =
    "http://api.open-meteo.com/v1/forecast?latitude=$LAT&longitude=$LON&current=temperature_2m,weather_code"

private const val MAX_REPLY_BYTES = 512

/**
 * Fetch the current conditions as a one-liner, e.g. "Overcast +17C". Returns null on any failure.
 * ASCII by construction: the description comes from the WMO code table below and the reply's only
 * non-ASCII bytes (the degree sign in `current_units`) are never displayed.
 */
fun fetchWeather(): String? {
    var conn: HttpURLConnection? = null
    try {
        val c = URL(WEATHER_URL).openConnection()
        conn = c
        c.setConnectTimeout(TIMEOUT_MS)
        c.setReadTimeout(TIMEOUT_MS)
        c.connect()
        val code = c.responseCode
        if (code != 200) {
            Log.i(TAG, "weather: HTTP $code")
            return null
        }
        val buf = ByteArray(MAX_REPLY_BYTES)
        val input = c.inputStream
        var total = 0
        while (total < buf.size) {
            val n = input.read(buf, total, buf.size - total)
            if (n < 0) {
                break
            }
            total += n
        }
        if (total == 0) {
            return null
        }
        val line = describe(bytesToString(buf, total))
        Log.i(TAG, "weather: $line")
        return line
    } catch (e: JSONException) {
        Log.i(TAG, "weather: bad reply: ${e.message}")
        return null
    } catch (e: IOException) {
        Log.i(TAG, "weather: fetch failed: ${e.message}")
        return null
    } catch (e: RuntimeException) {
        Log.i(TAG, "weather: unexpected: $e")
        return null
    } finally {
        // 16 HTTP handles exist in total — a leak per 15-min retry would
        // exhaust them within hours.
        conn?.disconnect()
    }
}

/**
 * The Java `String(byte[], int, int)` constructor, which the runtime serves. Kotlin's own
 * `String(bytes, off, len)` inlines to a `Charset` overload it does not (guides/kotlin.md).
 */
@Suppress("PLATFORM_CLASS_MAPPED_TO_KOTLIN")
private fun bytesToString(buf: ByteArray, len: Int): String =
    java.lang.String(buf, 0, len) as String

/** "Overcast +17C" from the reply's `current` object. */
private fun describe(json: String): String {
    val current = JSONObject(json).getJSONObject("current")
    val celsius = current.getDouble("temperature_2m")
    val rounded = (if (celsius >= 0) celsius + 0.5 else celsius - 0.5).toInt()
    val sign = if (rounded >= 0) "+" else ""
    return "${wmoText(current.getInt("weather_code"))} $sign${rounded}C"
}

/** The WMO 4677 weather codes open-meteo reports, in its own wording. */
private fun wmoText(code: Int): String =
    when (code) {
        0 -> "Clear"
        1 -> "Mainly clear"
        2 -> "Partly cloudy"
        3 -> "Overcast"
        45,
        48 -> "Fog"
        51,
        53,
        55 -> "Drizzle"
        56,
        57 -> "Freezing drizzle"
        61,
        63,
        65 -> "Rain"
        66,
        67 -> "Freezing rain"
        71,
        73,
        75 -> "Snow"
        77 -> "Snow grains"
        80,
        81,
        82 -> "Showers"
        85,
        86 -> "Snow showers"
        95 -> "Thunderstorm"
        96,
        99 -> "Thunderstorm with hail"
        else -> "Code $code"
    }
