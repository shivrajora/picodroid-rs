// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.KeyEvent;
import picodroid.view.inputmethod.EditorInfo;

/**
 * Callback fired when the user commits text on the system soft keyboard (taps OK / Enter). Mirrors
 * Android's {@code TextView.OnEditorActionListener}.
 *
 * <p>v1 caveats: {@code actionId} is always {@link EditorInfo#IME_ACTION_DONE} (no Search / Next /
 * Send distinctions yet); {@code event} is always {@code null} because the LVGL keyboard's OK is
 * synthesized, not a hardware keystroke.
 *
 * <p>Only fires for the implicit system keyboard. Apps that construct an explicit {@link Keyboard}
 * instance use {@link Keyboard#setOnReadyListener} instead.
 */
public interface OnEditorActionListener {
  /**
   * Return {@code true} to signal that the listener handled the action and the system should
   * suppress its default dismiss; {@code false} lets the keyboard hide as usual.
   */
  boolean onEditorAction(EditText v, int actionId, KeyEvent event);
}
