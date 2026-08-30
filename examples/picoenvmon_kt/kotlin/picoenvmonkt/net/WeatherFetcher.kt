// SPDX-License-Identifier: GPL-3.0-only
@file:JvmName("WeatherFetcher")

package picoenvmonkt.net

import java.io.IOException
import picodroid.net.HttpURLConnection
import picodroid.net.URL
import picodroid.util.Log
import picoenvmonkt.TAG

/**
 * One-line weather via wttr.in over plain HTTP (no TLS exists on this platform). Strictly
 * fail-soft: this depends on a third-party endpoint and real internet, so every failure — DNS,
 * timeout, non-200, garbage — returns null and the UI renders "unavailable". Nothing in CI ever
 * asserts on weather content.
 *
 * The fetch runs on the NetworkManager thread, serially with dashboard serving, so it must be
 * time-bounded: a stalled wttr.in with no timeouts starved the serve loop for a whole 25 s smoke
 * run (nightly 2026-08-18). Connect and read timeouts bound each blocking network call at
 * [TIMEOUT_MS]; the tiny fixed-size reply keeps the read count small.
 */

/** Per-phase (connect, read) bound. Worst-case housekeeping stall must stay well under 25 s. */
private const val TIMEOUT_MS = 4000

/** wttr.in location path — '+' for spaces, state suffix disambiguates. */
private const val CITY_PATH = "San+Mateo,California"

/** %25 is a URL-escaped '%': the format params are %C (condition) and %t (temperature). */
private const val WEATHER_URL = "http://wttr.in/$CITY_PATH?format=%25C+%25t"

private const val MAX_REPLY_BYTES = 128

/**
 * Fetch the one-liner, e.g. "Partly cloudy +11C". Returns null on any failure. ASCII-sanitized:
 * wttr.in emits UTF-8 condition glyphs and degree signs the LVGL font lacks.
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
        val line = sanitize(buf, total)
        if (line.isEmpty()) {
            return null
        }
        Log.i(TAG, "weather: $line")
        return line
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
 * Printable-ASCII filter: multi-byte glyphs collapse to single spaces, CR/LF end the line, and the
 * result carries no leading or trailing space — the Java version's `trim()` would be a
 * `kotlin/text/StringsKt` call here, so the space is held back until a printable character follows.
 */
private fun sanitize(buf: ByteArray, len: Int): String {
    val sb = StringBuilder()
    var wrote = false
    var pendingSpace = false
    for (i in 0 until len) {
        val b = buf[i].toInt() and 0xFF
        if (b == '\r'.code || b == '\n'.code) {
            break
        }
        if (b > 0x20 && b < 0x7F) {
            if (pendingSpace) {
                sb.append(' ')
                pendingSpace = false
            }
            sb.append(b.toChar())
            wrote = true
        } else if (wrote) {
            pendingSpace = true
        }
    }
    return sb.toString()
}
