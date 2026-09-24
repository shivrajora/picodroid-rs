// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.content.res.Resources;
import picodroid.text.TextUtils;
import picodroid.util.TypedValue;
import picodroid.view.Gravity;
import picodroid.view.View;

public class TextView extends View {
  /** Low bits of {@link #mLineMode}: 0 = no ellipsize, else {@code TruncateAt.ordinal() + 1}. */
  private static final int ELLIPSIZE_MASK = 0x7;

  /** Bit of {@link #mLineMode} set by {@link #setSingleLine}. */
  private static final int SINGLE_LINE = 0x8;

  /**
   * Bit of {@link #mLineMode} set by {@link #setIncludeFontPadding setIncludeFontPadding(false)}.
   * Not a line mode: it rides in the same slot so a text-size change can re-apply it, since the
   * trimmed leading is measured from the face.
   */
  private static final int FONT_PAD_OFF = 0x10;

  /** {@link #setMaxLines}'s count sits above this shift in {@link #mLineMode}; 0 = no limit. */
  private static final int MAX_LINES_SHIFT = 8;

  /** The largest line limit the packed field holds. */
  private static final int MAX_LINES_LIMIT = 0xFFFF;

  /**
   * The size of the default face in pixels: what a fresh label reports from {@link #getTextSize}.
   */
  private static final float DEFAULT_TEXT_SIZE = 14f;

  /**
   * The line mode — the ellipsize kind, the single-line flag and the max-lines count — packed into
   * one field so it costs a label one slot, with the {@link #FONT_PAD_OFF} flag beside them. Zero
   * is the default: wrap, no ellipsis, no limit, font padding kept.
   */
  private int mLineMode;

  /** The size last set by {@link #setTextSize}, in pixels; the face in use may differ. */
  private float mTextSize = DEFAULT_TEXT_SIZE;

  /** The gravity last set by {@link #setGravity}; Android's default is {@code TOP | START}. */
  private int mGravity = Gravity.TOP | Gravity.START;

  public TextView() {
    super(nativeCreate());
  }

  public TextView(Context ctx) {
    super(nativeCreate());
  }

