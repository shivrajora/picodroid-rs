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

  private int action;
  private int x;
  private int y;

  /** Tick-clock millis at the moment LVGL emitted this event. Used for fling velocity. */
  private long eventTime;

  MotionEvent(int action, int x, int y, long eventTime) {
    this.action = action;
    this.x = x;
    this.y = y;
    this.eventTime = eventTime;
  }

  public int getAction() {
    return action;
  }

  public int getX() {
    return x;
  }

  public int getY() {
    return y;
  }

  public long getEventTime() {
    return eventTime;
  }
}
