// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt

/**
 * Every cross-file constant of the app, as top-level `const val`s: kotlinc inlines them at each use
 * site, so this facade class is registered in the PAPK (20 B) but never parsed — unlike a
 * `companion object`, which is a real class that `<clinit>` instantiates.
 */
const val TAG = "PicoEnvMonKt"
const val PREFS_NAME = "picoenvmonkt"

/** Stable indices into every per-sensor table (rings, latest readings, Live tiles). */
const val IDX_TEMPERATURE = 0
const val IDX_HUMIDITY = 1
const val IDX_PRESSURE = 2
const val IDX_GAS = 3
const val IDX_LIGHT = 4
const val READING_COUNT = 5

/** Samples kept per sensor by `SensorLoggerService`. */
const val RING_CAPACITY = 60

/**
 * `NetworkManager` states. Board has no network link (`FEATURE_WIFI` / `FEATURE_ETHERNET` absent) —
 * thread never starts.
 */
const val STATE_NO_WIFI = 0

/** Waiting for the join + DHCP (~10 s on hardware; instant in sim). */
const val STATE_JOINING = 1

/** IP stack up; server/NTP/weather active. */
const val STATE_UP = 2

/** Initial 30 s wait expired — still retrying at a slow cadence. */
const val STATE_FAILED = 3

const val HTTP_PORT = 8080

/**
 * Weather display name (screen + dashboard labels). Build-time constant; a Settings entry or gradle
 * property is a documented follow-up.
 */
const val CITY = "San Mateo"
