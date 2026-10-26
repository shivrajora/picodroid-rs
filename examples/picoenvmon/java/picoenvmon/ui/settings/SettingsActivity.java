// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.settings;

import javax.inject.Inject;
import picodroid.graphics.Theme;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.NumberPicker;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.Toast;
import picoenvmon.EnvApp;
import picoenvmon.R;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.util.Formatter;

/**
 * Threshold + units editor (reached from the Home hub). Three focusable {@link NumberPicker} rows,
 * a focusable °C/°F {@link Switch}, and an explicit Save {@link Button}. Under the standardized
 * model A/B move focus between controls and X (ENTER) activates the focused one. Activating a
 * picker enters keypad edit mode (secondary-color outline): A/B then step the value by the row's
 * step size instead of moving focus, X commits, and Y leaves edit mode without leaving the screen.
 * Outside edit mode Y returns to the hub; Save persists to the app's preferences and finishes.
 */
public class SettingsActivity extends NavActivity {

  @Inject ThresholdConfig thresholds;
  @Inject Formatter formatter;
  private NumberPicker tempField;
  private NumberPicker humField;
  private NumberPicker luxField;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(EnvApp.TAG, "Settings.onCreate");
    getDisplay();

    int gap = dimen(R.dimen.gap);
    LinearLayout root = makeScreenRoot();
    addTitle(root, R.string.menu_settings);

    ThresholdConfig th = thresholds;
    tempField = addRow(root, R.string.settings_temp_hi, 0, 60, 1, th.tempHiCentiC / 100, gap);
    humField = addRow(root, R.string.settings_hum_lo, 0, 100, 1, th.humLoMilliPct / 1000, gap);
    luxField = addRow(root, R.string.settings_lux_lo, 0, 10000, 10, th.luxLo, gap);

    root.addView(buildUnitsRow(), stacked(dimen(R.dimen.switch_row_height), gap));

    Button save = new Button(this);
    save.setText(getString(R.string.save));
    save.setOnClickListener(v -> commit());
    LinearLayout.LayoutParams saveLp =
        new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT);
    saveLp.topMargin = gap;
    root.addView(save, saveLp);

    installHintBar(root, R.string.hint_settings, gap);

    setContentView(root);
  }

  private LinearLayout buildUnitsRow() {
    LinearLayout row = new LinearLayout(this);
    row.setOrientation(LinearLayout.HORIZONTAL);
    // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
    // headroom or it clips top/bottom: zero vertical padding plus the row's height give the full
    // circle room to render.
    row.setPadding(4, 0, 4, 0);

    TextView label = new TextView(this);
    label.setText(getString(R.string.settings_units));
    label.setTextColor(Theme.colorTextSecondary);
    // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
    // size (android:layout_weight="1"). WRAP_CONTENT height lets the row's cross-axis
    // centering center the glyphs — a fixed-height label draws its text top-aligned.
    row.addView(
        label, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

    Switch units = new Switch(this);
    units.setChecked(formatter.isFahrenheit());
    units.setOnCheckedChangeListener((buttonView, isChecked) -> formatter.setFahrenheit(isChecked));
    row.addView(units);

    return row;
  }

  private NumberPicker addRow(
      LinearLayout root, int labelRes, int min, int max, int step, int initialValue, int gap) {
    LinearLayout row = new LinearLayout(this);
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setPadding(4, 2, 4, 2);

    TextView lbl = new TextView(this);
    lbl.setText(getString(labelRes));
    lbl.setTextColor(Theme.colorTextSecondary);
    // The label takes what the picker leaves (layout_weight="1"), so the pickers line up on the
    // right edge on every panel; WRAP_CONTENT height lets the row center the glyphs.
    row.addView(lbl, new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f));

    NumberPicker field = new NumberPicker(this);
    field.setMinValue(min);
    field.setMaxValue(max);
    field.setStep(step);
    field.setValue(initialValue);
    row.addView(
        field,
        new LinearLayout.LayoutParams(dimen(R.dimen.picker_width), dimen(R.dimen.picker_height)));

    root.addView(row, stacked(dimen(R.dimen.picker_row_height), gap));
    return field;
  }

  private void commit() {
    ThresholdConfig th = thresholds;
    th.tempHiCentiC = tempField.getValue() * 100;
    th.humLoMilliPct = humField.getValue() * 1000;
    th.luxLo = luxField.getValue();
    boolean ok = th.save(getSharedPreferences(EnvApp.PREFS_NAME, MODE_PRIVATE));
    Log.i(
        EnvApp.TAG,
        "Settings saved: tempHi="
            + th.tempHiCentiC
            + " humLo="
            + th.humLoMilliPct
            + " luxLo="
            + th.luxLo
            + " ok="
            + ok);
    Toast.makeText(this, getString(ok ? R.string.saved : R.string.save_failed), Toast.LENGTH_SHORT)
        .show();
    finish();
  }
}
