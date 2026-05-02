// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

/**
 * Single-line text input. Tapping pops up the system soft keyboard by default; opt-out via {@link
 * #setShowKeyboardOnTouch}{@code (false)} when constructing an explicit {@link Keyboard} instance.
 *
 * <p>Done-detection: register an {@link OnEditorActionListener} via {@link
 * #setOnEditorActionListener} to react to the keyboard's OK / Enter key. The listener fires only
 * for the implicit system keyboard; explicit {@code Keyboard} instances use {@link
 * Keyboard#setOnReadyListener} instead.
 */
public class EditText extends View {

  private OnEditorActionListener editorActionListener;

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

  /**
   * Register a listener for the system keyboard's OK / Enter key. Pass {@code null} to clear; in
   * that case the keyboard falls back to its default auto-dismiss behavior.
   */
  public void setOnEditorActionListener(OnEditorActionListener listener) {
    this.editorActionListener = listener;
    nativeRegisterEditorActionListener();
  }

  /**
   * Invoked from native code when the system keyboard's OK key fires while bound to this EditText.
   * Returns {@code true} if the listener consumed the action — in which case the native layer skips
   * its default dismiss.
   */
  boolean fireEditorAction(int actionId) {
    if (editorActionListener == null) {
      return false;
    }
    return editorActionListener.onEditorAction(this, actionId, null);
  }

  private native void nativeRegisterEditorActionListener();
}
