// SPDX-License-Identifier: GPL-3.0-only
package pickerdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.DatePicker;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picodroid.widget.TimePicker;

public class PickerDemoActivity extends Activity {
  private static final String TAG = "PickerDemo";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "Display ready");

    ScrollView scroll = new ScrollView(this);

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(8, 8, 8, 8);
    scroll.addView(
        root,
        new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

    TextView dateLabel = new TextView(this);
    dateLabel.setText("Date: --");
    dateLabel.setTextColor(Color.WHITE);
    root.addView(dateLabel);

    DatePicker date = new DatePicker(this);
    date.updateDate(2026, 4, 1); // May: the month is 0-based, as Android's
    date.setOnDateChangedListener(
        (view, year, month, day) -> {
          dateLabel.setText("Date: " + year + "-" + (month + 1) + "-" + day);
          Log.i(TAG, "date " + year + "-" + (month + 1) + "-" + day);
        });
    root.addView(date, new LinearLayout.LayoutParams(220, 200));

    TextView timeLabel = new TextView(this);
    timeLabel.setText("Time: 12:00");
    timeLabel.setTextColor(Color.WHITE);
    root.addView(timeLabel);

    TimePicker time = new TimePicker(this);
    time.setHour(12);
    time.setMinute(0);
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
    root.addView(time, new LinearLayout.LayoutParams(200, 100));

    Button toggle = new Button(this);
    toggle.setText("Toggle 12/24h");
    toggle.setOnClickListener(
        v -> {
          boolean next = !time.is24HourView();
          time.setIs24HourView(next);
          Log.i(TAG, "mode -> " + (next ? "24h" : "12h"));
        });
    root.addView(toggle, new LinearLayout.LayoutParams(200, 36));

    setContentView(scroll);
  }
}
