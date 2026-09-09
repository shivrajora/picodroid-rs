// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The settings screens' one shape: a column of {@link #ROW_HEIGHT}-pixel rows under a header row of
 * the same height. Every row spans the display, so a test can tap row {@code n} at {@code y = 20 +
 * 40 * n} with the header as row 0; a focusable row also takes the keypad's select.
 */
final class Screens {
  /** Row height in pixels, header included. */
  static final int ROW_HEIGHT = 40;

  private static final int HEADER_COLOR = 0xFF1F8A8A;

  private Screens() {}

  /** A full-screen vertical column with no padding, so rows start at y = 0. */
  static LinearLayout column(Activity a) {
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(a.getDisplay().getWidth(), a.getDisplay().getHeight());
    root.setPadding(0, 0, 0, 0);
    root.setSpacing(0);
    return root;
  }

  /** The header row: the screen's title on a tinted band; a tap or select runs {@code onClick}. */
  static View header(Activity a, String title, View.OnClickListener onClick) {
    TextView t = text(a, title);
    t.setBackground(new GradientDrawable().setColor(HEADER_COLOR).setCornerRadius(0));
    t.setFocusable(true);
    t.setOnClickListener(onClick);
    return t;
  }

  /** A focusable row that runs {@code onClick} on a tap or the keypad's select. */
  static View row(Activity a, String label, View.OnClickListener onClick) {
    TextView t = text(a, label);
    t.setFocusable(true);
    t.setOnClickListener(onClick);
    return t;
  }

  /** A plain row of text. */
  static TextView text(Activity a, String s) {
    TextView t = new TextView();
    t.setSize(a.getDisplay().getWidth(), ROW_HEIGHT);
    t.setPadding(8, 10, 8, 0);
    t.setText(s);
    t.setTextColor(Color.WHITE);
    return t;
  }

  /** Bytes as whole kilobytes, for a row. */
  static String kb(long bytes) {
    return "" + (bytes / 1024L);
  }
}
