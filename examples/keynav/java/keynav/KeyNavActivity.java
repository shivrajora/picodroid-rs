// SPDX-License-Identifier: GPL-3.0-only
package keynav;

import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.AdapterView;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.SeekBar;
import picodroid.widget.Spinner;
import picodroid.widget.TextView;
import picodroid.widget.TimePicker;

/**
 * Every stock value widget driven by the four keys (docs/designs/app-portability-2026-10.md K7),
 * from the sim's control channel (test.ctrl): a SeekBar, a Spinner and a TimePicker edited with
 * SELECT then UP/DOWN, a ScrollView of plain text scrolled by UP/DOWN, a list dialog walked with
 * UP/DOWN and picked with SELECT, and a plain clickable TextView that the keys reach because a
 * click listener makes a view focusable on a board with navigation keys. Each change is logged.
 */
public class KeyNavActivity extends Activity {
  private static final String TAG = "KeyNav";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    // The window, not the default 160x160, which clipped the last three stops out of sight on
    // every board; and the column fits a 240-tall panel (QA round 2, R5).
    root.setSize(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);

    SeekBar seek = new SeekBar(this);
    seek.setMax(100);
    seek.setProgress(50);
    seek.setLayoutParams(new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 20));
    seek.setOnSeekBarChangeListener(
        new SeekBar.OnSeekBarChangeListener() {
          @Override
          public void onProgressChanged(SeekBar bar, int progress, boolean fromUser) {
            Log.i(TAG, "seek " + progress);
          }

          @Override
          public void onStartTrackingTouch(SeekBar bar) {}

          @Override
          public void onStopTrackingTouch(SeekBar bar) {}
        });
    root.addView(seek);

    Spinner spinner = new Spinner(this);
    spinner.setAdapter(new ArrayAdapter<>(this, new String[] {"Red", "Green", "Blue"}));
    spinner.setOnItemSelectedListener(
        new AdapterView.OnItemSelectedListener() {
          @Override
          public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
            Log.i(TAG, "spinner " + position);
          }

          @Override
          public void onNothingSelected(AdapterView<?> parent) {}
        });
    root.addView(spinner);

    TimePicker time = new TimePicker(this);
    time.setIs24HourView(true);
    time.setTime(12, 30);
    time.setOnTimeChangedListener(
        (picker, hour, minute) -> Log.i(TAG, "time " + hour + ":" + minute));
    root.addView(time);

    // Plain text, no focusable inside: the keys scroll it.
    ScrollView scroll = new ScrollView(this);
    scroll.setLayoutParams(new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 28));
    LinearLayout lines = new LinearLayout(this);
    lines.setOrientation(LinearLayout.VERTICAL);
    for (int i = 0; i < 12; i++) {
      TextView line = new TextView(this);
      line.setText("line " + i);
      lines.addView(line);
    }
    scroll.addView(lines);
    root.addView(scroll);

    // A clickable TextView: focusable on a key board because it is clickable, as on Android.
    TextView pick = new TextView(this);
    pick.setText("Pick a colour");
    pick.setOnClickListener(
        v ->
            new AlertDialog.Builder()
                .setTitle("Colour")
                .setItems(
                    new String[] {"Red", "Green", "Blue"},
                    (dialog, which) -> Log.i(TAG, "picked " + which))
                .show());
    root.addView(pick);

    Button done = new Button(this);
    done.setText("Done");
    done.setOnClickListener(v -> Log.i(TAG, "done"));
    root.addView(done);

    setContentView(root);
    seek.requestFocus();
    Log.i(TAG, "ready scrollable=" + (scroll.getHeight() > 0));
  }
}
