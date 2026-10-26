// SPDX-License-Identifier: GPL-3.0-only
package dragdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.MotionEvent;
import picodroid.view.OnTouchListener;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DragDemoActivity extends Activity {
  private static final int TILE_SIZE = 60;
  private static final int TILE_HALF = TILE_SIZE / 2;
  private static final int HEADER_HEIGHT = 90;

  private FrameLayout root;
  private TextView tile;
  private final FrameLayout.LayoutParams tileParams =
      new FrameLayout.LayoutParams(TILE_SIZE, TILE_SIZE, Gravity.TOP | Gravity.LEFT);
  private TextView status;
  private int moveCount;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    getDisplay();

    // FrameLayout root so the tile can live at an absolute position (its
    // margins). A LinearLayout would re-flow children on every layout pass.
    root = new FrameLayout(this);

    LinearLayout header = new LinearLayout(this);
    header.setOrientation(LinearLayout.VERTICAL);
    header.setPadding(10, 10, 10, 0);

    TextView title = new TextView(this);
    title.setText("Drag Demo");
    title.setTextColor(Color.WHITE);
    header.addView(title);

    TextView hint = new TextView(this);
    hint.setText("Touch anywhere to move the tile.");
    hint.setTextColor(Color.WHITE);
    header.addView(hint, new LinearLayout.LayoutParams(220, 40));

    status = new TextView(this);
    status.setText("(no touch yet)");
    status.setTextColor(Color.CYAN);
    header.addView(status);

    root.addView(
        header,
        new FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT, HEADER_HEIGHT, Gravity.TOP | Gravity.LEFT));

    // The draggable tile — sized 60x60, centered at (120, 150) initially.
    tile = new TextView(this);
    tile.setText("");
    tile.setBackgroundColor(Color.argb(255, 80, 120, 200));
    tileParams.leftMargin = 120 - TILE_HALF;
    tileParams.topMargin = 150 - TILE_HALF;
    root.addView(tile, tileParams);

    // Listener lives on the root, not the tile. With ACTION_MOVE keyed
    // by the registered widget's hit-test, attaching to a small tile
    // would lose tracking the moment the finger crossed its boundary
    // (LV_EVENT_PRESS_LOST is out of scope in v1). Root-listener +
    // tile-follows-finger gives a clean demo of the new MOVE delivery.
    root.setOnTouchListener(
        new OnTouchListener() {
          @Override
          public boolean onTouch(View v, MotionEvent event) {
            int action = event.getAction();
            // Raw (screen-absolute) coords: the tile is positioned in screen
            // space, and getX/getY are now view-relative to the root.
            int x = event.getRawX();
            int y = event.getRawY();
            if (action == MotionEvent.ACTION_DOWN) {
              moveCount = 0;
              moveTileTo(x, y);
              status.setText("DOWN x=" + x + " y=" + y);
              return true;
            }
            if (action == MotionEvent.ACTION_MOVE) {
              moveCount++;
              moveTileTo(x, y);
              status.setText("MOVE #" + moveCount + " x=" + x + " y=" + y);
              return true;
            }
            if (action == MotionEvent.ACTION_UP) {
              status.setText("UP @ (" + x + ", " + y + "), MOVEs=" + moveCount);
              Log.i("DragDemo", "drag complete: " + moveCount + " MOVEs");
              return true;
            }
            return false;
          }
        });

    setContentView(root);
  }

  private void moveTileTo(int touchX, int touchY) {
    tileParams.leftMargin = clamp(touchX - TILE_HALF, 0, root.getWidth() - TILE_SIZE);
    tileParams.topMargin = clamp(touchY - TILE_HALF, HEADER_HEIGHT, root.getHeight() - TILE_SIZE);
    tile.setLayoutParams(tileParams);
  }

  private static int clamp(int v, int lo, int hi) {
    if (v < lo) return lo;
    if (v > hi) return hi;
    return v;
  }
}
