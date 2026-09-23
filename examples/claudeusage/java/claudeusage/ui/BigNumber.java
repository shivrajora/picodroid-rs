// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.Context;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.ImageView;

/**
 * A large number drawn from glyph images, because the SDK renders text at one size only. The
 * sprites come from tools/gen_digits.py and are drawn onto the card colour (assets carry no alpha).
 */
final class BigNumber {
  static final int HEIGHT = 44;

  private static final int DIGIT_WIDTH = 28;
  private static final int PERCENT_WIDTH = 43;
  private static final int DASH_WIDTH = 17;
  private static final int SLOTS = 5;

  private static final int[] DIGITS = {
    R.drawable.d0,
    R.drawable.d1,
    R.drawable.d2,
    R.drawable.d3,
    R.drawable.d4,
    R.drawable.d5,
    R.drawable.d6,
    R.drawable.d7,
    R.drawable.d8,
    R.drawable.d9
  };

  private final Context ctx;
  private final ImageView[] slots = new ImageView[SLOTS];

  /** The drawable each slot shows; 0 while it is hidden. */
  private final int[] shown = new int[SLOTS];

  private final ViewGroup parent;
  private final int x;
  private final int y;

  /** Whether {@code x} is the text's centre rather than its left edge. */
  private final boolean centred;

  private String shownText = "";
  private boolean shownDim;

  BigNumber(Context ctx, ViewGroup parent, int x, int y) {
    this(ctx, parent, x, y, false);
  }

  BigNumber(Context ctx, ViewGroup parent, int x, int y, boolean centred) {
    this.ctx = ctx;
    this.parent = parent;
    this.x = x;
    this.y = y;
    this.centred = centred;
  }

  /** "42%", or "--%" when {@code value} is negative. Returns the width drawn. */
  int showPercent(int value, boolean dim) {
    return show(value < 0 ? "--%" : value + "%", dim);
  }

  /** "+12" with an explicit sign, for a rate. */
  int showSigned(int value, boolean dim) {
    return show(value < 0 ? "--" : "+" + value, dim);
  }

  private int show(String text, boolean dim) {
    if (!text.equals(shownText)) {
      shownText = text;
      int cx = centred ? x - widthOf(text) / 2 : x;
      for (int i = 0; i < SLOTS; i++) {
        if (i >= text.length()) {
          if (slots[i] != null && shown[i] != 0) {
            slots[i].setVisibility(View.INVISIBLE);
            shown[i] = 0;
          }
          continue;
        }
        char c = text.charAt(i);
        int glyph = glyph(c);
        if (slots[i] == null) {
          slots[i] = new ImageView(ctx);
          parent.addView(slots[i]);
          if (shownDim) {
            slots[i].setAlpha(Ui.DIM);
          }
        }
        // Glyphs differ in width, so a slot's position depends on what precedes it.
        slots[i].setPosition(cx, y);
        if (glyph != shown[i]) {
          slots[i].setImageResource(glyph);
          slots[i].setVisibility(View.VISIBLE);
          shown[i] = glyph;
        }
        cx += width(c);
      }
    }
    if (dim != shownDim) {
      shownDim = dim;
      for (int i = 0; i < SLOTS; i++) {
        if (slots[i] != null) {
          slots[i].setAlpha(dim ? Ui.DIM : 1f);
        }
      }
    }
    return widthOf(text);
  }

  private static int widthOf(String text) {
    int w = 0;
    for (int i = 0; i < text.length(); i++) {
      w += width(text.charAt(i));
    }
    return w;
  }

  private static int glyph(char c) {
    if (c >= '0' && c <= '9') {
      return DIGITS[c - '0'];
    }
    return c == '%' ? R.drawable.pct : (c == '+' ? R.drawable.plus : R.drawable.dash);
  }

  private static int width(char c) {
    return c == '%' ? PERCENT_WIDTH : (c == '-' ? DASH_WIDTH : DIGIT_WIDTH);
  }
}
