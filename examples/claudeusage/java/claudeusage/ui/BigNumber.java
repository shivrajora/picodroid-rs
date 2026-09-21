// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

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

  private static final String[] DIGITS = {
    "d0.png", "d1.png", "d2.png", "d3.png", "d4.png", "d5.png", "d6.png", "d7.png", "d8.png",
    "d9.png"
  };

  private final ImageView[] slots = new ImageView[SLOTS];
  private final String[] shown = new String[SLOTS];
  private final ViewGroup parent;
  private final int x;
  private final int y;
  private String shownText = "";
  private boolean shownDim;

  BigNumber(ViewGroup parent, int x, int y) {
    this.parent = parent;
    this.x = x;
    this.y = y;
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
      int cx = x;
      for (int i = 0; i < SLOTS; i++) {
        if (i >= text.length()) {
          if (slots[i] != null && shown[i] != null) {
            slots[i].setVisibility(View.INVISIBLE);
            shown[i] = null;
          }
          continue;
        }
        char c = text.charAt(i);
        String glyph = glyph(c);
        if (slots[i] == null) {
          slots[i] = new ImageView();
          parent.addView(slots[i]);
          if (shownDim) {
            slots[i].setAlpha(Ui.DIM);
          }
        }
        // Glyphs differ in width, so a slot's position depends on what precedes it.
        slots[i].setPosition(cx, y);
        if (!glyph.equals(shown[i])) {
          slots[i].setImageSource(glyph);
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

  private static String glyph(char c) {
    if (c >= '0' && c <= '9') {
      return DIGITS[c - '0'];
    }
    return c == '%' ? "pct.png" : (c == '+' ? "plus.png" : "dash.png");
  }

  private static int width(char c) {
    return c == '%' ? PERCENT_WIDTH : (c == '-' ? DASH_WIDTH : DIGIT_WIDTH);
  }
}
