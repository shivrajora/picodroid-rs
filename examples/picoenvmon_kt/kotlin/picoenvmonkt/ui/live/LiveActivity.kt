// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.live

import javax.inject.Inject
import picodroid.content.ComponentName
import picodroid.content.Intent
import picodroid.content.ServiceConnection
import picodroid.graphics.Theme
import picodroid.graphics.drawable.GradientDrawable
import picodroid.hardware.Sensor
import picodroid.os.Bundle
import picodroid.os.IBinder
import picodroid.util.Log
import picodroid.view.View
import picodroid.widget.FrameLayout
import picodroid.widget.GridLayout
import picodroid.widget.LinearLayout
import picodroid.widget.Switch
import picodroid.widget.TextView
import picoenvmonkt.IDX_GAS
import picoenvmonkt.IDX_HUMIDITY
import picoenvmonkt.IDX_LIGHT
import picoenvmonkt.IDX_PRESSURE
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.READING_COUNT
import picoenvmonkt.TAG
import picoenvmonkt.data.ThresholdConfig
import picoenvmonkt.service.SensorLoggerService
import picoenvmonkt.service.SmoothedSensorListener
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.Formatter

/**
 * Live dashboard (reached from the Home hub). Binds [SensorLoggerService] for 1 Hz smoothed sensor
 * callbacks that feed 5 tiles, and exposes a focusable Logger [Switch] that toggles the foreground
 * logging service — the single interactive control, so under the standardized model X (ENTER) on
 * the focused Switch toggles it and Y returns to the hub. (The old touch-long-press °C↔°F toggle is
 * gone; units now live in Settings.) The two-method SDK interfaces are class supertypes, exactly as
 * in Java — only single-method interfaces SAM-convert to lambdas.
 */
class LiveActivity : NavActivity(), ServiceConnection, SmoothedSensorListener {
    @Inject lateinit var formatter: Formatter
    @Inject lateinit var thresholds: ThresholdConfig
    private var service: SensorLoggerService? = null
    private var serviceRunning = false
    private var loggerSwitch: Switch? = null

