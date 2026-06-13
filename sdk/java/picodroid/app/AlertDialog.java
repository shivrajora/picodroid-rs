// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.DialogInterface;

public class AlertDialog implements DialogInterface {
  // Match Android values verbatim so apps porting in idioms like
  // `which == DialogInterface.BUTTON_POSITIVE` keep working.
  static final int WHICH_POSITIVE_INTERNAL = 0;
  static final int WHICH_NEGATIVE_INTERNAL = 1;
  static final int WHICH_NEUTRAL_INTERNAL = 2;

  private final int nativeHandle;
  private DialogInterface.OnClickListener positiveListener;
  private DialogInterface.OnClickListener negativeListener;
  private DialogInterface.OnClickListener neutralListener;

  private AlertDialog(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public void show() {
    nativeShow(nativeHandle);
  }

  @Override
  public void dismiss() {
    nativeDismiss(nativeHandle);
  }

  @Override
  public void cancel() {
    // Match Android: cancel is just dismiss for a basic AlertDialog.
    dismiss();
  }

  /**
   * Invoked from the native event loop when one of the dialog's buttons is clicked. Runs the
   * matching listener (if any) and then dismisses the dialog, mirroring Android's default behavior.
   * The native layer passes 0 for positive and 1 for negative; we translate to the canonical {@code
   * DialogInterface.BUTTON_POSITIVE} / {@code BUTTON_NEGATIVE} values before invoking the listener.
   */
  void fireButtonClick(int whichInternal) {
    DialogInterface.OnClickListener l;
    int which;
    if (whichInternal == WHICH_POSITIVE_INTERNAL) {
      l = positiveListener;
      which = DialogInterface.BUTTON_POSITIVE;
    } else if (whichInternal == WHICH_NEGATIVE_INTERNAL) {
      l = negativeListener;
      which = DialogInterface.BUTTON_NEGATIVE;
    } else {
      l = neutralListener;
      which = DialogInterface.BUTTON_NEUTRAL;
    }
    if (l != null) {
      l.onClick(this, which);
    }
    dismiss();
  }

  private static native int nativeCreate(
      String title, String message, String positiveText, String negativeText, String neutralText);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeDismiss(int nativeHandle);

  private native void nativeRegisterButtonClickListener();

  public static class Builder {
    private String title = "";
    private String message = "";
    private String positiveText;
    private DialogInterface.OnClickListener positiveListener;
    private String negativeText;
    private DialogInterface.OnClickListener negativeListener;
    private String neutralText;
    private DialogInterface.OnClickListener neutralListener;

    public Builder() {}

    public Builder(Context ctx) {}

    public Builder setTitle(String title) {
      this.title = title;
      return this;
    }

    public Builder setMessage(String message) {
      this.message = message;
      return this;
    }

    public Builder setPositiveButton(String text, DialogInterface.OnClickListener listener) {
      this.positiveText = text;
      this.positiveListener = listener;
      return this;
    }

    public Builder setNegativeButton(String text, DialogInterface.OnClickListener listener) {
      this.negativeText = text;
      this.negativeListener = listener;
      return this;
    }

    /** Mirrors {@code AlertDialog.Builder#setNeutralButton}; placed leftmost, Android-style. */
    public Builder setNeutralButton(String text, DialogInterface.OnClickListener listener) {
      this.neutralText = text;
      this.neutralListener = listener;
      return this;
    }

    public AlertDialog create() {
      AlertDialog d =
          new AlertDialog(
              nativeCreate(
                  title,
                  message,
                  positiveText == null ? "" : positiveText,
                  negativeText == null ? "" : negativeText,
                  neutralText == null ? "" : neutralText));
      d.positiveListener = positiveListener;
      d.negativeListener = negativeListener;
      d.neutralListener = neutralListener;
      d.nativeRegisterButtonClickListener();
      return d;
    }

    public AlertDialog show() {
      AlertDialog d = create();
      d.show();
      return d;
    }
  }
}
