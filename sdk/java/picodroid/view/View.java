package picodroid.view;

public class View {
  public static final int VISIBLE = 0;
  public static final int INVISIBLE = 1;
  public static final int GONE = 2;

  int nativeHandle;
  OnKeyListener onKeyListener;
  OnTouchListener onTouchListener;

  protected View(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public void setOnKeyListener(OnKeyListener listener) {
    this.onKeyListener = listener;
    nativeRegisterKeyListener();
  }

  /**
   * Register a touch listener. The framework also flips this View's LVGL CLICKABLE flag so the
   * underlying touch indev routes Press/Release events here. Pass {@code null} to clear (the
   * CLICKABLE flag stays on — clearing it on a button widget would break click behavior).
   */
  public void setOnTouchListener(OnTouchListener listener) {
    this.onTouchListener = listener;
    nativeRegisterTouchListener();
  }

  private native void nativeRegisterKeyListener();

  private native void nativeRegisterTouchListener();

  boolean fireKey(KeyEvent event) {
    if (onKeyListener != null) {
      return onKeyListener.onKey(this, event);
    }
    return false;
  }

  boolean fireTouch(MotionEvent event) {
    if (onTouchListener != null) {
      return onTouchListener.onTouch(this, event);
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

  /**
   * Returns a fresh {@link ViewPropertyAnimator} for this view. Mirrors {@code View.animate()} in
   * Android — chain alpha/x/y + setDuration on the result and call {@code start()}.
   */
  public ViewPropertyAnimator animate() {
    return new ViewPropertyAnimator(this);
  }
}
