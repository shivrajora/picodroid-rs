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
 * Hardware keys, every way Android delivers them: a focused view's {@link OnKeyListener} first,
 * then the Activity's {@link #onKeyDown} / {@link #onKeyLongPress} / {@link #onKeyUp} for whatever
 * the view left alone. The button consumes DPAD_CENTER and passes everything else on.
 *
 * <p>DPAD_UP shows Android's two-actions-per-button pattern: the press only {@linkplain
 * KeyEvent#startTracking starts tracking}; a long hold runs {@link #onKeyLongPress}, which cancels
 * the release; a short press runs its action from {@link #onKeyUp}, when the release is tracked and
 * not cancelled. DPAD_DOWN shows auto-repeat: hold it and {@link KeyEvent#getRepeatCount} climbs.
 * BACK is left to the defaults, Android's contract: {@code onKeyDown} tracks it and {@code onKeyUp}
 * runs {@link #onBackPressed}, which finishes the demo.
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

    // A focused view sees a key first. Without one every key goes straight to the Activity.
    Button focus = new Button("Focus me");
    focus.setSize(200, 50);
    focus.setOnKeyListener(this);
    root.addView(focus);

    setContentView(root);
    focus.requestFocus();
    Log.i(TAG, "ready");
  }

  @Override
  public boolean onKey(View v, KeyEvent event) {
    if (event.getKeyCode() != KeyEvent.KEYCODE_DPAD_CENTER) {
      return false; // not ours: falls through to the Activity
    }
    show("view " + name(event.getAction()) + " keyCode=" + event.getKeyCode());
    return true;
  }

  @Override
  public boolean onKeyDown(int keyCode, KeyEvent event) {
    switch (keyCode) {
      case KeyEvent.KEYCODE_DPAD_UP:
        // Two actions: nothing happens on the press itself. Tracking asks for the long-press
        // and marks the release; the count check keeps a repeat from re-arming it.
        if (event.getRepeatCount() == 0) {
          event.startTracking();
          show("activity DOWN keyCode=" + keyCode + " (release: short, hold: long)");
        }
        return true;
      case KeyEvent.KEYCODE_BACK:
        return super.onKeyDown(keyCode, event); // the default tracks it for onBackPressed
      default:
        // Every other key acts on the press, and keeps acting while held.
        show("activity DOWN keyCode=" + keyCode + " repeat=" + event.getRepeatCount());
        return true;
    }
  }

  @Override
  public boolean onKeyLongPress(int keyCode, KeyEvent event) {
    if (keyCode == KeyEvent.KEYCODE_DPAD_UP) {
      show("activity LONG keyCode=" + keyCode);
      return true; // cancels the release: onKeyUp sees isCanceled()
    }
    return super.onKeyLongPress(keyCode, event);
  }

  @Override
  public boolean onKeyUp(int keyCode, KeyEvent event) {
    if (keyCode == KeyEvent.KEYCODE_DPAD_UP) {
      if (event.isTracking() && !event.isCanceled()) {
        show("activity SHORT keyCode=" + keyCode);
      } else {
        show("activity UP keyCode=" + keyCode + (event.isCanceled() ? " canceled" : ""));
      }
      return true;
    }
    if (keyCode == KeyEvent.KEYCODE_BACK) {
      return super.onKeyUp(keyCode, event); // a tracked, uncancelled release finishes the demo
    }
    show("activity UP keyCode=" + keyCode);
    return true;
  }

  private static String name(int action) {
    return action == KeyEvent.ACTION_DOWN ? "DOWN" : "UP";
  }

  private void show(String line) {
    status.setText(line);
    Log.i(TAG, line);
  }
}
