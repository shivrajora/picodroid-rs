// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

/**
 * Mirrors {@code android.widget.RadioButton}: a two-state button with a circular indicator. Mutual
 * exclusion is provided by placing RadioButtons inside a {@link RadioGroup} — a standalone
 * RadioButton toggles like a CheckBox, exactly as on Android.
 */
public class RadioButton extends CompoundButton {
  public RadioButton() {
    super(nativeCreate());
  }

  public RadioButton(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setText(String text);

  @Override
  public native boolean isChecked();

  @Override
  public native void setChecked(boolean checked);
}
