// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.home

import picodroid.content.Intent
import picodroid.graphics.Theme
import picodroid.util.Log
import picodroid.widget.ArrayAdapter
import picodroid.widget.ListView
import picodroid.widget.TextView
import picoenvmonkt.TAG
import picoenvmonkt.service.SensorLoggerService
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.ui.history.HistoryActivity
import picoenvmonkt.ui.live.LiveActivity
import picoenvmonkt.ui.network.NetworkActivity
import picoenvmonkt.ui.settings.SettingsActivity

/**
 * Root hub: a selectable menu of destinations under the standardized 4-button navigation model. A/B
 * move the highlight, X opens the highlighted screen; Y is intentionally disabled here so the root
 * hub can't be backed out of (which would exit the app). The live 5-tile sensor dashboard lives in
 * [LiveActivity]; History and Settings are siblings. Adding a screen later is one more
 * `labels`/`destinations` entry plus the new Activity. The two tables are instance fields (Home is
 * created once) rather than statics: a `companion object` would be one more parsed class.
 */
class HomeActivity : NavActivity() {
    private val labels = arrayOf("Live", "History", "Network", "Settings")
    private val destinations =
        arrayOf<Class<*>>(
            LiveActivity::class.java,
            HistoryActivity::class.java,
            NetworkActivity::class.java,
            SettingsActivity::class.java,
        )

    // Held as a field so the GC roots the menu ListView via this Activity, in addition to the
    // native
    // item-click listener map — defense-in-depth against the unfielded-callback-view sweep.
    private var menu: ListView? = null

    override fun onCreate() {
        Log.i(TAG, "Home.onCreate")
        getDisplay()

        // The device is an environmental monitor: sensor logging is ON from boot
        // so the web dashboard serves live readings without anyone touching the
        // device (LatestReadings is fed only by this service's 1 Hz smoothing
        // emit — found as "every dashboard row reads --" on a fresh flash). The
        // Live screen's Logger switch restores from isLogging and remains the
        // off-toggle. Idempotent: the service guards re-start internally.
        startService(Intent(SensorLoggerService::class.java))

        val root = makeScreenRoot()

        val title = TextView()
        title.setText("PicoEnvMonKt")
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)

        val list = ListView()
        list.setSize(224, 188)
        list.setAdapter(ArrayAdapter<String>(labels))
        // Android-faithful 4-arg item-click: A/B move the row highlight, X (ENTER) activates the
        // focused row -> open its destination.
        list.setOnItemClickListener { _, _, position, _ ->
            startActivity(Intent(destinations[position]))
        }
        root.addView(list)
        menu = list

        installHintBar(root, "A:Up  B:Down  X:Open")

        setContentView(root)
    }

    // Root hub: Back has nowhere to return to, so swallow it instead of finishing (which would exit
    // the app). Deliberately does not call super.onBackPressed().
    override fun onBackPressed() {
        // no-op
    }
}
