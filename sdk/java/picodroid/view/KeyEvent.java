// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * A hardware key press or release. Mirrors {@code android.view.KeyEvent}: one instance per edge,
 * delivered first to the focused view's {@link OnKeyListener} and then, if unconsumed, to the
 * foreground Activity's {@code onKeyDown} / {@code onKeyUp}.
 *
 * <p>The framework recycles one instance for every key edge, so an event must not be kept past the
 * callback it arrives in.
 */
public class KeyEvent {
  public static final int ACTION_DOWN = 0;
  public static final int ACTION_UP = 1;

  public static final int KEYCODE_HOME = 3;
  public static final int KEYCODE_BACK = 4;
  public static final int KEYCODE_DPAD_UP = 19;
  public static final int KEYCODE_DPAD_DOWN = 20;
  public static final int KEYCODE_DPAD_LEFT = 21;
  public static final int KEYCODE_DPAD_RIGHT = 22;
  public static final int KEYCODE_DPAD_CENTER = 23;

  // Field order is the native slot order (graphics/fields.rs::key_event); append only.
  private int action;
  private int keyCode;

  /**
   * Set by {@link #startTracking} on the DOWN edge and carried to the matching UP edge by the
   * dispatcher, so a handler that consumed the press can tell the release apart from one whose
   * press went elsewhere. Cleared on every new press.
   */
  private boolean tracking;

  KeyEvent(int action, int keyCode) {
    this.action = action;
    this.keyCode = keyCode;
  }

  public int getAction() {
    return action;
  }

  public int getKeyCode() {
    return keyCode;
  }

  /**
   * Mark this press as one the caller wants to see the release of. Call it from {@code onKeyDown}
   * and test {@link #isTracking} in {@code onKeyUp}: the default {@code Activity.onKeyDown} does
   * this for BACK so that {@code onKeyUp} only runs {@code onBackPressed} when the press was not
   * taken by an override. Mirrors Android's {@code KeyEvent.startTracking()}.
   */
  public void startTracking() {
    tracking = true;
  }

  /** Whether {@link #startTracking} was called on the press this release belongs to. */
  public boolean isTracking() {
    return tracking;
  }
}
