// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.settings

import javax.inject.Inject
import picodroid.content.Context
import picodroid.graphics.Theme
import picodroid.os.Bundle
import picodroid.util.Log
import picodroid.widget.Button
import picodroid.widget.LinearLayout
import picodroid.widget.NumberPicker
import picodroid.widget.Switch
import picodroid.widget.TextView
import picodroid.widget.Toast
import picoenvmon_kt.R
import picoenvmonkt.PREFS_NAME
import picoenvmonkt.TAG
import picoenvmonkt.data.ThresholdConfig
import picoenvmonkt.ui.common.NavActivity
import picoenvmonkt.util.Formatter

/**
 * Threshold + units editor (reached from the Home hub). Three focusable [NumberPicker] rows, a
 * focusable °C/°F [Switch], and an explicit Save [Button]. Under the standardized model A/B move
 * focus between controls and X (ENTER) activates the focused one. Activating a picker enters keypad
 * edit mode (secondary-color outline): A/B then step the value by the row's step size instead of
 * moving focus, X commits, and Y leaves edit mode without leaving the screen. Outside edit mode Y
 * returns to the hub; Save persists to the app's preferences and finishes.
 */
class SettingsActivity : NavActivity() {
    @Inject lateinit var thresholds: ThresholdConfig
    @Inject lateinit var formatter: Formatter
    private var tempField: NumberPicker? = null
    private var humField: NumberPicker? = null
    private var luxField: NumberPicker? = null

    override fun onCreate(savedInstanceState: Bundle?) {

        super.onCreate(savedInstanceState)
        Log.i(TAG, "Settings.onCreate")
        getDisplay()

        val gap = dimen(R.dimen.gap)
        val root = makeScreenRoot()
        addTitle(root, R.string.menu_settings)

        val th = thresholds
        tempField = addRow(root, R.string.settings_temp_hi, 0, 60, 1, th.tempHiCentiC / 100, gap)
        humField = addRow(root, R.string.settings_hum_lo, 0, 100, 1, th.humLoMilliPct / 1000, gap)
        luxField = addRow(root, R.string.settings_lux_lo, 0, 10000, 10, th.luxLo, gap)

        root.addView(buildUnitsRow(), stacked(dimen(R.dimen.switch_row_height), gap))

        val save = Button(this)
        save.setText(getString(R.string.save))
        save.setOnClickListener { commit() }
        val saveLp =
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.WRAP_CONTENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            )
        saveLp.topMargin = gap
        root.addView(save, saveLp)

        installHintBar(root, R.string.hint_settings, gap)

        setContentView(root)
    }

    private fun buildUnitsRow(): LinearLayout {
        val row = LinearLayout(this)
        row.setOrientation(LinearLayout.HORIZONTAL)
        // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
        // headroom or it clips top/bottom: zero vertical padding plus the row's height give the
        // full circle room to render.
        row.setPadding(4, 0, 4, 0)

        val label = TextView(this)
        label.setText(getString(R.string.settings_units))
        label.setTextColor(Theme.colorTextSecondary)
        // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
        // size (android:layout_weight="1"). WRAP_CONTENT height lets the row's cross-axis
        // centering center the glyphs — a fixed-height label draws its text top-aligned.
        row.addView(label, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        val units = Switch(this)
        units.setChecked(formatter.isFahrenheit)
        units.setOnCheckedChangeListener { _, isChecked -> formatter.isFahrenheit = isChecked }
        row.addView(units)

        return row
    }

    private fun addRow(
        root: LinearLayout,
        labelRes: Int,
        min: Int,
        max: Int,
        step: Int,
        initialValue: Int,
        gap: Int,
    ): NumberPicker {
        val row = LinearLayout(this)
        row.setOrientation(LinearLayout.HORIZONTAL)
        row.setPadding(4, 2, 4, 2)

        val lbl = TextView(this)
        lbl.setText(getString(labelRes))
        lbl.setTextColor(Theme.colorTextSecondary)
        // The label takes what the picker leaves (layout_weight="1"), so the pickers line up on
        // the right edge on every panel; WRAP_CONTENT height lets the row center the glyphs.
        row.addView(lbl, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        val field = NumberPicker(this)
        field.setMinValue(min)
        field.setMaxValue(max)
        field.setStep(step)
        field.setValue(initialValue)
        row.addView(
            field,
            LinearLayout.LayoutParams(dimen(R.dimen.picker_width), dimen(R.dimen.picker_height)),
        )

        root.addView(row, stacked(dimen(R.dimen.picker_row_height), gap))
        return field
    }

    private fun commit() {
        val temp = tempField ?: return
        val hum = humField ?: return
        val lux = luxField ?: return
        val th = thresholds
        th.tempHiCentiC = temp.getValue() * 100
        th.humLoMilliPct = hum.getValue() * 1000
        th.luxLo = lux.getValue()
        val ok = th.save(getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE))
        Log.i(
            TAG,
            "Settings saved: tempHi=${th.tempHiCentiC} humLo=${th.humLoMilliPct} luxLo=${th.luxLo} ok=$ok",
        )
        Toast.makeText(
                this,
                getString(if (ok) R.string.saved else R.string.save_failed),
                Toast.LENGTH_SHORT,
            )
            .show()
        finish()
    }
}
