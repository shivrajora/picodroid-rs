// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.home

import picodroid.content.Intent
import picodroid.os.Bundle
import picodroid.util.Log
import picodroid.widget.ArrayAdapter
import picodroid.widget.LinearLayout
import picodroid.widget.ListView
import picoenvmon_kt.R
import picoenvmonkt.TAG
import picoenvmonkt.service.SensorLoggerService
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.ui.history.HistoryActivity
import picoenvmonkt.ui.live.LiveActivity
import picoenvmonkt.ui.network.NetworkActivity
import picoenvmonkt.ui.settings.SettingsActivity

/**
 * Root hub: a selectable menu of destinations under the standardized 4-button navigation model. A/B
 * move the highlight, X opens the highlighted screen, Y exits: the inherited `onBackPressed`
 * finishes this last Activity, which ends the app and returns to the launcher, exactly as on an
 * Android home screen. The live 5-tile sensor dashboard lives in [LiveActivity]; History and
 * Settings are siblings. Adding a screen later is one more `labels`/`destinations` entry, its
 * string and its manifest `<activity>`, plus the new Activity. The two tables are instance fields
 * (Home is created once) rather than statics: a `companion object` would be one more parsed class.
 */
class HomeActivity : NavActivity() {
    private val labels =
        intArrayOf(
            R.string.menu_live,
            R.string.menu_history,
            R.string.menu_network,
            R.string.menu_settings,
        )
    private val destinations =
        arrayOf<Class<*>>(
            LiveActivity::class.java,
            HistoryActivity::class.java,
            NetworkActivity::class.java,
            SettingsActivity::class.java,
        )

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "Home.onCreate")
        getDisplay()

        // The device is an environmental monitor: sensor logging is ON from boot
        // so the web dashboard serves live readings without anyone touching the
        // device (LatestReadings is fed only by this service's 1 Hz smoothing
        // emit — found as "every dashboard row reads --" on a fresh flash). The
        // Live screen's Logger switch restores from isLogging and remains the
        // off-toggle. Idempotent: the service guards re-start internally.
        startService(Intent(this, SensorLoggerService::class.java))

        val root = makeScreenRoot()
        addTitle(root, R.string.app_name)

        val adapter = ArrayAdapter<String>(this, picodroid.R.layout.simple_list_item_1)
        for (label in labels) {
            adapter.add(getString(label))
        }
        val list = ListView(this)
        list.setAdapter(adapter)
        // Android-faithful 4-arg item-click: A/B move the row highlight, X (ENTER) activates the
        // focused row -> open its destination.
        list.setOnItemClickListener { _, _, position, _ ->
            startActivity(Intent(this, destinations[position]))
        }
        // The list takes the height the title and the hint bar leave (android:layout_weight="1"),
        // so the hint bar sits on the bottom edge of every panel.
        root.addView(
            list,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f),
        )

        installHintBar(root, R.string.hint_home)

        setContentView(root)
    }
}
