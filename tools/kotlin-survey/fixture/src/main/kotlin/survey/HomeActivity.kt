// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.app.Activity
import picodroid.app.AlertDialog
import picodroid.concurrent.Executors
import picodroid.concurrent.Thread
import picodroid.content.Context
import picodroid.content.Intent
import picodroid.content.ServiceConnection
import picodroid.hardware.Sensor
import picodroid.hardware.SensorEvent
import picodroid.hardware.SensorEventListener
import picodroid.hardware.SensorManager
import picodroid.os.IBinder
import picodroid.util.Log
import picodroid.widget.Button
import picodroid.widget.CheckBox
import picodroid.widget.LinearLayout
import picodroid.widget.ListView
import picodroid.widget.TextView

/**
 * Activity 1 (HomeActivity/LiveActivity shape): lateinit, lazy, anonymous
 * objects for the two-method SDK interfaces, SAM lambdas for the one-method
 * ones, `!!`/`?.`/`?:`/`as?`, and `when` over a String.
 */
class HomeActivity : Activity() {
    private lateinit var status: TextView
    private var svc: SensorService? = null
    private var mode: String = "idle"
    private val prefs by lazy { getSharedPreferences("survey", Context.MODE_PRIVATE) }

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(binder: IBinder) {
            svc = (binder as? SensorService.LocalBinder)?.service
            status.setText("connected: ${svc?.latest()}")
        }

        override fun onServiceDisconnected() {
            svc = null
        }
    }

    private val rawListener = object : SensorEventListener {
        override fun onSensorChanged(event: SensorEvent) {
            status.setText("raw ${event.sensor.type}=${event.values[0]}")
        }

        override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {
            Log.d(SurveyApp.TAG, "accuracy $accuracy for ${sensor.name}")
        }
    }

    override fun onCreate() {
        super.onCreate()
        mode = intent?.getStringExtra("mode") ?: "idle"
        val root = LinearLayout().apply { setOrientation(LinearLayout.VERTICAL) }
        status = TextView().also { it.setText("mode=$mode limit=${prefs.getInt("limit", 60)}") }
        root.addView(status)
        val refresh = Button("Refresh")
        refresh.setOnClickListener { refresh() }
        root.addView(refresh)
        val menu = ListView()
        menu.addItem("Live")
        menu.addItem("History")
        menu.setOnItemClickListener { _, _, position, _ -> select(position) }
        root.addView(menu)
        val toggle = CheckBox()
        toggle.setOnCheckedChangeListener { _, checked -> setLogging(checked) }
        root.addView(toggle)
        setContentView(root)

        bindService(Intent(SensorService::class.java), connection)
        val sm = SensorManager.getInstance()
        sm.getDefaultSensor(Sensor.TYPE_LIGHT)?.let { sm.registerListener(rawListener, it, SensorManager.SENSOR_DELAY_NORMAL) }

        Thread { poll() }.start()
        Thread(Runnable { poll() }).start()
        Executors.mainExecutor().execute { render() }

        when (mode) {
            "live" -> Log.i(SurveyApp.TAG, "live mode")
            "history" -> startActivity(Intent(HistoryActivity::class.java).putExtra("idx", 3))
            else -> Log.w(SurveyApp.TAG, "unknown mode $mode")
        }
    }

    private fun refresh() {
        val s = svc!!
        status.setText("latest ${s.latest()}")
    }

    private fun select(position: Int) {
        if (position == 1) {
            startActivity(Intent(HistoryActivity::class.java))
        } else {
            AlertDialog.Builder()
                .setTitle("Live")
                .setMessage("pos=$position")
                .setPositiveButton("OK") { _, which -> Log.i(SurveyApp.TAG, "ok $which") }
                .show()
        }
    }

    private fun setLogging(enabled: Boolean) {
        svc?.setLogging(enabled)
    }

    private fun poll() {
        Log.d(SurveyApp.TAG, "poll ${svc?.latest() ?: -1f}")
    }

    private fun render() {
        status.setText(svc?.describe() ?: "no service")
    }

    override fun onDestroy() {
        SensorManager.getInstance().unregisterListener(rawListener)
        unbindService(connection)
        super.onDestroy()
    }
}
