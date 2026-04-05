package picodroid.widget;

import picodroid.view.View;

public class Button extends View {
  private Runnable onClickListener;

  public Button(String text) {
    super(nativeCreate(text));
  }

  private static native int nativeCreate(String text);

  public native void setText(String text);

  public native boolean wasClicked();

  public void setOnClickListener(Runnable listener) {
    this.onClickListener = listener;
  }

  void fireClick() {
    if (onClickListener != null) {
      onClickListener.run();
    }
  }
}
