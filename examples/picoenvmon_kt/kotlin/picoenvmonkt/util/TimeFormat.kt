// SPDX-License-Identifier: GPL-3.0-only
@file:JvmName("TimeFormat")

package picoenvmonkt.util

/**
 * Epoch-milliseconds → display strings, pure integer math (there is no java.util.Date/Calendar in
 * the SDK). Times are UTC plus a single build-time offset — no timezone database exists on this
 * platform; adjust [UTC_OFFSET_MINUTES] for local display if desired. Top-level functions in a
 * `TimeFormat` facade: plain `invokestatic`, no singleton object to allocate.
 */

/** Displayed-time offset from UTC, in minutes. 0 = UTC (documented on the Network screen). */
const val UTC_OFFSET_MINUTES = 0

/** "HH:MM:SS". */
fun hms(epochMs: Long): String {
    val daySec = localDaySeconds(epochMs)
    return String.format(
        "%02d:%02d:%02d",
        (daySec / 3600).toInt(),
        ((daySec % 3600) / 60).toInt(),
        (daySec % 60).toInt(),
    )
}

/** "HH:MM" — for tight History rows. */
fun hm(epochMs: Long): String {
    val daySec = localDaySeconds(epochMs)
    return String.format("%02d:%02d", (daySec / 3600).toInt(), ((daySec % 3600) / 60).toInt())
}

/** "YYYY-MM-DD HH:MM:SS". */
fun dateTime(epochMs: Long): String {
    val adjusted = epochMs + UTC_OFFSET_MINUTES * 60_000L
    val days = floorDiv(adjusted, 86_400_000L)
    val ymd = civilFromDays(days)
    return String.format("%04d-%02d-%02d ", ymd[0], ymd[1], ymd[2]) + hms(epochMs)
}

private fun localDaySeconds(epochMs: Long): Long {
    val adjusted = epochMs + UTC_OFFSET_MINUTES * 60_000L
    val sec = floorDiv(adjusted, 1000L)
    var daySec = sec % 86_400L
    if (daySec < 0) {
        daySec += 86_400L
    }
    return daySec
}

/**
 * Days-since-epoch → {year, month, day}. Howard Hinnant's civil_from_days algorithm — integer only,
 * exact over the int range.
 */
private fun civilFromDays(days: Long): IntArray {
    val z = days + 719_468L
    val era = floorDiv(z, 146_097L)
    val doe = z - era * 146_097L // [0, 146096]
    val yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365 // [0, 399]
    var y = yoe + era * 400
    val doy = doe - (365 * yoe + yoe / 4 - yoe / 100) // [0, 365]
    val mp = (5 * doy + 2) / 153 // [0, 11]
    val d = doy - (153 * mp + 2) / 5 + 1 // [1, 31]
    val m = if (mp < 10) mp + 3 else mp - 9 // [1, 12]
    if (m <= 2) {
        y += 1
    }
    return intArrayOf(y.toInt(), m.toInt(), d.toInt())
}

private fun floorDiv(a: Long, b: Long): Long {
    var q = a / b
    if ((a % b != 0L) && ((a < 0) != (b < 0))) {
        q--
    }
    return q
}
