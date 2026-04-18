package picodroid.view;

public class View {
  public static final int VISIBLE = 0;
  public static final int INVISIBLE = 1;
  public static final int GONE = 2;

  int nativeHandle;
  OnKeyListener onKeyListener;

  protected View(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public void setOnKeyListener(OnKeyListener listener) {
    this.onKeyListener = listener;
    nativeRegisterKeyListener();
  }

  private native void nativeRegisterKeyListener();

  boolean fireKey(KeyEvent event) {
    if (onKeyListener != null) {
      return onKeyListener.onKey(this, event);
    }
    return false;
  }

  public native void setPosition(int x, int y);

  public native void setSize(int width, int height);

  public native void setBackgroundColor(int argb);

  public native void setVisibility(int visibility);

  public native void setPadding(int left, int top, int right, int bottom);

  public native void setEnabled(boolean enabled);

  public native void setAlpha(float alpha);

  public native void close();
}
