// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.Context;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Metrics and the few building blocks every page shares. The chrome comes from {@code
 * res/layout/activity_main.xml}; pages are placed absolutely, because they are built a few views
 * per tick and their geometry is pixel art tuned to the 320x240 panel.
 */
final class Ui {
  static final int WIDTH = 320;

  /** Between the header and the footer; keep in step with {@code @dimen/page_height}. */
  static final int PAGE_HEIGHT = 190;

  static final int MARGIN = 8;
  static final int CARD_WIDTH = WIDTH - 2 * MARGIN;
  static final int CARD_PAD = 12;
  static final int LINE_HEIGHT = 18;

  /** Stale numbers are dimmed to this, so they never read as live. */
  static final float DIM = 0.35f;

  /** The display face: the one figure per page that has to read across the room. */
  static final int DISPLAY_SIZE = 64;

  /**
   * A {@link #DISPLAY_SIZE} label's box with the font padding trimmed: the face's 66 px line less
   * the 7 px of leading above its digits and 1 px of descent. The digits fill it from the top.
   */
  static final int DISPLAY_BOX = 58;

  /** GradientDrawable carries the colour's alpha through to the background opacity. */
  static final int TRANSPARENT = 0x00000000;

  private Ui() {}

  /**
   * Strips the theme's border from an inflated container. The layout compiler has no attribute for
   * it, so the chrome does this once after {@code setContentView}.
   */
  static void flat(View v, int color) {
    new GradientDrawable().setColor(color).setCornerRadius(0).setStroke(0, color).applyTo(v);
  }

  /** A container that draws nothing itself. */
  static FrameLayout group(Context ctx, int x, int y, int width, int height, int backdrop) {
    FrameLayout f = new FrameLayout(ctx);
    f.setSize(width, height);
    f.setPosition(x, y);
    f.setPadding(0, 0, 0, 0);
    f.setBackground(new GradientDrawable().setColor(backdrop).setCornerRadius(0));
    return f;
  }

  static FrameLayout card(Context ctx, ViewGroup parent, int y, int height, int color) {
    return card(ctx, parent, MARGIN, y, CARD_WIDTH, height, color);
  }

  static FrameLayout card(
      Context ctx, ViewGroup parent, int x, int y, int width, int height, int color) {
    FrameLayout f = box(ctx, x, y, width, height, color, 12);
    parent.addView(f);
    return f;
  }

  /** A filled rounded rectangle: bars, dots, pills, markers. */
  static FrameLayout box(Context ctx, int x, int y, int width, int height, int color, int radius) {
    FrameLayout f = new FrameLayout(ctx);
    f.setSize(width, height);
    f.setPosition(x, y);
    f.setPadding(0, 0, 0, 0);
    f.setBackground(new GradientDrawable().setColor(color).setCornerRadius(radius));
    return f;
  }

  static void fill(FrameLayout box, int color, int radius) {
    box.setBackground(new GradientDrawable().setColor(color).setCornerRadius(radius));
  }

  static TextView label(Context ctx, ViewGroup parent, String text, int x, int y, int color) {
    TextView t = new TextView(ctx);
    t.setText(text);
    t.setTextColor(color);
    t.setSingleLine();
    t.setPosition(x, y);
    parent.addView(t);
    return t;
  }

  /** A label whose text ends at {@code x + width}, whatever its length. */
  static TextView labelRight(
      Context ctx, ViewGroup parent, String text, int x, int y, int width, int color) {
    return aligned(ctx, parent, text, x, y, width, LINE_HEIGHT, color, Gravity.RIGHT);
  }

  static TextView labelCentred(
      Context ctx, ViewGroup parent, String text, int x, int y, int width, int color) {
    return aligned(ctx, parent, text, x, y, width, LINE_HEIGHT, color, Gravity.CENTER_HORIZONTAL);
  }

  /** A centred label in a row {@code height} tall, for a face taller than {@link #LINE_HEIGHT}. */
  static TextView labelCentred(
      Context ctx, ViewGroup parent, String text, int x, int y, int width, int height, int color) {
    return aligned(ctx, parent, text, x, y, width, height, color, Gravity.CENTER_HORIZONTAL);
  }

  /** A transparent horizontal row that places its children by {@code gravity}. */
  static LinearLayout row(Context ctx, int x, int y, int width, int height, int gravity) {
    LinearLayout row = new LinearLayout(ctx);
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(width, height);
    row.setPosition(x, y);
    row.setPadding(0, 0, 0, 0);
    row.setSpacing(0);
    row.setGravity(gravity);
    row.setBackground(new GradientDrawable().setColor(TRANSPARENT).setCornerRadius(0));
    return row;
  }

  private static TextView aligned(
      Context ctx,
      ViewGroup parent,
      String text,
      int x,
      int y,
      int width,
      int height,
      int color,
      int gravity) {
    LinearLayout row = row(ctx, x, y, width, height, gravity | Gravity.CENTER_VERTICAL);
    TextView t = new TextView(ctx);
    t.setText(text);
    t.setTextColor(color);
    t.setSingleLine();
    row.addView(t);
    parent.addView(row);
    return t;
  }
}