  /**
   * For subclasses whose native object is not a bare label ({@link Button}: a button with a child
   * label) — they create their own object and hand the handle up.
   */
  protected TextView(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  public native void setText(String text);

  /**
   * Mirrors Android's {@code TextView.getText()}: the label's current text as a {@link
   * CharSequence} (a {@link String} at runtime), so the {@code getText().toString()} idiom works
   * unchanged. Returns an empty string for a label with no text. The full text, even while an
   * ellipsis is shown.
   */
  public native CharSequence getText();

  public native void setTextColor(int argb);

  /**
   * Mirrors Android's {@code TextView.setTextSize(float)}: the text size in scaled pixels, which
   * are pixels here (see {@link picodroid.util.DisplayMetrics}). Default 14, the size of the face
   * every label starts in.
   *
   * <p>Divergence: the faces are bitmaps, one per size the board compiles — its {@code text_sizes}
   * ladder, 14, 20, 28 and 64 on the RP2350 boards and 14 alone on the RP2040 — so the text renders
   * in the compiled face nearest the size asked for, a tie going to the larger. {@link
   * #getTextSize} returns the size asked for, as on Android; {@link #getLineHeight} reports the
   * face in use. A single-line or max-lines limit and {@link #setIncludeFontPadding} follow the new
   * face.
   */
  public void setTextSize(float size) {
    setTextSize(TypedValue.COMPLEX_UNIT_SP, size);
  }

  /**
   * Mirrors Android's {@code TextView.setTextSize(int, float)}: {@code size} in one of {@link
   * TypedValue}'s {@code COMPLEX_UNIT_*} units, converted to pixels by {@link
   * TypedValue#applyDimension}. See {@link #setTextSize(float)} for how a size becomes a face.
   */
  public void setTextSize(int unit, float size) {
    float px = TypedValue.applyDimension(unit, size, Resources.getInstance().getDisplayMetrics());
    if (px == mTextSize) {
      return;
    }
    mTextSize = px;
    nativeSetTextSize(px);
    // The trimmed leading and the line cap are both measured from the face, so both follow it;
    // the pads first, because the cap includes them.
    if ((mLineMode & FONT_PAD_OFF) != 0) {
      nativeSetIncludeFontPadding(false);
    }
    if (hasLineMode()) {
      applyLineMode();
    }
  }

  /** Mirrors Android: the size set by {@link #setTextSize}, in pixels; 14 until one is set. */
  public float getTextSize() {
    return mTextSize;
  }

  /**
   * Mirrors Android's {@code TextView.getLineHeight()}: the height of one line of text in the face
   * in use, in pixels — 16 for the default face. The way to learn which face {@link #setTextSize}
   * landed on.
   */
  public int getLineHeight() {
    return nativeGetLineHeight();
  }

  private native void nativeSetTextSize(float px);

  private native int nativeGetLineHeight();

  /**
   * Mirrors Android's {@code TextView.setGravity(int)}: where the text sits inside the view when
   * the view is wider than its text — {@link Gravity#LEFT} / {@link Gravity#START} (the default),
   * {@link Gravity#CENTER_HORIZONTAL} or {@link Gravity#RIGHT} / {@link Gravity#END}. A {@code
   * wrap_content} view is exactly its text's width, so the horizontal gravity only shows on a view
   * given a width (a fixed size, {@code match_parent}, or a {@code layout_weight}).
   *
   * <p>The vertical half ({@link Gravity#TOP}, {@link Gravity#CENTER_VERTICAL}, {@link
   * Gravity#BOTTOM}) is kept for {@link #getGravity} but not drawn: an LVGL label is always its
   * text's height, so there is no spare room to place the text in. To centre a label in a taller
   * row, give the parent {@code LinearLayout} the vertical gravity instead.
   */
  public void setGravity(int gravity) {
    if ((gravity & Gravity.HORIZONTAL_GRAVITY_MASK) == 0) {
      gravity |= Gravity.START;
    }
    if ((gravity & Gravity.VERTICAL_GRAVITY_MASK) == 0) {
      gravity |= Gravity.TOP;
    }
    mGravity = gravity;
    nativeSetGravity(gravity);
  }

  /** Mirrors Android: the gravity set by {@link #setGravity}, {@code TOP | START} until one is. */
  public int getGravity() {
    return mGravity;
  }

  private native void nativeSetGravity(int gravity);

  /**
   * Mirrors Android's {@code TextView.setIncludeFontPadding(boolean)}. When {@code false}, strips
   * the font's top side-bearing whitespace so the label box hugs the glyphs, balancing the visible
   * gap above and below the label inside a {@link LinearLayout}. Default {@code true}.
   */
  public void setIncludeFontPadding(boolean include) {
    mLineMode = include ? mLineMode & ~FONT_PAD_OFF : mLineMode | FONT_PAD_OFF;
    nativeSetIncludeFontPadding(include);
    if (hasLineMode()) {
      applyLineMode();
    }
  }

  private native void nativeSetIncludeFontPadding(boolean include);

  /** Mirrors Android: {@link #setSingleLine(boolean) setSingleLine(true)}. */
  public void setSingleLine() {
    setSingleLine(true);
  }

  /**
   * Mirrors Android's {@code TextView.setSingleLine(boolean)}: the text stays on one line instead
   * of wrapping, and the view is at most one line tall. Without an ellipsize the text is clipped at
   * the view's edge (a content-sized view grows to the text's width instead); with {@link
   * #setEllipsize} it is cut with dots. Divergences: a taller explicit height shrinks to one line
   * (Android keeps the box and draws at the top), and a newline in the text still breaks the line.
   * {@code false} lifts the limit, and any {@link #setMaxLines} limit with it, as on Android.
   */
  public void setSingleLine(boolean singleLine) {
    if (singleLine) {
      mLineMode |= SINGLE_LINE;
    } else {
      mLineMode &= ELLIPSIZE_MASK | FONT_PAD_OFF;
    }
    applyLineMode();
  }

  /**
   * Mirrors Android's {@code TextView.setEllipsize(TextUtils.TruncateAt)}: how text that does not
   * fit is cut. {@code END} — and {@code START} and {@code MIDDLE}, which render the same way here
   * — cuts the last line that fits with three ASCII dots (the bundled font has no U+2026 glyph), so
   * it shows on a {@link #setSingleLine single-line} or {@link #setMaxLines max-lines} view, or one
   * whose height is fixed; a view that sizes to its content never runs out of room. {@code MARQUEE}
   * scrolls the text circularly whenever it is wider than the view (Android only while the view is
   * selected). {@code null} clears it. {@link #getText} returns the full text either way.
   */
  @SuppressWarnings("EnumOrdinal") // the kind is packed by ordinal on purpose; the enum is ours
  public void setEllipsize(TextUtils.TruncateAt where) {
    int kind = where == null ? 0 : where.ordinal() + 1;
    mLineMode = (mLineMode & ~ELLIPSIZE_MASK) | kind;
    applyLineMode();
  }

  /** Mirrors Android: the kind set by {@link #setEllipsize}, or {@code null} for none. */
  public TextUtils.TruncateAt getEllipsize() {
    int kind = mLineMode & ELLIPSIZE_MASK;
    if (kind == 1) {
      return TextUtils.TruncateAt.START;
    }
    if (kind == 2) {
      return TextUtils.TruncateAt.MIDDLE;
    }
    if (kind == 3) {
      return TextUtils.TruncateAt.END;
    }
    if (kind == 4) {
      return TextUtils.TruncateAt.MARQUEE;
    }
    return null;
  }

  /**
   * Mirrors Android's {@code TextView.setMaxLines(int)}: the view is at most {@code maxLines} lines
   * tall — the text wraps that far and the rest is clipped, or cut with dots under {@link
   * #setEllipsize}. A value below 1 lifts the limit. A taller explicit height shrinks to the limit.
   */
  public void setMaxLines(int maxLines) {
    int lines = maxLines < 1 ? 0 : maxLines;
    if (lines > MAX_LINES_LIMIT) {
      lines = MAX_LINES_LIMIT;
    }
    mLineMode = (mLineMode & ((1 << MAX_LINES_SHIFT) - 1)) | (lines << MAX_LINES_SHIFT);
    applyLineMode();
  }

  /**
   * Mirrors Android: the limit set by {@link #setMaxLines}, 1 under {@link #setSingleLine}, {@code
   * Integer.MAX_VALUE} for none.
   */
  public int getMaxLines() {
    if ((mLineMode & SINGLE_LINE) != 0) {
      return 1;
    }
    int lines = mLineMode >>> MAX_LINES_SHIFT;
    return lines == 0 ? Integer.MAX_VALUE : lines;
  }

  /**
   * The line limit is a box height that includes the padding, so a padding change re-applies it.
   */
  @Override
  public void setPadding(int left, int top, int right, int bottom) {
    super.setPadding(left, top, right, bottom);
    if (hasLineMode()) {
      applyLineMode();
    }
  }

  /** Whether any line mode is set — the {@link #FONT_PAD_OFF} flag is not one. */
  private boolean hasLineMode() {
    return (mLineMode & ~FONT_PAD_OFF) != 0;
  }

  private void applyLineMode() {
    nativeSetLineMode(
        mLineMode & ELLIPSIZE_MASK, mLineMode >>> MAX_LINES_SHIFT, (mLineMode & SINGLE_LINE) != 0);
  }

  private native void nativeSetLineMode(int ellipsize, int maxLines, boolean singleLine);
}
