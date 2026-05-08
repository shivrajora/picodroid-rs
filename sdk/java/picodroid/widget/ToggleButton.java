// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

public class ToggleButton extends CompoundButton {
  public ToggleButton() {
    super(nativeCreate());
  }

  public ToggleButton(String textOn, String textOff) {
    super(nativeCreateWithText(textOn, textOff));
  }

  public ToggleButton(Context ctx) {
    super(nativeCreate());
  }

  public ToggleButton(Context ctx, String textOn, String textOff) {
    super(nativeCreateWithText(textOn, textOff));
  }

  private static native int nativeCreate();

  private static native int nativeCreateWithText(String textOn, String textOff);

  public native boolean isChecked();

  public native void setChecked(boolean checked);

  public native void toggle();

  public native void setTextOn(String text);

  public native void setTextOff(String text);
}
