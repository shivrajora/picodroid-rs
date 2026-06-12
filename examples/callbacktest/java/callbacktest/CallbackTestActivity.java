// SPDX-License-Identifier: GPL-3.0-only
package callbacktest;

import picodroid.app.Activity;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.AdapterView;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.LinearLayout;
import picodroid.widget.SeekBar;
import picodroid.widget.Spinner;
import picodroid.widget.Switch;
import picodroid.widget.Toast;
import picodroid.widget.ToggleButton;

/**
 * End-to-end regression test for the widget-callback dispatch path.
 *
 * <p>Registers a lambda listener on each widget, then synthetically fires the corresponding LVGL
 * event via {@code performClick()} / {@code performCheckedChange()} / etc. Each listener prints a
 * unique token via {@code Log.i}; {@code scripts/hil-tests.conf} asserts every token appears in
 * stdout. Runs under both shrink modes via {@code scripts/sim-run.sh}.
 *
 * <p>Lambdas (not named {@code Runnable} classes) are deliberate — they exercise the same
 * invokedynamic → field-stored proxy → cross-execute dispatch path that the shrink bug fixed in
 * eba57c3 killed.
 */
public class CallbackTestActivity extends Activity {
  @Override
  public void onCreate() {
    // Force display init. getDisplay() lazily brings up the LVGL engine; widget
    // constructors below parent to the screen and crash if it's still null.
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);

    Button btn = new Button("b");
    btn.setSize(100, 30);
    btn.setOnClickListener(v -> Log.i("CBT", "BUTTON"));
    root.addView(btn);
    btn.performClick();

    ToggleButton tog = new ToggleButton("on", "off");
    tog.setSize(100, 30);
    tog.setOnCheckedChangeListener((view, checked) -> Log.i("CBT", "TOGGLE"));
    root.addView(tog);
    tog.performCheckedChange();

    Switch sw = new Switch();
    sw.setSize(60, 30);
    sw.setOnCheckedChangeListener((view, checked) -> Log.i("CBT", "SWITCH"));
    root.addView(sw);
    sw.performCheckedChange();

    CheckBox cb = new CheckBox();
    cb.setText("x");
    cb.setOnCheckedChangeListener((view, checked) -> Log.i("CBT", "CHECKBOX"));
    root.addView(cb);
    cb.performCheckedChange();

    SeekBar sb = new SeekBar(100);
    sb.setSize(200, 20);
    sb.setOnSeekBarChangeListener((bar, progress, fromUser) -> Log.i("CBT", "SEEKBAR"));
    root.addView(sb);
    sb.performProgressChange();

    Spinner sp = new Spinner();
    sp.setItems("a\nb");
    sp.setSize(100, 30);
    sp.setOnItemSelectedListener(
        new AdapterView.OnItemSelectedListener() {
          @Override
          public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
            Log.i("CBT", "SPINNER");
          }

          @Override
          public void onNothingSelected(AdapterView<?> parent) {}
        });
    root.addView(sp);
    sp.performItemSelected();

    Toast toast = Toast.makeText(this, "cbt", Toast.LENGTH_SHORT);
    toast.setDuration(Toast.LENGTH_LONG);
    if (toast.getDuration() == Toast.LENGTH_LONG) {
      Log.i("CBT", "TOAST_DURATION");
    }

    setContentView(root);
    Log.i("CBT", "SETUP_DONE");
  }
}
