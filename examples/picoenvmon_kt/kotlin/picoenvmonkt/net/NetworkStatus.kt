// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.net

/**
 * What the Network screen shows, as [NetworkManager.status] publishes it: immutable, one per change
 * of the link, the clock or the weather. A plain class, not a data class: nothing compares or
 * copies it, and the generated members would cost methods.
 */
class NetworkStatus
internal constructor(
    /** One of `STATE_NO_WIFI` and its siblings. */
    @JvmField val state: Int,
    /** Dotted-quad local address, or null while the link is down or its address unknown. */
    @JvmField val ipAddress: String?,
    /** Dashboard URL ("http://a.b.c.d:8080/"), or null with [ipAddress]. */
    @JvmField val url: String?,
    /** Whether the platform's time service has anchored the wall clock this boot. */
    @JvmField val timeSynced: Boolean,
    /** Latest weather one-liner, or null (unavailable / not fetched yet). */
    @JvmField val weather: String?,
)
