// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.inputmethod;

/**
 * IME action codes delivered to {@link picodroid.widget.OnEditorActionListener#onEditorAction}.
 * Only {@link #IME_ACTION_DONE} is emitted by the system soft keyboard's OK key in v1; the rest of
 * Android's `IME_ACTION_*` constants are reserved for future use (Search/Next/Send variants are not
 * yet distinguished by the LVGL keyboard).
 */
public final class EditorInfo {
  /** Matches Android's value so apps that already know the contract pass it through verbatim. */
  public static final int IME_ACTION_DONE = 6;

  private EditorInfo() {}
}
