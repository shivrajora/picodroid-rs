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

  // List render modes, shared with the native side via nativeCreateWithList.
  static final int LIST_MODE_ITEMS = 0;
  static final int LIST_MODE_SINGLE = 1;
  static final int LIST_MODE_MULTI = 2;

  // Hard cap matching the native renderer constraint (focusable rows beyond
  // ~12 stall the 48 KB LVGL renderer). A button-matrix is one object, but
  // the cap stays as documented defence — the Builder throws past it.
  static final int MAX_LIST_ITEMS = 12;

  private final int nativeHandle;
  private DialogInterface.OnClickListener positiveListener;
  private DialogInterface.OnClickListener negativeListener;
  private DialogInterface.OnClickListener neutralListener;

  // List state (populated for setItems / setSingleChoiceItems variants).
  private int listMode = LIST_MODE_ITEMS;
  private DialogInterface.OnClickListener itemsListener;

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

  /**
   * Invoked from the native event loop when a list row is clicked. {@code position} is the row
   * index; {@code checked} is its post-click state (always {@code true} for plain item lists).
   * Plain item lists dismiss after the callback (Android); choice lists stay open.
   */
  void fireItemClick(int position, boolean checked) {
    if (listMode == LIST_MODE_ITEMS) {
      if (itemsListener != null) {
        itemsListener.onClick(this, position);
      }
      dismiss();
    } else if (listMode == LIST_MODE_SINGLE) {
      // Single-choice: which == the selected position, dialog stays open.
      if (itemsListener != null) {
        itemsListener.onClick(this, position);
      }
    }
    // LIST_MODE_MULTI is handled by an override added with the multi-choice
    // variant; this base routing leaves it to that subclass logic.
  }

  private static native int nativeCreate(
      String title, String message, String positiveText, String negativeText, String neutralText);

  private static native int nativeCreateWithList(
      String title,
      String message,
      String positiveText,
      String negativeText,
      String neutralText,
      String itemsJoined,
      int mode,
      int checkedMask);

  private static native void nativeShow(int nativeHandle);

  /**
   * Synthetically click list row {@code position} for headless testing — drives {@link
   * #fireItemClick} through the real dispatch queue (and the choice-mode checkable toggle).
   */
  public void performItemClick(int position) {
    nativePerformItemClick(nativeHandle, position);
  }

  private static native void nativePerformItemClick(int nativeHandle, int position);

  private static native void nativeDismiss(int nativeHandle);

  private native void nativeRegisterButtonClickListener();

  /**
   * Joins {@code items} with {@code '\n'}, enforcing the {@link #MAX_LIST_ITEMS} cap. Picodroid
   * divergence: Android's list setters take {@code CharSequence[]}; picodroid uses {@code String[]}
   * (the SDK has no {@code CharSequence}).
   */
  private static String joinItems(String[] items) {
    if (items.length > MAX_LIST_ITEMS) {
      throw new IllegalArgumentException(
          "AlertDialog list capped at "
              + MAX_LIST_ITEMS
              + " items (LVGL renderer constraint); got "
              + items.length);
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < items.length; i++) {
      if (i > 0) {
        sb.append('\n');
      }
      sb.append(items[i] == null ? "" : items[i]);
    }
    return sb.toString();
  }

  public static class Builder {
    private String title = "";
    private String message = "";
    private String positiveText;
    private DialogInterface.OnClickListener positiveListener;
    private String negativeText;
    private DialogInterface.OnClickListener negativeListener;
    private String neutralText;
    private DialogInterface.OnClickListener neutralListener;
    private String itemsJoined;
    private int listMode = LIST_MODE_ITEMS;
    private int checkedMask;
    private DialogInterface.OnClickListener itemsListener;

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

    /**
     * Mirrors {@code AlertDialog.Builder#setItems}: a tappable list, each row dismissing the dialog
     * and reporting its index to {@code listener.onClick(dialog, position)}. Capped at {@link
     * #MAX_LIST_ITEMS} rows (throws {@code IllegalArgumentException} past it). Per Android, a
     * {@code setMessage} also set wins and the list is dropped (logged native-side).
     */
    public Builder setItems(String[] items, DialogInterface.OnClickListener listener) {
      this.itemsJoined = joinItems(items);
      this.listMode = LIST_MODE_ITEMS;
      this.itemsListener = listener;
      return this;
    }

    public AlertDialog create() {
      AlertDialog d;
      if (itemsJoined != null) {
        d =
            new AlertDialog(
                nativeCreateWithList(
                    title,
                    message,
                    positiveText == null ? "" : positiveText,
                    negativeText == null ? "" : negativeText,
                    neutralText == null ? "" : neutralText,
                    itemsJoined,
                    listMode,
                    checkedMask));
        d.listMode = listMode;
        d.itemsListener = itemsListener;
      } else {
        d =
            new AlertDialog(
                nativeCreate(
                    title,
                    message,
                    positiveText == null ? "" : positiveText,
                    negativeText == null ? "" : negativeText,
                    neutralText == null ? "" : neutralText));
      }
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