    private val tileRoots = arrayOfNulls<View>(READING_COUNT)
    private val tileValues = arrayOfNulls<TextView>(READING_COUNT)

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "Live.onCreate")
        getDisplay()

        val root = makeScreenRoot()
        root.setSpacing(4)

        val title = TextView()
        title.setText("Live")
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)

        // One GradientDrawable shared across all 5 tile backgrounds (same color/radius/stroke).
        val tileBg = GradientDrawable()
        tileBg.setColor(Theme.colorSurface).setCornerRadius(6).setStroke(1, Theme.colorOutline)

        // The five tiles as one two-column grid: the label column is as wide as its widest
        // label, the value column takes the rest, and the columns line up across the tiles
        // without a fixed width anywhere.
        val tiles = GridLayout()
        tiles.setColumnCount(2)
        tiles.setSpacing(4)
        buildTile(tiles, IDX_TEMPERATURE, "Temp", tileBg)
        buildTile(tiles, IDX_HUMIDITY, "Humidity", tileBg)
        buildTile(tiles, IDX_PRESSURE, "Pressure", tileBg)
        buildTile(tiles, IDX_GAS, "Air quality", tileBg)
        buildTile(tiles, IDX_LIGHT, "Light", tileBg)
        root.addView(
            tiles,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            ),
        )

        // Focusable Logger toggle (the old Home X=toggle-logger action). It is the only group-def
        // widget on the screen, so it auto-focuses; X toggles it.
        root.addView(buildLoggerRow())

        installHintBar(root, "A/B:Move  X:Toggle  Y:Back")

        setContentView(root)

        bindService(Intent(SensorLoggerService::class.java), this, BIND_AUTO_CREATE)
    }

    override fun onDestroy() {
        service?.removeSmoothedListener(this)
        try {
            unbindService(this)
        } catch (t: Throwable) {
            Log.i(TAG, "Live unbind ignored: $t")
        }
    }

    override fun onServiceConnected(name: ComponentName, binder: IBinder) {
        val svc = (binder as SensorLoggerService.LocalBinder).service ?: return
        service = svc
        svc.addSmoothedListener(this)
        // The bound Service is the source of truth for the logging state (Android LocalBinder
        // pattern).
        // A *started* logging Service outlives the unbind we do in onDestroy, so re-entering the
        // Live
        // screen reconnects to a service that may already be logging while the freshly built Switch
        // still shows its default (off). Reflect the real state onto the toggle here.
        //
        // Update the model field *before* the widget: android.widget.CompoundButton#setChecked
        // fires
        // OnCheckedChangeListener, so setting serviceRunning first makes setLogger's
        // `if (on == serviceRunning)` guard absorb the re-entrant callback — no spurious
        // start/stop.
        val logging = svc.isLogging
        serviceRunning = logging
        loggerSwitch?.setChecked(logging)
    }

    override fun onServiceDisconnected(name: ComponentName) {
        service = null
    }

    private fun buildLoggerRow(): LinearLayout {
        val row = LinearLayout()
        row.setOrientation(LinearLayout.HORIZONTAL)
        // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
        // headroom or it clips top/bottom. Two things steal that headroom: the row's 2 px theme
        // border and its padding. A borderless background (stroke 0) zeroes the border, and zero
        // vertical padding plus a 34 px height give the full circle room to render.
        row.setSize(LinearLayout.LayoutParams.MATCH_PARENT, 34)
        row.setPadding(8, 0, 8, 0)
        row.setBackground(GradientDrawable().setColor(Theme.colorBackground))

        val label = TextView()
        label.setText("Logger")
        label.setTextColor(Theme.colorTextSecondary)
        // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
        // size (mirrors android:layout_weight="1"), so the switch is no longer clipped.
        // WRAP_CONTENT height lets the row's flex cross-axis centering center the glyphs —
        // a fixed-height label draws its text top-aligned inside its own box.
        row.addView(label, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        val toggle = Switch()
        toggle.setOnCheckedChangeListener { _, isChecked -> setLogger(isChecked) }
        row.addView(toggle)
        loggerSwitch = toggle

        return row
    }

    private fun setLogger(on: Boolean) {
        if (on == serviceRunning) {
            return
        }
        val svc = Intent(SensorLoggerService::class.java)
        if (on) {
            startService(svc)
        } else {
            stopService(svc)
        }
        serviceRunning = on
        Log.i(TAG, "Logger ${if (on) "started" else "stopped"}")
    }

    /**
     * One row of the grid: the card spans both columns behind a label in the first and a value in
     * the second, each centred on the row. Cells may overlap, as on Android; the card is added
     * first so it draws under the text.
     */
    private fun buildTile(grid: GridLayout, idx: Int, label: String, bg: GradientDrawable) {
        val card = FrameLayout()
        card.setBackground(bg)
        val cardCell =
            GridLayout.LayoutParams(GridLayout.spec(idx), GridLayout.spec(0, 2, GridLayout.FILL))
        cardCell.height = 26
        grid.addView(card, cardCell)

        val labelView = TextView()
        labelView.setText(label)
        labelView.setTextColor(Theme.colorTextSecondary)
        val labelCell =
            GridLayout.LayoutParams(
                GridLayout.spec(idx, GridLayout.CENTER),
                GridLayout.spec(0, GridLayout.LEFT),
            )
        labelCell.setMargins(8, 0, 0, 0)
        grid.addView(labelView, labelCell)

        val valueView = TextView()
        // ASCII "--" placeholder: the bundled LVGL Montserrat subset has no em-dash
        // (U+2014) glyph, so "—" renders as a missing-glyph box.
        valueView.setText("--")
        valueView.setTextColor(Theme.colorText)
        val valueCell =
            GridLayout.LayoutParams(
                GridLayout.spec(idx, GridLayout.CENTER),
                GridLayout.spec(1, GridLayout.LEFT, 1f),
            )
        valueCell.setMargins(8, 0, 8, 0)
        grid.addView(valueView, valueCell)

        tileRoots[idx] = card
        tileValues[idx] = valueView
    }

    override fun onSmoothedSensor(sensorType: Int, value: Float) {
        val f = formatter
        when (sensorType) {
            Sensor.TYPE_AMBIENT_TEMPERATURE -> {
                tileValues[IDX_TEMPERATURE]?.setText(f.formatTemp(value))
                flashOnBreach(tileRoots[IDX_TEMPERATURE], thresholds.tempBreached(value))
            }
            Sensor.TYPE_RELATIVE_HUMIDITY -> {
                tileValues[IDX_HUMIDITY]?.setText(f.formatHumidity(value))
                flashOnBreach(tileRoots[IDX_HUMIDITY], thresholds.humidityBreached(value))
            }
            Sensor.TYPE_PRESSURE -> tileValues[IDX_PRESSURE]?.setText(f.formatPressure(value))
            Sensor.TYPE_GAS_RESISTANCE -> tileValues[IDX_GAS]?.setText(f.formatGasIaq(value))
            Sensor.TYPE_LIGHT -> {
                tileValues[IDX_LIGHT]?.setText(f.formatLux(value))
                flashOnBreach(tileRoots[IDX_LIGHT], thresholds.luxBreached(value))
            }
            else -> {}
        }
    }

    private fun flashOnBreach(tile: View?, breached: Boolean) {
        if (!breached || tile == null) {
            return
        }
        // Dip, then restore once the dip has finished. Two back-to-back alpha()
        // starts on the same view would replace each other; chaining through
        // withEndAction is the Android idiom.
        tile
            .animate()
            .alpha(0.35f)
            .setDuration(180)
            .withEndAction { tile.animate().alpha(1f).setDuration(360).start() }
            .start()
    }
}
