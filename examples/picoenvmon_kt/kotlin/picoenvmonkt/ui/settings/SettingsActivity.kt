// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.settings

import javax.inject.Inject
import picodroid.content.SharedPreferences
import picodroid.graphics.Theme
import picodroid.graphics.drawable.GradientDrawable
import picodroid.util.Log
import picodroid.view.View
import picodroid.widget.Button
import picodroid.widget.LinearLayout
import picodroid.widget.NumberPicker
import picodroid.widget.Switch
import picodroid.widget.TextView
import picodroid.widget.Toast
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
 * returns to the hub; Save persists and finishes.
 */
class SettingsActivity : NavActivity() {
    @Inject lateinit var thresholds: ThresholdConfig
    @Inject lateinit var formatter: Formatter
    @Inject lateinit var prefs: SharedPreferences
    private var tempField: NumberPicker? = null
    private var humField: NumberPicker? = null
    private var luxField: NumberPicker? = null

    override fun onCreate() {
        Log.i(TAG, "Settings.onCreate")
        getDisplay()

        val root = makeScreenRoot()
        root.setSpacing(4)

        val title = TextView()
        title.setText("Settings")
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)

        val th = thresholds
        tempField = addRow(root, "Temp Hi °C", 0, 60, 1, th.tempHiCentiC / 100)
        humField = addRow(root, "Hum Lo %", 0, 100, 1, th.humLoMilliPct / 1000)
        luxField = addRow(root, "Lux Lo", 0, 10000, 10, th.luxLo)

        root.addView(buildUnitsRow())
        root.addView(buildSaveButton())

        // Keep this the same length as the other screens' hints so the whole
        // legend fits the 224 px ButtonHintBar (the longer "X:Edit/Save" clipped
        // "Y:Back" to "Y:B"). The Save button is self-labelled, so "X:Edit" is
        // enough; while editing, the same A/B keys read naturally as value
        // up/down and the secondary outline marks the mode.
        installHintBar(root, "A:Up  B:Down  X:Edit  Y:Back")

        setContentView(root)
    }

    private fun buildUnitsRow(): LinearLayout {
        val row = LinearLayout()
        row.setOrientation(LinearLayout.HORIZONTAL)
        // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
        // headroom or it clips top/bottom. Two things steal that headroom: the row's 2 px theme
        // border and its padding. A borderless background (stroke 0) zeroes the border, and zero
        // vertical padding plus a 34 px height give the full circle room to render.
        row.setSize(224, 34)
        row.setPadding(4, 0, 4, 0)
        row.setBackground(GradientDrawable().setColor(Theme.colorBackground))

        val label = TextView()
        label.setText("Units °F")
        label.setTextColor(Theme.colorTextSecondary)
        // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
        // size (mirrors android:layout_weight="1"), so the switch is no longer clipped.
        // WRAP_CONTENT height lets the row's flex cross-axis centering center the glyphs —
        // a fixed-height label draws its text top-aligned inside its own box.
        row.addView(label, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        val units = Switch()
        units.setChecked(formatter.isFahrenheit)
        units.setOnCheckedChangeListener { _, isChecked -> formatter.isFahrenheit = isChecked }
        row.addView(units)

        return row
    }

    private fun buildSaveButton(): Button {
        val saveButton = Button("Save")
        saveButton.setOnClickListener { commit() }
        return saveButton
    }

    private fun addRow(
        root: LinearLayout,
        label: String,
        min: Int,
        max: Int,
        step: Int,
        initialValue: Int,
    ): NumberPicker {
        val row = LinearLayout()
        row.setOrientation(LinearLayout.HORIZONTAL)
        row.setSize(224, 30)
        row.setPadding(4, 2, 4, 2)

        val lbl = TextView()
        lbl.setText(label)
        lbl.setTextColor(Theme.colorTextSecondary)
        // Fixed width keeps the value column aligned; WRAP_CONTENT height lets the
        // row's flex cross-axis centering actually center the glyphs.
        lbl.setSize(110, View.WRAP_CONTENT)
        row.addView(lbl)

        val field = NumberPicker()
        field.setSize(108, 26)
        field.setMinValue(min)
        field.setMaxValue(max)
        field.setStep(step)
        field.setValue(initialValue)
        row.addView(field)

        root.addView(row)
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
        val ok = th.save(prefs)
        Log.i(
            TAG,
            "Settings saved: tempHi=${th.tempHiCentiC} humLo=${th.humLoMilliPct} luxLo=${th.luxLo} ok=$ok",
        )
        Toast.makeText(this, if (ok) "Saved" else "Save failed", Toast.LENGTH_SHORT).show()
        finish()
    }
}
