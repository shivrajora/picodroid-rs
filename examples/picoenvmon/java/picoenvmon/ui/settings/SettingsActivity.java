// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.settings;

import picodroid.app.Activity;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.view.inputmethod.EditorInfo;
import picodroid.widget.EditText;
import picodroid.widget.LinearLayout;
import picodroid.widget.OnEditorActionListener;
import picodroid.widget.TextView;
import picodroid.widget.Toast;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.di.EnvActivityComponent;
import picoenvmon.di.EnvAppComponent;

/**
 * Threshold-entry screen. Three EditTexts wired to a single OnEditorActionListener —
 * IME_ACTION_DONE commits the values to Preferences and finishes the screen.
 */
public class SettingsActivity extends Activity {

  private EnvActivityComponent comp;
  private EditText tempField;
  private EditText humField;
  private EditText luxField;

  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Settings.onCreate");
    comp = new EnvActivityComponent();
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(8, 6, 8, 6);
    root.setBackgroundColor(Theme.colorBackground);

    GradientDrawable card = new GradientDrawable();
    card.setColor(Theme.colorSurface).setCornerRadius(6).setStroke(1, Theme.colorOutline);

    TextView title = new TextView();
    title.setText("Thresholds");
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

    TextView footer = new TextView();
    footer.setText("Tap a field; OK = save");
    footer.setTextColor(Theme.colorTextSecondary);
    root.addView(footer);

    setContentView(root);
  }

  private EditText addRow(LinearLayout root, String label, int initialValue) {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(224, 32);
    row.setPadding(4, 2, 4, 2);

    TextView lbl = new TextView();
    lbl.setText(label);
    lbl.setTextColor(Theme.colorTextSecondary);
    lbl.setSize(110, 28);
    row.addView(lbl);

    EditText field = new EditText();
    field.setSize(108, 28);
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
    Toast.makeText(ok ? "Saved" : "Save failed", Toast.LENGTH_SHORT).show();
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
