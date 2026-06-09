// SPDX-License-Identifier: GPL-3.0-only
package swipedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.SwipeRefreshLayout;
import picodroid.widget.TextView;

public class SwipeDemoActivity extends Activity {
  private static final String TAG = "SwipeDemo";

  private int refreshCount = 0;

  @Override
  public void onCreate() {
    getDisplay();
    Log.i(TAG, "Display ready");

    SwipeRefreshLayout refresh = new SwipeRefreshLayout();
    refresh.setSize(240, 240);

    LinearLayout content = new LinearLayout();
    content.setOrientation(LinearLayout.VERTICAL);
    content.setSize(240, 240);
    content.setPadding(10, 40, 10, 10);

    TextView label = new TextView();
    label.setText("Pull down or swipe");
    label.setTextColor(Color.WHITE);
    content.addView(label);

    TextView counter = new TextView();
    counter.setText("Refreshes: 0");
    counter.setTextColor(Color.CYAN);
    content.addView(counter);

    refresh.setOnRefreshListener(
        () -> {
          refreshCount++;
          counter.setText("Refreshes: " + refreshCount);
          Log.i(TAG, "refresh " + refreshCount);
          // In a real app the work would happen async; for the demo we just
          // immediately clear the spinner.
          refresh.setRefreshing(false);
        });
    refresh.addView(content);

    // Also exercise the bare OnSwipeListener (any direction logs).
    content.setOnSwipeListener(
        (v, dir) -> {
          String name;
          if (dir == View.SWIPE_LEFT) {
            name = "left";
          } else if (dir == View.SWIPE_RIGHT) {
            name = "right";
          } else if (dir == View.SWIPE_UP) {
            name = "up";
          } else {
            name = "down";
          }
          Log.i(TAG, "swipe " + name);
        });

    setContentView(refresh);
  }
}
