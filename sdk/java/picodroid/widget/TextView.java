// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.text.TextUtils;
import picodroid.view.View;

public class TextView extends View {
  /** Low bits of {@link #mLineMode}: 0 = no ellipsize, else {@code TruncateAt.ordinal() + 1}. */
  private static final int ELLIPSIZE_MASK = 0x7;

  /** Bit of {@link #mLineMode} set by {@link #setSingleLine}. */
  private static final int SINGLE_LINE = 0x8;

  /** {@link #setMaxLines}'s count sits above this shift in {@link #mLineMode}; 0 = no limit. */
  private static final int MAX_LINES_SHIFT = 8;

  /** The largest line limit the packed field holds. */
  private static final int MAX_LINES_LIMIT = 0xFFFF;

  /**
   * The line mode — the ellipsize kind, the single-line flag and the max-lines count — packed into
   * one field so it costs a label one slot. Zero is the default: wrap, no ellipsis, no limit.
   */
  private int mLineMode;

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
   * Mirrors Android's {@code TextView.setIncludeFontPadding(boolean)}. When {@code false}, strips
   * the font's top side-bearing whitespace so the label box hugs the glyphs, balancing the visible
   * gap above and below the label inside a {@link LinearLayout}. Default {@code true}.
   */
  public void setIncludeFontPadding(boolean include) {
    nativeSetIncludeFontPadding(include);
    if (mLineMode != 0) {
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
      mLineMode &= ELLIPSIZE_MASK;
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
    if (mLineMode != 0) {
      applyLineMode();
    }
  }

  private void applyLineMode() {
    nativeSetLineMode(
        mLineMode & ELLIPSIZE_MASK, mLineMode >>> MAX_LINES_SHIFT, (mLineMode & SINGLE_LINE) != 0);
  }

  private native void nativeSetLineMode(int ellipsize, int maxLines, boolean singleLine);
}
