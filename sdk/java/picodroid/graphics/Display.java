// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics;

import picodroid.view.View;

public class Display {
  private int width;
  private int height;

  private Display(int width, int height) {
    this.width = width;
    this.height = height;
  }

  public static native Display getInstance();

  public native void setContentView(View root);

  public native void update();

  public int getWidth() {
    return width;
  }

  public int getHeight() {
    return height;
  }
}
