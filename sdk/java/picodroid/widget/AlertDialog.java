package picodroid.widget;

public class AlertDialog {
  static final int BUTTON_POSITIVE = 0;
  static final int BUTTON_NEGATIVE = 1;

  private final int nativeHandle;
  private Runnable positiveListener;
  private Runnable negativeListener;

  private AlertDialog(int nativeHandle) {
    this.nativeHandle = nativeHandle;
  }

  public void show() {
    nativeShow(nativeHandle);
  }

  public void dismiss() {
    nativeDismiss(nativeHandle);
  }

  /**
   * Invoked from the native event loop when one of the dialog's buttons is clicked. Runs the
   * matching listener (if any) and then dismisses the dialog, mirroring Android's default behavior.
   */
  void fireButtonClick(int which) {
    Runnable r = (which == BUTTON_POSITIVE) ? positiveListener : negativeListener;
    if (r != null) {
      r.run();
    }
    dismiss();
  }

  private static native int nativeCreate(
      String title, String message, String positiveText, String negativeText);

  private static native void nativeShow(int nativeHandle);

  private static native void nativeDismiss(int nativeHandle);

  private native void nativeRegisterButtonClickListener();

  public static class Builder {
    private String title = "";
    private String message = "";
    private String positiveText;
    private Runnable positiveListener;
    private String negativeText;
    private Runnable negativeListener;

    public Builder setTitle(String title) {
      this.title = title;
      return this;
    }

    public Builder setMessage(String message) {
      this.message = message;
      return this;
    }

    public Builder setPositiveButton(String text, Runnable listener) {
      this.positiveText = text;
      this.positiveListener = listener;
      return this;
    }

    public Builder setNegativeButton(String text, Runnable listener) {
      this.negativeText = text;
      this.negativeListener = listener;
      return this;
    }

    public AlertDialog create() {
      AlertDialog d =
          new AlertDialog(
              nativeCreate(
                  title,
                  message,
                  positiveText == null ? "" : positiveText,
                  negativeText == null ? "" : negativeText));
      d.positiveListener = positiveListener;
      d.negativeListener = negativeListener;
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
