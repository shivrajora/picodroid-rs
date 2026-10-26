// SPDX-License-Identifier: GPL-3.0-only
package gesturedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.view.GestureDetector;
import picodroid.view.MotionEvent;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class GestureDemoActivity extends Activity {
  private TextView status;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    GestureActivityComponent c = new GestureActivityComponent();
    GestureAppComponent app = c.appComponent();

    getDisplay();

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText("Gesture Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView hint = new TextView(this);
    hint.setText("Tap, long-press, or swipe the box.");
    hint.setTextColor(Color.WHITE);
    root.addView(hint);

    status = new TextView(this);
    status.setText("(no gesture yet)");
    status.setTextColor(Color.CYAN);
    root.addView(status);

    // The "touch surface" is a sized TextView. setOnTouchListener flips on
    // LVGL's CLICKABLE flag so the underlying object actually receives
    // press/release events.
    TextView surface = new TextView(this);
    surface.setText("touch me");
    surface.setBackgroundColor(Color.argb(255, 60, 60, 80));
    // SimpleOnGestureListener (vs the raw interface) exercises the abstract
    // base class: subclass through a class-typed supertype, not invokeinterface.
    surface.setOnTouchListener(
        new GestureDetector(
            this,
            new GestureDetector.SimpleOnGestureListener() {
              @Override
              public void onSingleTap(MotionEvent e) {
                // getX/getY are view-relative to the touch surface.
                app.info("tap");
                status.setText("Tap @ rel(" + e.getX() + ", " + e.getY() + ")");
              }

              @Override
              public void onLongPress(MotionEvent e) {
                app.info("long-press");
                status.setText("Long press @ rel(" + e.getX() + ", " + e.getY() + ")");
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
    // GestureDetector.onTouch returns true (consumes the stream), so this
    // click must be suppressed when the surface is tapped — Android's
    // contract. If "surface-click-LEAK" ever logs, suppression broke.
    surface.setOnClickListener(v -> app.info("surface-click-LEAK"));
    root.addView(surface, new LinearLayout.LayoutParams(220, 120));

    // A second tile wired to View.OnLongClickListener directly (the LVGL
    // long-press path, separate from GestureDetector). performLongClick()
    // exercises the pure-Java synthesis for a deterministic headless check.
    TextView tile = new TextView(this);
    tile.setText("hold me");
    tile.setBackgroundColor(Color.argb(255, 80, 60, 60));
    tile.setOnLongClickListener(
        v -> {
          app.info("tile long-clicked");
          return true;
        });
    // No onTouch consumer on this tile, so a short tap's click fires normally
    // (the suppression baseline — proves the reorder didn't break clicks).
    tile.setOnClickListener(v -> app.info("tile-click-ok"));
    root.addView(tile, new LinearLayout.LayoutParams(220, 40));
    if (tile.performLongClick()) {
      app.info("long-click consumed");
    }

    setContentView(root);
  }
}
