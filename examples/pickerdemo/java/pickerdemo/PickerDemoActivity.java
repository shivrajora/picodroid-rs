// SPDX-License-Identifier: GPL-3.0-only
package pickerdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.DatePicker;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picodroid.widget.TimePicker;

public class PickerDemoActivity extends Activity {
  private static final String TAG = "PickerDemo";

  public void onCreate() {
    getDisplay();
    Log.i(TAG, "Display ready");

    ScrollView scroll = new ScrollView();
    scroll.setSize(240, 240);

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 460);
    root.setPadding(8, 8, 8, 8);
    scroll.addView(root);

    TextView dateLabel = new TextView();
    dateLabel.setText("Date: --");
    dateLabel.setTextColor(Color.WHITE);
    root.addView(dateLabel);

    DatePicker date = new DatePicker();
    date.setSize(220, 200);
    date.setDate(2026, 5, 1);
    date.setOnDateChangedListener(
        (view, year, month, day) -> {
          dateLabel.setText("Date: " + year + "-" + month + "-" + day);
          Log.i(TAG, "date " + year + "-" + month + "-" + day);
        });
    root.addView(date);

    TextView timeLabel = new TextView();
    timeLabel.setText("Time: 12:00");
    timeLabel.setTextColor(Color.WHITE);
    root.addView(timeLabel);

    TimePicker time = new TimePicker();
    time.setSize(200, 100);
    time.setTime(12, 0);
    time.setOnTimeChangedListener(
        (view, h, m) -> {
          String mode = time.is24HourView() ? "24h" : "12h";
          timeLabel.setText(
              "Time: "
                  + (h < 10 ? "0" : "")
                  + h
                  + ":"
                  + (m < 10 ? "0" : "")
                  + m
                  + " ("
                  + mode
                  + ")");
          Log.i(TAG, "time " + h + ":" + m + " " + mode);
        });
    root.addView(time);

    Button toggle = new Button("Toggle 12/24h");
    toggle.setSize(200, 36);
    toggle.setOnClickListener(
        v -> {
          boolean next = !time.is24HourView();
          time.setIs24HourView(next);
          Log.i(TAG, "mode -> " + (next ? "24h" : "12h"));
        });
    root.addView(toggle);

    setContentView(scroll);
  }
}
