// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics;

/**
 * How {@link Canvas} draws: colour, stroke, and text settings. Mirrors {@code
 * android.graphics.Paint} for the settings a {@link Canvas} honours.
 *
 * <p>Divergences: no shaders, path effects or typefaces; anti-aliasing is always on (the flag is
 * kept for source compatibility); {@link Cap#SQUARE} draws as {@link Cap#BUTT}; a text size snaps
 * to the nearest face the board compiled, as {@code TextView.setTextSize} does, and the text
 * metrics below are that face's, in whole pixels. A {@code Paint} holds only settings, so one
 * instance can be shared by every draw call of an {@code onDraw}.
 */
public class Paint {
  /** Accepted for source compatibility; picodroid always anti-aliases. */
  public static final int ANTI_ALIAS_FLAG = 0x01;

  /** Whether a shape is filled, outlined, or both. Mirrors {@code Paint.Style}. */
  public enum Style {
    FILL,
    STROKE,
    FILL_AND_STROKE
  }

  /** The end of a stroked line or arc. Mirrors {@code Paint.Cap}. */
  public enum Cap {
    BUTT,
    ROUND,
    SQUARE
  }

  /** Where {@link Canvas#drawText} places text relative to its x. Mirrors {@code Paint.Align}. */
  public enum Align {
    LEFT,
    CENTER,
    RIGHT
  }

  private static final float DEFAULT_TEXT_SIZE = 12f;

  // Read by the native draw ops by slot (graphics/fields.rs::paint): keep this order, and keep
  // the enums as ordinals so the natives read ints. The enum classes are loaded only when an app
  // names them.
  int color = Color.BLACK;
  float strokeWidth;
  int style;
  int strokeCap;
  float textSize = DEFAULT_TEXT_SIZE;
  int textAlign;
  int flags;

  /** Opaque black, filled, hairline stroke, 12 px text, left-aligned. */
  public Paint() {}

  public Paint(int flags) {
    this.flags = flags;
  }

  /** A copy of {@code src}. */
  public Paint(Paint src) {
    set(src);
  }

  /** Copies every setting from {@code src}. */
  public void set(Paint src) {
    color = src.color;
    strokeWidth = src.strokeWidth;
    style = src.style;
    strokeCap = src.strokeCap;
    textSize = src.textSize;
    textAlign = src.textAlign;
    flags = src.flags;
  }

  /** Back to the defaults of {@link #Paint()}. */
  public void reset() {
    color = Color.BLACK;
    strokeWidth = 0f;
    style = 0;
    strokeCap = 0;
    textSize = DEFAULT_TEXT_SIZE;
    textAlign = 0;
    flags = 0;
  }

  public int getFlags() {
    return flags;
  }

  public void setFlags(int flags) {
    this.flags = flags;
  }

  public boolean isAntiAlias() {
    return (flags & ANTI_ALIAS_FLAG) != 0;
  }

  public void setAntiAlias(boolean aa) {
    flags = aa ? flags | ANTI_ALIAS_FLAG : flags & ~ANTI_ALIAS_FLAG;
  }

  /** The colour, {@code 0xAARRGGBB}. */
  public int getColor() {
    return color;
  }

  /** The colour, {@code 0xAARRGGBB}; its alpha is the opacity of everything drawn with it. */
  public void setColor(int color) {
    this.color = color;
  }

  public int getAlpha() {
    return color >>> 24;
  }

  /** Replaces the colour's alpha, 0 to 255, keeping its RGB. */
  public void setAlpha(int a) {
    color = (color & 0x00FFFFFF) | ((a & 0xFF) << 24);
  }

  public void setARGB(int a, int r, int g, int b) {
    color = Color.argb(a, r, g, b);
  }

  public float getStrokeWidth() {
    return strokeWidth;
  }

  /** Stroke width in pixels; 0 is a one-pixel hairline, as on Android. */
  public void setStrokeWidth(float width) {
    strokeWidth = width;
  }

  public Style getStyle() {
    return Style.values()[style];
  }

  @SuppressWarnings("EnumOrdinal") // packed by ordinal for the natives; the enum is ours
  public void setStyle(Style style) {
    this.style = style.ordinal();
  }

  public Cap getStrokeCap() {
    return Cap.values()[strokeCap];
  }

  @SuppressWarnings("EnumOrdinal") // packed by ordinal for the natives; the enum is ours
  public void setStrokeCap(Cap cap) {
    strokeCap = cap.ordinal();
  }

  public float getTextSize() {
    return textSize;
  }

  /** Text size in pixels; snaps to the nearest face this board compiled. */
  public void setTextSize(float textSize) {
    this.textSize = textSize;
  }

  public Align getTextAlign() {
    return Align.values()[textAlign];
  }

  @SuppressWarnings("EnumOrdinal") // packed by ordinal for the natives; the enum is ours
  public void setTextAlign(Align align) {
    textAlign = align.ordinal();
  }

  /**
   * The distance from the baseline up to the top of a line of text, negative. Mirrors {@code
   * Paint.ascent()}: text whose line should start at {@code top} is drawn at baseline {@code top -
   * ascent()}.
   */
  public native float ascent();

  /** The distance from the baseline down to the bottom of a line of text. Mirrors Android. */
  public native float descent();

  /** The width of {@code text} drawn with this paint, in pixels. Mirrors Android. */
  public native float measureText(String text);
}
