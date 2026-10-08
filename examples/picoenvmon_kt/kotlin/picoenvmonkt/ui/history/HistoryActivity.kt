// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.history

import javax.inject.Inject
import picodroid.app.AlertDialog
import picodroid.content.ComponentName
import picodroid.content.Intent
import picodroid.content.ServiceConnection
import picodroid.graphics.Theme
import picodroid.os.Bundle
import picodroid.os.IBinder
import picodroid.util.Log
import picodroid.widget.ArrayAdapter
import picodroid.widget.LinearLayout
import picodroid.widget.ListView
import picodroid.widget.TextView
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.RING_CAPACITY
import picoenvmonkt.TAG
import picoenvmonkt.service.SensorLoggerService
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.Formatter
import picoenvmonkt.util.dateTime
import picoenvmonkt.util.hm

/**
 * Max focusable rows rendered. Each `lv_list` button row consumes the board's small (48 KB) LVGL
 * render pool, and the full ring (60) leaves too little headroom for the draw tasks needed to
 * render them — so we show only the most recent window. Comfortably within the safe bound on this
 * board; raising it risks a render-pool stall.
 */
private const val MAX_ROWS = 12

/**
 * Temperature history (reached from the Home hub). Binds [SensorLoggerService], snapshots its
 * temperature ring buffer, and renders one focusable [ListView] row per sample. Under the
 * standardized model A/B move the row highlight, X opens an info dialog for the highlighted sample
 * (BACK/Y dismisses it), and Y returns to the hub. The info dialog is now an explicit on-demand
 * action rather than firing unconditionally on connect.
 */
class HistoryActivity : NavActivity(), ServiceConnection {
    @Inject lateinit var formatter: Formatter
    private var list: ListView? = null
    private var statusLine: TextView? = null
    private val samples = FloatArray(RING_CAPACITY)
    private val sampleTs = IntArray(RING_CAPACITY)
    private var sampleCount = 0
    private var firstShown = 0

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "History.onCreate")
        getDisplay()

        val root = makeScreenRoot()

        val title = TextView()
        title.setText("Temp history")
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)

        val status = TextView()
        // ASCII "..." — the bundled font has no ellipsis (U+2026) glyph.
        status.setText("Connecting...")
        status.setTextColor(Theme.colorTextSecondary)
        root.addView(status)
        statusLine = status

        val rows = ListView()
        rows.setOnItemClickListener { _, _, position, _ -> showSampleDialog(position) }
        // Weighted: the list grows with the panel and the hint bar stays on the bottom edge.
        root.addView(
            rows,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f),
        )
        list = rows

        installHintBar(root, "A:Up  B:Down  X:Info  Y:Back")

        setContentView(root)

        bindService(Intent(SensorLoggerService::class.java), this, BIND_AUTO_CREATE)
    }

    override fun onDestroy() {
        try {
            unbindService(this)
        } catch (t: Throwable) {
            Log.i(TAG, "History unbind ignored: $t")
        }
    }

    override fun onServiceConnected(name: ComponentName, binder: IBinder) {
        val svc = (binder as SensorLoggerService.LocalBinder).service ?: return
        sampleCount = svc.snapshot(IDX_TEMPERATURE, samples, sampleTs)
        Log.i(TAG, "History bound, samples=$sampleCount")

        // Render the most-recent window (see MAX_ROWS). Rows are labelled with their real ring
        // index.
        firstShown = if (sampleCount > MAX_ROWS) sampleCount - MAX_ROWS else 0
        val status = statusLine
        if (sampleCount == 0) {
            // The ring fills only while the SensorLoggerService runs, and it survives
            // screen changes only as a started/foreground service, so point the user at
            // the Logger toggle in Live. Kept short to fit the status line width.
            status?.setText("No data - enable Logger")
        } else if (firstShown > 0) {
            status?.setText("$sampleCount samples (recent $MAX_ROWS)")
        } else {
            status?.setText("$sampleCount samples")
        }

        val f = formatter
        val adapter = ArrayAdapter<String>()
        for (i in firstShown until sampleCount) {
            // "HH:MM 21.3C" once the sample carries an NTP-anchored stamp; the
            // pre-sync "[i]" ring-index form otherwise.
            val label =
                if (sampleTs[i] > 0) hm(sampleTs[i] * 1000L) + " " + f.formatTemp(samples[i])
                else "[$i] " + f.formatTemp(samples[i])
            adapter.add(label)
        }
        list?.setAdapter(adapter)
    }

    override fun onServiceDisconnected(name: ComponentName) {
        Log.i(TAG, "History service disconnected")
    }

    private fun showSampleDialog(position: Int) {
        // position is the row index within the rendered window; map back to the real sample index.
        val idx = firstShown + position
        if (idx < 0 || idx >= sampleCount) {
            return
        }
        val f = formatter
        val whenStamp = if (sampleTs[idx] > 0) "\nTime: " + dateTime(sampleTs[idx] * 1000L) else ""
        AlertDialog.Builder()
            .setTitle("Sample $idx")
            .setMessage("Temperature: " + f.formatTemp(samples[idx]) + whenStamp)
            .setPositiveButton("OK") { _, _ -> }
            .show()
    }
}
