// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.content.res.ColorStateList;
import picodroid.view.View;

/** The little the screens share in code; their layouts are {@code res/layout}. */
final class Ui {
  /** Stale numbers are dimmed to this, so they never read as live. */
  static final float DIM = 0.35f;

  /** {@link #DIM} as a colour's alpha. */
  static final int DIM_ALPHA = (int) (DIM * 255);

  private Ui() {}

  /** Recolours a shape background: the shape stays, and the same colour again costs nothing. */
  static void tint(View shape, int color) {
    shape.setBackgroundTintList(ColorStateList.valueOf(color));
  }
}
