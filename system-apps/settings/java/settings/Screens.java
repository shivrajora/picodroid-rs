// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.text.TextUtils;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;

/**
 * The settings screens' one shape: a column of {@link #ROW_HEIGHT}-pixel rows under a header row of
 * the same height, in a ScrollView. Every row spans the display and holds one line — a long label
 * is cut with an ellipsis, and a suffix (a version, the storage numbers) always shows — so a test
 * can tap row {@code n} at {@code y = 20 + 40 * n} with the header as row 0; a focusable row also
 * takes the keypad's select, and every row is focusable so a column longer than the screen can be
 * walked — and scrolled — with the buttons.
 */
final class Screens {
  /** Row height in pixels, header included. */
  static final int ROW_HEIGHT = 40;

  private static final int HEADER_COLOR = 0xFF1F8A8A;

  /** Side padding of a row, each side. */
  private static final int PAD_X = 8;

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
    View row = row(a, title, null, onClick);
    row.setBackground(new GradientDrawable().setColor(HEADER_COLOR).setCornerRadius(0));
    return row;
  }

  /** A focusable row that runs {@code onClick} on a tap or the keypad's select. */
  static View row(Activity a, String label, View.OnClickListener onClick) {
    return row(a, label, null, onClick);
  }

  /** A row of information: focusable, so the keypad can walk (and scroll) past it, but inert. */
  static View info(Activity a, String s) {
    return row(a, s, null, null);
  }

  /** {@link #info} with a {@code suffix} that always shows; the label is cut instead. */
  static View info(Activity a, String label, String suffix) {
    return row(a, label, suffix, null);
  }

  /**
   * A row: {@code label} on one line, cut with an ellipsis when it does not fit, then {@code
   * suffix} (a version, the storage numbers), which always shows. Focusable; {@code onClick} may be
   * null. The row is a horizontal layout, so the label takes what the suffix leaves and both sit
   * centred on the row's height.
   */
  static View row(Activity a, String label, String suffix, View.OnClickListener onClick) {
    LinearLayout row = new LinearLayout();
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(a.getDisplay().getWidth(), ROW_HEIGHT);
    row.setPadding(PAD_X, 0, PAD_X, 0);
    row.setSpacing(0);
    TextView text = new TextView();
    text.setText(label);
    text.setTextColor(Color.WHITE);
    text.setSingleLine();
    text.setEllipsize(TextUtils.TruncateAt.END);
    row.addView(text, new LinearLayout.LayoutParams(0, View.WRAP_CONTENT, 1f));
    if (suffix != null) {
      TextView tail = new TextView();
      tail.setText(suffix);
      tail.setTextColor(Color.WHITE);
      row.addView(tail);
    }
    row.setFocusable(true);
    if (onClick != null) {
      row.setOnClickListener(onClick);
    }
    return row;
  }

  /** Bytes as whole kilobytes, for a row. */
  static String kb(long bytes) {
    return "" + (bytes / 1024L);
  }
}
