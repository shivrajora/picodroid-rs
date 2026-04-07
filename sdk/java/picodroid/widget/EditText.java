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
}
