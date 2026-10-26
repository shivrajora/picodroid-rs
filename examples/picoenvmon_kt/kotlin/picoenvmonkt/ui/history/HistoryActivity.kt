// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.history

import javax.inject.Inject
import picodroid.app.AlertDialog
import picodroid.content.ComponentName
import picodroid.content.Intent
import picodroid.content.ServiceConnection
import picodroid.graphics.Theme
import picodroid.lifecycle.ViewModelProvider
import picodroid.os.Bundle
import picodroid.os.IBinder
import picodroid.util.Log
import picodroid.widget.ArrayAdapter
import picodroid.widget.LinearLayout
import picodroid.widget.ListView
import picodroid.widget.TextView
import picoenvmon_kt.R
import picoenvmonkt.TAG
import picoenvmonkt.service.SensorLoggerService
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.Formatter
import picoenvmonkt.util.dateTime
import picoenvmonkt.util.hm

/**
 * Temperature history (reached from the Home hub). Binds [SensorLoggerService], has
 * [HistoryViewModel] snapshot its temperature ring, and renders one focusable [ListView] row per
 * sample. Under the standardized model A/B move the row highlight, X opens an info dialog for the
 * highlighted sample (BACK/Y dismisses it), and Y returns to the hub.
 */
class HistoryActivity : NavActivity(), ServiceConnection {
    @Inject lateinit var formatter: Formatter
    private var model: HistoryViewModel? = null
    private var list: ListView? = null
    private var statusLine: TextView? = null

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "History.onCreate")
        getDisplay()

        val root = makeScreenRoot()
        addTitle(root, R.string.title_history)

        val status = TextView(this)
        status.setText(getString(R.string.history_connecting))
        status.setTextColor(Theme.colorTextSecondary)
        root.addView(status)
        statusLine = status

        val rows = ListView(this)
        rows.setOnItemClickListener { _, _, position, _ -> showSampleDialog(position) }
        // Weighted: the list grows with the panel and the hint bar stays on the bottom edge.
        root.addView(
            rows,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f),
        )
        list = rows

        installHintBar(root, R.string.hint_history)

        setContentView(root)

        val vm = ViewModelProvider(this).get(HistoryViewModel::class.java)
        vm.history().observe(this) { showHistory(it) }
        model = vm

        bindService(Intent(this, SensorLoggerService::class.java), this, BIND_AUTO_CREATE)
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
        model?.load(svc)
    }

    override fun onServiceDisconnected(name: ComponentName) {
        Log.i(TAG, "History service disconnected")
    }

    private fun showHistory(h: HistoryViewModel.History) {
        val count = h.count
        Log.i(TAG, "History bound, samples=$count")
        // The ring fills only while the SensorLoggerService runs, and it survives screen changes
        // only as a started/foreground service, so an empty one points the user at the Logger
        // toggle in Live.
        statusLine?.setText(
            if (count == 0) getString(R.string.history_empty)
            else getString(R.string.history_count, count)
        )

        // Every sample is a row: the list keeps only the rows in view alive and re-binds them as
        // it scrolls, so the whole ring costs what a screenful did.
        val f = formatter
        val adapter = ArrayAdapter<String>(this, picodroid.R.layout.simple_list_item_1)
        for (i in 0 until count) {
            val temp = f.formatTemp(h.samples[i])
            val ts = h.timestamps[i]
            adapter.add(
                if (ts > 0) getString(R.string.history_row_time, hm(ts * 1000L), temp)
                else getString(R.string.history_row_index, i, temp)
            )
        }
        val rows = list ?: return
        rows.setAdapter(adapter)
        // Oldest first, as the ring is; open on the newest sample.
        rows.setSelection(count - 1)
        Log.i(TAG, "History rows " + rows.getChildCount() + " of " + count)
    }

    private fun showSampleDialog(position: Int) {
        val h = model?.history()?.getValue() ?: return
        if (position < 0 || position >= h.count) {
            return
        }
        val temp = formatter.formatTemp(h.samples[position])
        val ts = h.timestamps[position]
        val message =
            if (ts > 0) getString(R.string.sample_message_time, temp, dateTime(ts * 1000L))
            else getString(R.string.sample_message, temp)
        AlertDialog.Builder(this)
            .setTitle(getString(R.string.sample_title, position))
            .setMessage(message)
            .setPositiveButton(getString(R.string.ok)) { _, _ -> }
            .show()
    }
}
