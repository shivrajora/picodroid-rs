// SPDX-License-Identifier: GPL-3.0-only
package keydemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Hardware keys, both ways Android delivers them: a focused view's {@link OnKeyListener} first,
 * then the Activity's {@link #onKeyDown} / {@link #onKeyUp} for whatever the view left alone. The
 * button consumes DPAD_CENTER and passes everything else on; BACK is consumed by the Activity so
 * the demo never finishes, which is also what the default {@code onKeyUp} would do with it.
 */
public class KeyDemoActivity extends Activity implements OnKeyListener {
  private static final String TAG = "KeyDemo";
  private TextView status;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // Force display init before constructing any widgets.
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Hardware Key Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    status = new TextView();
    status.setText("Press any button");
    status.setTextColor(Color.CYAN);
    root.addView(status);

    // A focused view sees a key first. Without one every key goes straight to onKeyDown/onKeyUp.
    Button focus = new Button("Focus me");
    focus.setSize(200, 50);
    focus.setOnKeyListener(this);
    root.addView(focus);

    setContentView(root);
  }

  @Override
  public boolean onKey(View v, KeyEvent event) {
    if (event.getKeyCode() != KeyEvent.KEYCODE_DPAD_CENTER) {
      return false; // not ours: falls through to the Activity
    }
    show("view", event.getAction(), event.getKeyCode());
    return true;
  }

  @Override
  public boolean onKeyDown(int keyCode, KeyEvent event) {
    show("activity", KeyEvent.ACTION_DOWN, keyCode);
    return true;
  }

  @Override
  public boolean onKeyUp(int keyCode, KeyEvent event) {
    show("activity", KeyEvent.ACTION_UP, keyCode);
    return true;
  }

  private void show(String who, int action, int keyCode) {
    String line =
        who + " " + (action == KeyEvent.ACTION_DOWN ? "DOWN" : "UP") + " keyCode=" + keyCode;
    status.setText(line);
    Log.i(TAG, line);
  }
}
