// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

/**
 * A floating, optionally-actionable bottom-of-screen message. Like {@link Toast}, but with a
 * tappable lozenge that runs a registered {@link Runnable}. Auto-dismisses unless created with
 * {@link #LENGTH_INDEFINITE}, in which case the caller (or the action tap) is responsible for
 * dismissal.
 *
 * <p>Snackbar is not a {@link picodroid.view.View} subclass — the framework owns layout and
 * positioning. Native methods take the handle as an explicit argument rather than reading it off a
 * View slot.
 */
public class Snackbar {
  public static final int LENGTH_SHORT = 0;
  public static final int LENGTH_LONG = 1;
  public static final int LENGTH_INDEFINITE = -1;

  private final int nativeHandle;
  private Runnable actionListener;

  private Snackbar(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public static Snackbar make(String text, int duration) {
    return new Snackbar(nativeCreate(text, duration));
  }

  public Snackbar setAction(String label, Runnable listener) {
    this.actionListener = listener;
    nativeSetAction(nativeHandle, label);
    nativeRegisterActionClickListener();
    return this;
  }

  public void show() {
    nativeShow(nativeHandle);
  }

  public void dismiss() {
    nativeDismiss(nativeHandle);
  }

  /**
   * Invoked from the native event loop when the action lozenge is tapped. Runs the listener (if
   * any) then dismisses the snackbar, mirroring Material guidelines.
   */
  void fireActionClick() {
    Runnable r = actionListener;
    if (r != null) {
      r.run();
    }
    dismiss();
  }

  private static native int nativeCreate(String text, int duration);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeDismiss(int nativeHandle);

  private static native void nativeSetAction(int nativeHandle, String label);

  private native void nativeRegisterActionClickListener();
}
