// SPDX-License-Identifier: GPL-3.0-only
package touchprobe;

import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.MotionEvent;
import picodroid.view.OnTouchListener;
import picodroid.view.View;
import picodroid.widget.TextView;

/**
 * A bench probe for the touch calibration (docs/bench-handoff-2026-10-11.md §3): one full-screen
 * view that logs where every touch-down lands, in panel pixels, so a finger on the four corners and
 * the centre shows whether a board's {@code [touch] cal_*} table maps the panel's edges to the
 * screen's.
 */
public class TouchProbeActivity extends Activity {
  private static final String TAG = "TouchProbe";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // An unsized content root is the window, so getX/getY are panel pixels.
    TextView surface = new TextView(this);
    surface.setText("tap the corners");
    surface.setOnTouchListener(
        new OnTouchListener() {
          @Override
          public boolean onTouch(View v, MotionEvent e) {
            if (e.getAction() == MotionEvent.ACTION_DOWN) {
              Log.i(TAG, "down " + e.getX() + "," + e.getY());
              surface.setText("down " + e.getX() + "," + e.getY());
            }
            return true;
          }
        });
    setContentView(surface);
    Log.i(TAG, "ready");
  }
}
