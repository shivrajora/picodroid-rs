// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.settings.R;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;
import picodroid.widget.Toolbar;

/**
 * The settings screens' one shape: an app bar ({@link Toolbar}) of {@link #ROW_HEIGHT} pixels, then
 * a column of rows of the same height in a ScrollView that takes the rest of the screen. The bar is
 * the Activity's action bar: it shows the screen's title (its {@code android:label} in the
 * manifest) and, below the root, the up button that finishes the screen. Every row spans the
 * display and holds one line — a long label is cut with an ellipsis, and a suffix (a version, the
 * storage numbers) always shows — so a test can tap row {@code n} at {@code y = 20 + 40 * n} with
 * the bar as row 0; a focusable row also takes the keypad's select, and every row is focusable so a
 * column longer than the screen can be walked — and scrolled — with the buttons.
 *
 * <p>A screen is built inside {@code onCreate}, as on Android: {@link #screen} puts the bar up and
 * sets the content view, and the rows are inflated from {@code res/layout/row.xml} into the column
 * one call each; the column wraps them. The main loop allows a screen build a slow first frame
 * (docs/designs/ui-build-budget-2026-10.md).
 */
final class Screens {
  /** Row height in pixels, the app bar's included. */
  static final int ROW_HEIGHT = 40;

  private static final int HEADER_COLOR = 0xFF1F8A8A;

  private Screens() {}

  /**
   * A screen: the app bar over a fresh column in a ScrollView, set as {@code a}'s content view. The
   * rows follow through {@link #row} and {@link #info}. {@code up}: the bar has the up button.
   */
  static LinearLayout screen(Activity a, boolean up) {
    LinearLayout column = column(a);
    page(a, up).addView(scrollable(a, column), rest());
    return column;
  }

  /**
   * The page a screen is built in, set as {@code a}'s content view: a column holding the app bar,
   * which becomes {@code a}'s action bar. What follows it takes the rest of the screen with {@link
   * #rest}.
   */
  static LinearLayout page(Activity a, boolean up) {
    LinearLayout page = column(a);
    Toolbar bar = new Toolbar(a);
    bar.setBackgroundColor(HEADER_COLOR);
    page.addView(bar, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ROW_HEIGHT));
    a.setActionBar(bar);
    a.getActionBar().setDisplayHomeAsUpEnabled(up);
    a.setContentView(page);
    return page;
  }

  /** The layout params of what fills a {@link #page} under its bar. */
  static LinearLayout.LayoutParams rest() {
    return new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f);
  }

  /** A vertical column with no padding, so rows start at y = 0. */
  static LinearLayout column(Context context) {
    LinearLayout root = new LinearLayout(context);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(0, 0, 0, 0);
    return root;
  }

  /**
   * {@code column} in a ScrollView, which fills the page under its bar: a LinearLayout
   * does not scroll, as on Android, so this is what makes a long column reachable — by drag on a
   * touch panel, and by the focus moving on a keypad. The column takes the ScrollView's width and
   * wraps its rows.
   */
  static View scrollable(Context context, LinearLayout column) {
    ScrollView scroller = new ScrollView(context);
    scroller.setPadding(0, 0, 0, 0);
    scroller.addView(column);
    return scroller;
  }

  /** A focusable row, appended to {@code column}, that runs {@code onClick} on a tap or select. */
  static View row(Activity a, LinearLayout column, String label, View.OnClickListener onClick) {
    return build(a, column, label, null, onClick);
  }

  /** {@link #row} with a {@code suffix} that always shows; the label is cut instead. */
  static View row(
      Activity a, LinearLayout column, String label, String suffix, View.OnClickListener onClick) {
    return build(a, column, label, suffix, onClick);
  }

  /** A row of information: focusable, so the keypad can walk (and scroll) past it, but inert. */
  static View info(Activity a, LinearLayout column, String s) {
    return build(a, column, s, null, null);
  }

  /** {@link #info} with a {@code suffix} that always shows; the label is cut instead. */
  static View info(Activity a, LinearLayout column, String label, String suffix) {
    return build(a, column, label, suffix, null);
  }

  /** The suffix of a row made with one, for a screen that fills it in later. */
  static TextView tailOf(View row) {
    return row.findViewById(R.id.tail);
  }

  /**
   * A row, inflated into {@code column} as its last child: {@code label} on one line, cut with an
   * ellipsis when it does not fit, then {@code suffix} (a version, the storage numbers), which
   * always shows. Focusable; {@code suffix} and {@code onClick} may be null.
   */
  private static View build(
      Activity a, LinearLayout column, String label, String suffix, View.OnClickListener onClick) {
    a.getLayoutInflater().inflate(suffix == null ? R.layout.row : R.layout.row_tail, column, true);
    View row = column.getChildAt(column.getChildCount() - 1);
    TextView text = row.findViewById(R.id.label);
    text.setText(label);
    if (suffix != null) {
      tailOf(row).setText(suffix);
    }
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
