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
    FrameLayout f = box(ctx, MARGIN, y, CARD_WIDTH, height, color, 12);
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
    return aligned(ctx, parent, text, x, y, width, color, Gravity.RIGHT);
  }

  static TextView labelCentred(
      Context ctx, ViewGroup parent, String text, int x, int y, int width, int color) {
    return aligned(ctx, parent, text, x, y, width, color, Gravity.CENTER_HORIZONTAL);
  }

  private static TextView aligned(
      Context ctx, ViewGroup parent, String text, int x, int y, int width, int color, int gravity) {
    LinearLayout row = new LinearLayout(ctx);
    row.setOrientation(LinearLayout.HORIZONTAL);
    row.setSize(width, LINE_HEIGHT);
    row.setPosition(x, y);
    row.setPadding(0, 0, 0, 0);
    row.setSpacing(0);
    row.setGravity(gravity | Gravity.CENTER_VERTICAL);
    row.setBackground(new GradientDrawable().setColor(TRANSPARENT).setCornerRadius(0));
    TextView t = new TextView(ctx);
    t.setText(text);
    t.setTextColor(color);
    t.setSingleLine();
    row.addView(t);
    parent.addView(row);
    return t;
  }
}
