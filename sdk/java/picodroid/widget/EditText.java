package picodroid.widget;

import picodroid.view.View;

public class EditText extends View {

  public EditText() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  public native String getText();

  public native void setHint(String hint);

  /**
   * Toggle whether tapping this EditText pops up the system soft keyboard. Default {@code true}.
   * Apps that construct their own {@link Keyboard} instance for full control over placement should
   * call {@code setShowKeyboardOnTouch(false)} so the system one doesn't also appear.
   */
  public native void setShowKeyboardOnTouch(boolean enabled);
}
