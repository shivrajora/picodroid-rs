// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
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
  private picodroid.text.TextWatcher[] watchers;
  private int watcherCount;

  public EditText() {
    super(nativeCreate());
  }

  public EditText(Context ctx) {
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
   * Set the input type, mirroring {@code android.widget.TextView.setInputType}. Only the class is
   * honored: {@link picodroid.text.InputType#TYPE_CLASS_NUMBER} makes the system keyboard open in
   * digit-pad mode for this field; anything else uses the default text layout.
   */
  public native void setInputType(int type);

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

  /**
   * Mirrors {@code android.widget.TextView#addTextChangedListener}. Only {@code afterTextChanged}
   * fires on picodroid — see {@link picodroid.text.TextWatcher} for the divergences. As on Android,
   * {@link #setText} also triggers the watcher.
   */
  public void addTextChangedListener(picodroid.text.TextWatcher watcher) {
    if (watcher == null) {
      return;
    }
    if (watchers == null) {
      watchers = new picodroid.text.TextWatcher[2];
    }
    if (watcherCount == watchers.length) {
      picodroid.text.TextWatcher[] bigger = new picodroid.text.TextWatcher[watchers.length * 2];
      System.arraycopy(watchers, 0, bigger, 0, watcherCount);
      watchers = bigger;
    }
    watchers[watcherCount++] = watcher;
    nativeRegisterTextChangedListener();
  }

  /** Mirrors {@code android.widget.TextView#removeTextChangedListener}. */
  public void removeTextChangedListener(picodroid.text.TextWatcher watcher) {
    for (int i = 0; i < watcherCount; i++) {
      if (watchers[i] == watcher) {
        for (int j = i; j < watcherCount - 1; j++) {
          watchers[j] = watchers[j + 1];
        }
        watchers[--watcherCount] = null;
        return;
      }
    }
  }

  /**
   * Invoked from native code when the textarea content changed. Re-reads the current text once and
   * fans it out — the native queue carries only the widget handle, so a burst of keystrokes
   * coalesces into one callback with the final text.
   */
  void fireTextChanged() {
    if (watcherCount == 0) {
      return;
    }
    String text = getText();
    for (int i = 0; i < watcherCount; i++) {
      watchers[i].afterTextChanged(text);
    }
  }

  private native void nativeRegisterTextChangedListener();
}
