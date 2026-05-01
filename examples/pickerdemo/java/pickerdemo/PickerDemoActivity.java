package pickerdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
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
        () -> {
          dateLabel.setText(
              "Date: " + date.getYear() + "-" + date.getMonth() + "-" + date.getDay());
          Log.i(TAG, "date " + date.getYear() + "-" + date.getMonth() + "-" + date.getDay());
        });
    root.addView(date);

    TextView timeLabel = new TextView();
    timeLabel.setText("Time: 12:00");
    timeLabel.setTextColor(Color.WHITE);
    root.addView(timeLabel);

    TimePicker time = new TimePicker();
    time.setSize(160, 100);
    time.setTime(12, 0);
    time.setOnTimeChangedListener(
        () -> {
          int h = time.getHour();
          int m = time.getMinute();
          timeLabel.setText("Time: " + (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m);
          Log.i(TAG, "time " + h + ":" + m);
        });
    root.addView(time);

    setContentView(scroll);
  }
}
