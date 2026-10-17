// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;

/**
 * Framework-internal: the views {@code LayoutInflater}'s native path adopts. The runtime makes a
 * framework view's widget in one native call; what it needs first is the Java wrapper to write the
 * handle and the cached fields into, and the wrapper's constructor that takes a handle is {@code
 * protected} in this package. Not an Android class: apps inflate through {@code LayoutInflater}.
 */
public final class InflaterSupport {
  private InflaterSupport() {}

  /**
   * A wrapper with no widget yet for the layout class code {@code cls} (the codes are {@code
   * papk_format::res::layout::class}; the labels here are checked against them by papk-pack's
   * {@code codes_match_the_java_sdk}), or {@code null} for a class the native path leaves to the
   * Java one.
   */
  public static View adopt(int cls) {
    switch (cls) {
      case 1: // CLASS_LINEAR_LAYOUT
        return new LinearLayout(0);
      case 2: // CLASS_FRAME_LAYOUT
        return new FrameLayout(0);
      case 3: // CLASS_SCROLL_VIEW
        return new ScrollView(0);
      case 4: // CLASS_TEXT_VIEW
        return new TextView(0);
      case 5: // CLASS_BUTTON
        return new Button(0);
      case 6: // CLASS_IMAGE_VIEW
        return new ImageView(0);
      case 12: // CLASS_RADIO_GROUP
        return new RadioGroup(0);
      case 21: // CLASS_SPACE
        return new Space(0);
      default:
        return null;
    }
  }
}
