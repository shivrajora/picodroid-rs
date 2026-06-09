// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.settings;

import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.InputType;
import picodroid.util.Log;
import picodroid.view.inputmethod.EditorInfo;
import picodroid.widget.Button;
import picodroid.widget.EditText;
import picodroid.widget.LinearLayout;
import picodroid.widget.OnEditorActionListener;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.Toast;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.ui.common.NavActivity;

/**
 * Threshold + units editor (reached from the Home hub). Three focusable {@link EditText} rows, a
 * focusable °C/°F {@link Switch} (the new home for the units toggle, replacing the dead touch
 * long-press), and an explicit Save {@link Button}. Under the standardized model A/B move focus
 * between controls, X (ENTER) activates the focused one — open the keyboard on a field, flip the
 * Switch, or commit on Save — and Y returns to the hub. IME DONE still commits as before.
 */
public class SettingsActivity extends NavActivity {

  private EnvActivityComponent comp;
  private EditText tempField;
  private EditText humField;
  private EditText luxField;

  @Override
  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Settings.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = makeScreenRoot();
    root.setSpacing(4);

    TextView title = new TextView();
    title.setText("Settings");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    ThresholdConfig th = comp.thresholds();
    tempField = addRow(root, "Temp Hi °C", th.tempHiCentiC / 100);
    humField = addRow(root, "Hum Lo %", th.humLoMilliPct / 1000);
    luxField = addRow(root, "Lux Lo", th.luxLo);

    OnEditorActionListener save =
        (v, actionId, ev) -> {
          if (actionId == EditorInfo.IME_ACTION_DONE) {
            commit();
            return true;
          }
          return false;
        };
    tempField.setOnEditorActionListener(save);
    humField.setOnEditorActionListener(save);
    luxField.setOnEditorActionListener(save);

    root.addView(buildUnitsRow());
    root.addView(buildSaveButton());

    // Keep this the same length as the other screens' hints so the whole
    // legend fits the 224 px ButtonHintBar (the longer "X:Edit/Save" clipped
    // "Y:Back" to "Y:B"). The Save button is self-labelled, so "X:Edit" is enough.
    installHintBar(root, "A:Up  B:Down  X:Edit  Y:Back");

    setContentView(root);
  }

  private LinearLayout buildUnitsRow() {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    // The Switch knob is drawn ~4 px larger than its track (LVGL theme), so it needs vertical
    // headroom or it clips top/bottom. Two things steal that headroom: the row's 2 px theme
    // border and its padding. A borderless background (stroke 0) zeroes the border, and zero
    // vertical padding plus a 34 px height give the full circle room to render.
    row.setSize(224, 34);
    row.setPadding(4, 0, 4, 0);
    row.setBackground(new GradientDrawable().setColor(Theme.colorBackground));

    TextView label = new TextView();
    label.setText("Units °F");
    label.setTextColor(Theme.colorTextSecondary);
    // weight=1 lets the label fill the row and pushes the Switch flush right at its natural
    // size (mirrors android:layout_weight="1"), so the switch is no longer clipped.
    row.addView(label, new LinearLayout.LayoutParams(0, 24, 1f));

    Switch units = new Switch();
    units.setChecked(comp.formatter().isFahrenheit());
    units.setOnCheckedChangeListener(
        (buttonView, isChecked) -> comp.formatter().setFahrenheit(isChecked));
    row.addView(units);

    return row;
  }

  private Button buildSaveButton() {
    Button saveButton = new Button("Save");
    saveButton.setOnClickListener(v -> commit());
    return saveButton;
  }

  private EditText addRow(LinearLayout root, String label, int initialValue) {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(224, 30);
    row.setPadding(4, 2, 4, 2);

    TextView lbl = new TextView();
    lbl.setText(label);
    lbl.setTextColor(Theme.colorTextSecondary);
    lbl.setSize(110, 26);
    row.addView(lbl);

    EditText field = new EditText();
    field.setSize(108, 26);
    // Threshold rows are all integers — give them the digit-pad keyboard.
    field.setInputType(InputType.TYPE_CLASS_NUMBER);
    field.setText(Integer.toString(initialValue));
    row.addView(field);

    root.addView(row);
    return field;
  }

  private void commit() {
    ThresholdConfig th = comp.thresholds();
    int tempC = parseOr(tempField.getText(), th.tempHiCentiC / 100);
    int humPct = parseOr(humField.getText(), th.humLoMilliPct / 1000);
    int lux = parseOr(luxField.getText(), th.luxLo);
    th.tempHiCentiC = tempC * 100;
    th.humLoMilliPct = humPct * 1000;
    th.luxLo = lux;
    boolean ok = th.save(comp.appComponent().prefs());
    Log.i(
        EnvAppComponent.TAG,
        "Settings saved: tempHi="
            + th.tempHiCentiC
            + " humLo="
            + th.humLoMilliPct
            + " luxLo="
            + th.luxLo
            + " ok="
            + ok);
    Toast.makeText(this, ok ? "Saved" : "Save failed", Toast.LENGTH_SHORT).show();
    finish();
  }

  private static int parseOr(String s, int fallback) {
    if (s == null || s.length() == 0) {
      return fallback;
    }
    int sign = 1;
    int start = 0;
    if (s.charAt(0) == '-') {
      sign = -1;
      start = 1;
    }
    int n = 0;
    for (int i = start; i < s.length(); i++) {
      char c = s.charAt(i);
      if (c < '0' || c > '9') {
        return fallback;
      }
      n = n * 10 + (c - '0');
    }
    return n * sign;
  }
}
