// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;

/**
 * The settings screens' one shape: a column of {@link #ROW_HEIGHT}-pixel rows under a header row of
 * the same height, in a ScrollView. Every row spans the display and holds one line, so a test can
 * tap row {@code n} at {@code y = 20 + 40 * n} with the header as row 0; a focusable row also takes
 * the keypad's select, and every row is focusable so a column longer than the screen can be walked
 * — and scrolled — with the buttons.
 */
final class Screens {
  /** Row height in pixels, header included. */
  static final int ROW_HEIGHT = 40;

  private static final int HEADER_COLOR = 0xFF1F8A8A;

  /** Side padding of a row, each side. */
  private static final int PAD_X = 8;

  /** About what one character of the default font takes, for fitting text to a row. */
  private static final int PX_PER_CHAR = 7;

  private Screens() {}

  /** A vertical column with no padding, so rows start at y = 0; sized by {@link #scrollable}. */
  static LinearLayout column(Activity a) {
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(0, 0, 0, 0);
    root.setSpacing(0);
    return root;
  }

  /**
   * The column of {@code rows} rows (header included) in a full-screen ScrollView: a LinearLayout
   * does not scroll, as on Android, so this is what makes a long column reachable — by drag on a
   * touch panel, and by the focus moving on a keypad.
   */
  static View scrollable(Activity a, LinearLayout column, int rows) {
    int width = a.getDisplay().getWidth();
    int height = a.getDisplay().getHeight();
    int content = rows * ROW_HEIGHT;
    column.setSize(width, content > height ? content : height);
    ScrollView scroller = new ScrollView();
    scroller.setSize(width, height);
    scroller.setPadding(0, 0, 0, 0);
    scroller.addView(column);
    return scroller;
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

  /** A row of information: focusable, so the keypad can walk (and scroll) past it, but inert. */
  static View info(Activity a, String s) {
    TextView t = text(a, s);
    t.setFocusable(true);
    return t;
  }

  /** A plain row of text, cut to one line. */
  static TextView text(Activity a, String s) {
    TextView t = new TextView();
    t.setSize(a.getDisplay().getWidth(), ROW_HEIGHT);
    t.setPadding(PAD_X, 10, PAD_X, 0);
    t.setText(fit(a, s, ""));
    t.setTextColor(Color.WHITE);
    return t;
  }

  /**
   * {@code label} followed by {@code suffix}, the label cut with an ellipsis so the whole fits one
   * row: the suffix (a version, the storage numbers) is always shown.
   */
  static String fit(Activity a, String label, String suffix) {
    int maxChars = (a.getDisplay().getWidth() - 2 * PAD_X) / PX_PER_CHAR - suffix.length();
    if (label.length() <= maxChars) {
      return label + suffix;
    }
    if (maxChars < 4) {
      return suffix;
    }
    return label.substring(0, maxChars - 3) + "..." + suffix;
  }

  /** Bytes as whole kilobytes, for a row. */
  static String kb(long bytes) {
    return "" + (bytes / 1024L);
  }
}
