// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.inputmethod;

/**
 * IME action codes delivered to {@link picodroid.widget.OnEditorActionListener#onEditorAction}. The
 * full {@code android.view.inputmethod.EditorInfo.IME_ACTION_*} set is defined with Android's exact
 * values so ported code compiles and comparisons stay meaningful — but only {@link
 * #IME_ACTION_DONE} is emitted by the system soft keyboard's OK key in v1 (the LVGL keyboard does
 * not yet distinguish Search/Next/Send variants).
 */
public final class EditorInfo {
  private EditorInfo() {}

  /** No specific action associated with the editor. */
  public static final int IME_ACTION_UNSPECIFIED = 0;

  /** There is no available action. */
  public static final int IME_ACTION_NONE = 1;

  /** "Go" — take the user to the target of the text, e.g. a typed URL. */
  public static final int IME_ACTION_GO = 2;

  /** "Search" — execute a search with the field's text. */
  public static final int IME_ACTION_SEARCH = 3;

  /** "Send" — deliver the text, e.g. an SMS or chat message. */
  public static final int IME_ACTION_SEND = 4;

  /** "Next" — advance focus to the next field that accepts text. */
  public static final int IME_ACTION_NEXT = 5;

  /**
   * "Done" — nothing left to do; close the keyboard. The only action the v1 system keyboard
   * actually emits.
   */
  public static final int IME_ACTION_DONE = 6;

  /** "Previous" — move focus to the previous field. */
  public static final int IME_ACTION_PREVIOUS = 7;
}
