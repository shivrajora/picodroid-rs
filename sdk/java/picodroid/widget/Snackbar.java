// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

/**
 * A floating, optionally-actionable bottom-of-screen message. Like {@link Toast}, but with a
 * tappable lozenge that runs a registered {@link View.OnClickListener}. Auto-dismisses unless
 * created with {@link #LENGTH_INDEFINITE}, in which case the caller (or the action tap) is
 * responsible for dismissal.
 *
 * <p>A shown snackbar is held by the framework until it goes away, however it goes — what Android's
 * {@code SnackbarManager} does — so {@code Snackbar.make(v, "Saved",
 * LENGTH_SHORT).setAction("Undo", l).show()} is a complete statement and needs no field.
 *
 * <p>Snackbar is not a {@link picodroid.view.View} subclass — the framework owns layout and
 * positioning. Native methods take the handle as an explicit argument rather than reading it off a
 * View slot.
 */
public class Snackbar {
  public static final int LENGTH_SHORT = 0;
  public static final int LENGTH_LONG = 1;
  public static final int LENGTH_INDEFINITE = -1;

  /**
   * The snackbars on screen, oldest first; four, as the framework shows at most four at once (a
   * fifth takes the oldest's place). A snackbar is held from {@link #show} until {@link #dismiss},
   * the action tap, or its timeout. A static is a GC root.
   */
  private static final Snackbar[] sShowing = new Snackbar[4];

  private final int nativeHandle;
  private View.OnClickListener actionListener;
  private boolean showing;

  private Snackbar(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  /**
   * Create a snackbar attached to the visual hierarchy rooted at {@code parent}. The {@code parent}
   * argument matches Android's signature; the framework currently positions all snackbars at the
   * screen bottom regardless of parent, but the parameter is captured for future per-container
   * placement. Pass any view from the current Activity (commonly the root layout).
   */
  public static Snackbar make(View parent, String text, int duration) {
    return new Snackbar(nativeCreate(text, duration));
  }

  public Snackbar setAction(String label, View.OnClickListener listener) {
    this.actionListener = listener;
    nativeSetAction(nativeHandle, label);
    return this;
  }

  /** Shows the snackbar; the framework holds it until it goes away. */
  public void show() {
    // Name this object as the receiver of the action tap and of the timeout, so the framework can
    // tell it when the bar has gone. Only a shown snackbar is bound.
    nativeRegisterActionClickListener();
    nativeShow(nativeHandle);
    showing = true;
    addShowing(this);
  }

  public void dismiss() {
    nativeDismiss(nativeHandle);
    dismissed();
  }

  /** Mirrors {@code BaseTransientBottomBar#isShown}: from {@link #show} until the bar went away. */
  public boolean isShown() {
    return showing;
  }

  /**
   * Invoked from the native event loop when the action lozenge is tapped. Runs the listener (if
   * any) then dismisses the snackbar, mirroring Material guidelines. The lozenge is not surfaced as
   * a {@link View} so {@code v} is {@code null} — match Android's behavior of passing the action
   * button view, which we don't track.
   */
  void fireActionClick() {
    View.OnClickListener l = actionListener;
    if (l != null) {
      l.onClick(null);
    }
    dismiss();
  }

  /**
   * Invoked from the native event loop after the bar's timeout took it down: the widgets are gone
   * by now and only the Java side is left to let go of the snackbar.
   */
  void fireDismiss() {
    dismissed();
  }

  private void dismissed() {
    showing = false;
    removeShowing(this);
  }

  private static void addShowing(Snackbar s) {
    for (int i = 0; i < sShowing.length; i++) {
      if (sShowing[i] == s) {
        return;
      }
    }
    for (int i = 0; i < sShowing.length; i++) {
      if (sShowing[i] == null) {
        sShowing[i] = s;
        return;
      }
    }
    // Full: the oldest gives way, as it does on the screen.
    for (int i = 0; i < sShowing.length - 1; i++) {
      sShowing[i] = sShowing[i + 1];
    }
    sShowing[sShowing.length - 1] = s;
  }

  private static void removeShowing(Snackbar s) {
    for (int i = 0; i < sShowing.length; i++) {
      if (sShowing[i] == s) {
        for (int j = i; j < sShowing.length - 1; j++) {
          sShowing[j] = sShowing[j + 1];
        }
        sShowing[sShowing.length - 1] = null;
        return;
      }
    }
  }

  private static native int nativeCreate(String text, int duration);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeDismiss(int nativeHandle);

  private static native void nativeSetAction(int nativeHandle, String label);

  private native void nativeRegisterActionClickListener();
}
