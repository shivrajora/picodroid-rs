// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

public class MotionEvent {
  public static final int ACTION_DOWN = 0;
  public static final int ACTION_UP = 1;
  public static final int ACTION_MOVE = 2;

  /**
   * picodroid extension (not in Android): synthetic event delivered when LVGL detects a long press
   * — fires *while the finger is still down*, before the eventual ACTION_UP. GestureDetector uses
   * this to call onLongPress at the canonical 400 ms LVGL threshold.
   */
  public static final int ACTION_LONG_PRESS = 3;

  // View-relative coordinates (Android's getX/getY): the touch point offset
  // by the receiving view's top-left.
  private int action;
  private int x;
  private int y;

  /** Tick-clock millis at the moment LVGL emitted this event. Used for fling velocity. */
  private long eventTime;

  // Screen-absolute coordinates (Android's getRawX/getRawY). Declared AFTER
  // eventTime so the action/x/y/eventTime field slots (0-3) the native
  // dispatcher writes by index stay put; rawX/rawY are slots 4/5.
  private int rawX;
  private int rawY;

  MotionEvent(int action, int x, int y, long eventTime) {
    this.action = action;
    this.x = x;
    this.y = y;
    this.eventTime = eventTime;
  }

  public int getAction() {
    return action;
  }

  /**
   * X relative to the receiving view's left edge. Mirrors {@code android.view.MotionEvent#getX()}.
   * Picodroid divergence: Android returns a float; picodroid uses int (no sub-pixel touch on a
   * resistive panel).
   */
  public int getX() {
    return x;
  }

  /** Y relative to the receiving view's top edge. Mirrors {@code MotionEvent#getY()}. */
  public int getY() {
    return y;
  }

  /** Screen-absolute X. Mirrors {@code android.view.MotionEvent#getRawX()}. */
  public int getRawX() {
    return rawX;
  }

  /** Screen-absolute Y. Mirrors {@code android.view.MotionEvent#getRawY()}. */
  public int getRawY() {
    return rawY;
  }

  public long getEventTime() {
    return eventTime;
  }
}
