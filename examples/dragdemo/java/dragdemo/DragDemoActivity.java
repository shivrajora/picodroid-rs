// SPDX-License-Identifier: GPL-3.0-only
package dragdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.view.MotionEvent;
import picodroid.view.OnTouchListener;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DragDemoActivity extends Activity {
  private static final int TILE_SIZE = 60;
  private static final int TILE_HALF = TILE_SIZE / 2;

  private TextView tile;
  private TextView status;
  private int moveCount;

  @Override
  public void onCreate() {
    getDisplay();

    // FrameLayout root so the tile can live at an absolute position.
    // A LinearLayout would re-flow children on every layout pass and
    // clobber setPosition.
    FrameLayout root = new FrameLayout();
    root.setSize(240, 240);

    LinearLayout header = new LinearLayout();
    header.setOrientation(LinearLayout.VERTICAL);
    header.setSize(240, 90);
    header.setPosition(0, 0);
    header.setPadding(10, 10, 10, 0);

    TextView title = new TextView();
    title.setText("Drag Demo");
    title.setTextColor(Color.WHITE);
    header.addView(title);

    TextView hint = new TextView();
    hint.setText("Touch anywhere to move the tile.");
    hint.setTextColor(Color.WHITE);
    hint.setSize(220, 40);
    header.addView(hint);

    status = new TextView();
    status.setText("(no touch yet)");
    status.setTextColor(Color.CYAN);
    header.addView(status);

    root.addView(header);

    // The draggable tile — sized 60x60, centered at (120, 150) initially.
    tile = new TextView();
    tile.setText("");
    tile.setSize(TILE_SIZE, TILE_SIZE);
    tile.setBackgroundColor(Color.argb(255, 80, 120, 200));
    tile.setPosition(120 - TILE_HALF, 150 - TILE_HALF);
    root.addView(tile);

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
    int tx = clamp(touchX - TILE_HALF, 0, 240 - TILE_SIZE);
    int ty = clamp(touchY - TILE_HALF, 90, 240 - TILE_SIZE);
    tile.setPosition(tx, ty);
  }

  private static int clamp(int v, int lo, int hi) {
    if (v < lo) return lo;
    if (v > hi) return hi;
    return v;
  }
}
