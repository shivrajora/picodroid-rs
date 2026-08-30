// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.network

import javax.inject.Inject
import picodroid.graphics.Theme
import picodroid.util.Log
import picodroid.widget.Button
import picodroid.widget.LinearLayout
import picodroid.widget.TextView
import picoenvmonkt.CITY
import picoenvmonkt.STATE_FAILED
import picoenvmonkt.STATE_JOINING
import picoenvmonkt.STATE_NO_WIFI
import picoenvmonkt.STATE_UP
import picoenvmonkt.TAG
import picoenvmonkt.net.NetworkManager
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.hms

/**
 * Network status screen (reached from the Home hub). On WiFi-less boards it degrades to a single
 * "not available" message — the hub entry is shown everywhere so the feature is discoverable, and
 * this screen is where the `FEATURE_WIFI` probe's answer becomes visible.
 *
 * Callbacks arrive via [NetworkManager.Listener] on the main executor, so all widget mutation here
 * happens on the main thread.
 *
 * Sim caveat: the simulator's `NetworkInfo.getIpAddress()` is hardcoded to 127.0.0.1, so the URL
 * line reads `http://127.0.0.1:8080/` even though the server binds 0.0.0.0 and is LAN-reachable via
 * the host's real address.
 */
class NetworkActivity : NavActivity(), NetworkManager.Listener {
    @Inject lateinit var net: NetworkManager
    private var statusLine: TextView? = null
    private var ipLine: TextView? = null
    private var urlLine: TextView? = null
    private var timeLine: TextView? = null
    private var weatherLine: TextView? = null

    override fun onCreate() {
        Log.i(TAG, "Network.onCreate")
        getDisplay()

        val root = makeScreenRoot()

        val title = TextView()
        title.setText("Network")
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)

        if (net.state == STATE_NO_WIFI) {
            val msg = TextView()
            msg.setText("WiFi not available on this board")
            msg.setTextColor(Theme.colorTextSecondary)
            root.addView(msg)
            installHintBar(root, "Y:Back")
            setContentView(root)
            return
        }

        statusLine = addLine(root, Theme.colorText)
        ipLine = addLine(root, Theme.colorText)
        urlLine = addLine(root, Theme.colorText)
        timeLine = addLine(root, Theme.colorText)
        weatherLine = addLine(root, Theme.colorTextSecondary)

        // The screen's one focusable widget — auto-focused, X activates.
        val refresh = Button("Refresh")
        refresh.setOnClickListener { net.requestRefresh() }
        root.addView(refresh)

        installHintBar(root, "X:Refresh  Y:Back")
        setContentView(root)

        net.addListener(this)
        repaint()
    }

    private fun addLine(root: LinearLayout, color: Int): TextView {
        val line = TextView()
        line.setTextColor(color)
        root.addView(line)
        return line
    }

    override fun onDestroy() {
        net.removeListener(this)
    }

    override fun onNetworkChanged() {
        if (statusLine != null) {
            repaint()
        }
    }

    private fun repaint() {
        // Mutable properties never smart-cast; the degraded path leaves them null, so bind locals.
        val status = statusLine ?: return
        status.setText(
            when (net.state) {
                STATE_JOINING -> "WiFi: joining..."
                STATE_UP -> "WiFi: connected"
                STATE_FAILED -> "WiFi: not connected (retrying)"
                else -> "WiFi: unavailable"
            }
        )
        val ip = net.ipAddress
        ipLine?.setText(if (ip != null) "IP: $ip" else "IP: -")
        urlLine?.setText(net.url ?: "")
        timeLine?.setText(
            if (net.isTimeSynced) "Time: " + hms(System.currentTimeMillis()) + " UTC"
            else "Time: not synced"
        )
        val w = net.weather
        weatherLine?.setText("$CITY: ${w ?: "unavailable"}")
    }
}
