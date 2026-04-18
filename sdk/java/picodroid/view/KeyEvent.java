package picodroid.view;

public class KeyEvent {
  public static final int ACTION_DOWN = 0;
  public static final int ACTION_UP = 1;

  public static final int KEYCODE_BACK = 4;
  public static final int KEYCODE_DPAD_UP = 19;
  public static final int KEYCODE_DPAD_DOWN = 20;
  public static final int KEYCODE_DPAD_LEFT = 21;
  public static final int KEYCODE_DPAD_RIGHT = 22;
  public static final int KEYCODE_DPAD_CENTER = 23;

  private int action;
  private int keyCode;

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
}
