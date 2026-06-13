// SPDX-License-Identifier: GPL-3.0-only
package gesturedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.view.GestureDetector;
import picodroid.view.MotionEvent;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class GestureDemoActivity extends Activity {
  private TextView status;

  @Override
  public void onCreate() {
    GestureActivityComponent c = new GestureActivityComponent();
    GestureAppComponent app = c.appComponent();

    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Gesture Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView hint = new TextView();
    hint.setText("Tap, long-press, or swipe the box.");
    hint.setTextColor(Color.WHITE);
    root.addView(hint);

    status = new TextView();
    status.setText("(no gesture yet)");
    status.setTextColor(Color.CYAN);
    root.addView(status);

    // The "touch surface" is a sized TextView. setOnTouchListener flips on
    // LVGL's CLICKABLE flag so the underlying object actually receives
    // press/release events.
    TextView surface = new TextView();
    surface.setText("touch me");
    surface.setSize(220, 120);
    surface.setBackgroundColor(Color.argb(255, 60, 60, 80));
    // SimpleOnGestureListener (vs the raw interface) exercises the abstract
    // base class: subclass through a class-typed supertype, not invokeinterface.
    surface.setOnTouchListener(
        new GestureDetector(
            new GestureDetector.SimpleOnGestureListener() {
              @Override
              public void onSingleTap(MotionEvent e) {
                app.info("tap");
                status.setText("Tap @ (" + e.getX() + ", " + e.getY() + ")");
              }

              @Override
              public void onLongPress(MotionEvent e) {
                app.info("long-press");
                status.setText("Long press @ (" + e.getX() + ", " + e.getY() + ")");
              }

              @Override
              public void onFling(MotionEvent down, MotionEvent up, float vx, float vy) {
                String dir =
                    Math.abs(vx) > Math.abs(vy)
                        ? (vx > 0 ? "right" : "left")
                        : (vy > 0 ? "down" : "up");
                app.info("fling " + dir);
                status.setText("Fling " + dir + " (vx=" + (int) vx + ", vy=" + (int) vy + ")");
              }
            }));
    root.addView(surface);

    // A second tile wired to View.OnLongClickListener directly (the LVGL
    // long-press path, separate from GestureDetector). performLongClick()
    // exercises the pure-Java synthesis for a deterministic headless check.
    TextView tile = new TextView();
    tile.setText("hold me");
    tile.setSize(220, 40);
    tile.setBackgroundColor(Color.argb(255, 80, 60, 60));
    tile.setOnLongClickListener(
        v -> {
          app.info("tile long-clicked");
          return true;
        });
    root.addView(tile);
    if (tile.performLongClick()) {
      app.info("long-click consumed");
    }

    setContentView(root);
  }
}
