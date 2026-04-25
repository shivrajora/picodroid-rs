package picodroid.widget;

public class Toast {
  public static final int LENGTH_SHORT = 0;
  public static final int LENGTH_LONG = 1;

  private final int nativeHandle;

  private Toast(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public static Toast makeText(String text, int duration) {
    return new Toast(nativeCreate(text, duration));
  }

  public void show() {
    nativeShow(nativeHandle);
  }

  public void cancel() {
    nativeCancel(nativeHandle);
  }

  private static native int nativeCreate(String text, int duration);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeCancel(int nativeHandle);
}
