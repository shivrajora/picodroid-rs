// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Interface for dialog hosts that can be dismissed and notify a click on one of their buttons.
 * Mirrors {@code android.content.DialogInterface}. Implemented by {@link
 * picodroid.app.AlertDialog}; passed to {@link OnClickListener#onClick} so listeners can call
 * {@link #dismiss()} or read state without holding a typed reference to the concrete dialog.
 */
public interface DialogInterface {
  /** Identifies the positive (confirm/OK) button. Matches Android's value. */
  int BUTTON_POSITIVE = -1;

  /** Identifies the negative (cancel) button. Matches Android's value. */
  int BUTTON_NEGATIVE = -2;

  /** Identifies the neutral button. Matches Android's value. */
  int BUTTON_NEUTRAL = -3;

  void dismiss();

  void cancel();

  /** Click callback for a dialog button. {@code which} is one of the {@code BUTTON_*} constants. */
  interface OnClickListener {
    void onClick(DialogInterface dialog, int which);
  }

  /**
   * Click callback for a multi-choice list row. Mirrors {@code
   * android.content.DialogInterface.OnMultiChoiceClickListener}: {@code which} is the row index and
   * {@code isChecked} its new state.
   */
  interface OnMultiChoiceClickListener {
    void onClick(DialogInterface dialog, int which, boolean isChecked);
  }

  /** Notification when a dialog is dismissed (via button, BACK key, or {@link #dismiss()}). */
  interface OnDismissListener {
    void onDismiss(DialogInterface dialog);
  }
}
