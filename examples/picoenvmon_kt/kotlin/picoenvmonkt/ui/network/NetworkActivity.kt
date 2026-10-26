// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.network

import javax.inject.Inject
import picodroid.graphics.Theme
import picodroid.os.Bundle
import picodroid.util.Log
import picodroid.widget.Button
import picodroid.widget.LinearLayout
import picodroid.widget.TextView
import picoenvmon_kt.R
import picoenvmonkt.CITY
import picoenvmonkt.STATE_FAILED
import picoenvmonkt.STATE_JOINING
import picoenvmonkt.STATE_NO_WIFI
import picoenvmonkt.STATE_UP
import picoenvmonkt.TAG
import picoenvmonkt.net.NetworkManager
import picoenvmonkt.net.NetworkStatus
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.hms

/**
 * Network status screen (reached from the Home hub). On boards without a network link it degrades
 * to a single "not available" message — the hub entry is shown everywhere so the feature is
 * discoverable, and this screen is where the `FEATURE_WIFI` / `FEATURE_ETHERNET` probe's answer
 * becomes visible.
 *
 * Observes [NetworkManager.status] with this Activity as the lifecycle owner: every change arrives
 * on the main thread while the screen is started, and the observer goes with the screen.
 */
class NetworkActivity : NavActivity() {
    @Inject lateinit var net: NetworkManager
    private var statusLine: TextView? = null
    private var ipLine: TextView? = null
    private var urlLine: TextView? = null
    private var timeLine: TextView? = null
    private var weatherLine: TextView? = null

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "Network.onCreate")
        getDisplay()

        val root = makeScreenRoot()
        addTitle(root, R.string.menu_network)

        val now = net.status().getValue()
        if (now == null || now.state == STATE_NO_WIFI) {
            val msg = TextView(this)
            msg.setText(getString(R.string.net_not_available))
            msg.setTextColor(Theme.colorTextSecondary)
            root.addView(msg)
            installHintBar(root, R.string.hint_back)
            setContentView(root)
            return
        }

        statusLine = addLine(root, Theme.colorText)
        ipLine = addLine(root, Theme.colorText)
        urlLine = addLine(root, Theme.colorText)
        timeLine = addLine(root, Theme.colorText)
        weatherLine = addLine(root, Theme.colorTextSecondary)

        // The screen's one focusable widget — auto-focused, X activates.
        val refresh = Button(this)
        refresh.setText(getString(R.string.refresh))
        refresh.setOnClickListener { net.requestRefresh() }
        root.addView(refresh)

        installHintBar(root, R.string.hint_network)
        setContentView(root)

        net.status().observe(this) { show(it) }
    }

    private fun addLine(root: LinearLayout, color: Int): TextView {
        val line = TextView(this)
        line.setTextColor(color)
        root.addView(line)
        return line
    }

    private fun show(s: NetworkStatus) {
        statusLine?.setText(
            getString(
                when (s.state) {
                    STATE_JOINING -> R.string.net_joining
                    STATE_UP -> R.string.net_connected
                    STATE_FAILED -> R.string.net_failed
                    else -> R.string.net_down
                }
            )
        )
        val ip = s.ipAddress
        ipLine?.setText(
            if (ip != null) getString(R.string.net_ip, ip) else getString(R.string.net_ip_none)
        )
        urlLine?.setText(s.url ?: "")
        timeLine?.setText(
            if (s.timeSynced) getString(R.string.net_time, hms(System.currentTimeMillis()))
            else getString(R.string.net_time_unsynced)
        )
        weatherLine?.setText(
            getString(
                R.string.net_weather,
                CITY,
                s.weather ?: getString(R.string.weather_unavailable),
            )
        )
    }
}
