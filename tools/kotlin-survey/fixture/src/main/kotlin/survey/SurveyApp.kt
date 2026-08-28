// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.app.Application
import picodroid.content.Intent
import picodroid.graphics.Color
import picodroid.graphics.Theme
import picodroid.os.SystemClock
import picodroid.util.Log

/**
 * The picoenvmon-shaped fixture's Application (EnvApp): theme statics,
 * a service start, an activity start with extras, a companion with `const`
 * and non-const vals, and a `@JvmStatic` helper. Compiled and dumped, never run.
 */
class SurveyApp : Application() {
    companion object {
        const val TAG = "Survey"
        val BOOT_NS: Long = SystemClock.elapsedRealtimeNanos()

        @JvmStatic
        fun tag(sub: String): String = "$TAG/$sub"
    }

    override fun onCreate() {
        Theme.colorPrimary = Color.RED
        startService(Intent(SensorService::class.java))
        startActivity(Intent(HomeActivity::class.java).putExtra("mode", "live").putExtra("limit", 60))
        Log.i(tag("app"), "boot ${BOOT_NS}ns primary=${Theme.colorPrimary}")
    }
}
