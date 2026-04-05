package picodroid.view;

public class MotionEvent {
  public static final int ACTION_DOWN = 0;
  public static final int ACTION_UP = 1;
  public static final int ACTION_MOVE = 2;

  private int action;
  private int x;
  private int y;

  MotionEvent(int action, int x, int y) {
    this.action = action;
    this.x = x;
    this.y = y;
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
}
