// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics;

/**
 * The surface a {@link picodroid.view.View#onDraw} draws on. Mirrors {@code
 * android.graphics.Canvas} for shapes and text: coordinates are in pixels relative to the view's
 * top-left corner, a rectangle's right and bottom edges are exclusive, angles are degrees clockwise
 * from 3 o'clock, and everything is clipped to the view.
 *
 * <p>Picodroid keeps what {@code onDraw} drew, not the pixels: each call records one small drawing
 * op for the view, and the renderer replays them whenever it repaints the view, so a canvas view
 * costs a few bytes per call instead of a bitmap. The recording is replaced on the next {@code
 * onDraw}, which runs after {@link picodroid.view.View#invalidate()}. A {@code Canvas} draws only
 * during {@code onDraw}; outside it, and for a canvas made with {@link #Canvas()}, every call is a
 * no-op.
 *
 * <p>Divergences: no {@code Bitmap}, {@code Path}, {@code Rect}/{@code RectF} overloads, {@code
 * save}/{@code restore} or transforms; ovals and arcs are circular (the smaller side of the
 * bounds); a filled arc is always a wedge from the centre, and a stroked arc never draws the radii;
 * one view's recording holds about a hundred calls, and calls past that are dropped.
 */
public class Canvas {
  // Written by View while onDraw runs (graphics/fields.rs::canvas): the view's widget and size.
  // Keep this order.
  int nativeHandle;
  int width;
  int height;

  /** A canvas with nothing to draw on: every draw call is ignored. */
  public Canvas() {}

  /** The width of the view being drawn, in pixels; 0 outside {@code onDraw}. */
  public int getWidth() {
    return width;
  }

  /** The height of the view being drawn, in pixels; 0 outside {@code onDraw}. */
  public int getHeight() {
    return height;
  }

  /** Fills the whole view with {@code color}, {@code 0xAARRGGBB}. */
  public void drawColor(int color) {
    nativeDrawColor(color);
  }

  public void drawRect(float left, float top, float right, float bottom, Paint paint) {
    nativeDrawRect(left, top, right, bottom, 0f, paint);
  }

  /**
   * A rectangle with rounded corners. The corners are circular: the smaller of {@code rx} and
   * {@code ry} is the radius.
   */
  public void drawRoundRect(
      float left, float top, float right, float bottom, float rx, float ry, Paint paint) {
    nativeDrawRect(left, top, right, bottom, rx < ry ? rx : ry, paint);
  }

  public void drawCircle(float cx, float cy, float radius, Paint paint) {
    nativeDrawRect(cx - radius, cy - radius, cx + radius, cy + radius, radius, paint);
  }

  /** A straight line in the paint's colour, stroke width and cap, whatever its style. */
  public void drawLine(float startX, float startY, float stopX, float stopY, Paint paint) {
    nativeDrawLine(startX, startY, stopX, stopY, paint);
  }

  /**
   * An arc of the circle inscribed in the bounds, from {@code startAngle} through {@code
   * sweepAngle} degrees, clockwise from 3 o'clock. Filled, it is a wedge from the centre; stroked,
   * it is the arc alone, with round ends when the paint's cap is {@link Paint.Cap#ROUND}.
   */
  public void drawArc(
      float left,
      float top,
      float right,
      float bottom,
      float startAngle,
      float sweepAngle,
      boolean useCenter,
      Paint paint) {
    nativeDrawArc(left, top, right, bottom, startAngle, sweepAngle, paint);
  }

  /**
   * One line of text whose baseline is at {@code y}; {@code x} is its left edge, centre or right
   * edge according to the paint's {@link Paint#setTextAlign text align}.
   */
  public void drawText(String text, float x, float y, Paint paint) {
    nativeDrawText(text, x, y, paint);
  }

  private native void nativeDrawColor(int color);

  private native void nativeDrawRect(
      float left, float top, float right, float bottom, float radius, Paint paint);

  private native void nativeDrawLine(
      float startX, float startY, float stopX, float stopY, Paint paint);

  private native void nativeDrawArc(
      float left,
      float top,
      float right,
      float bottom,
      float startAngle,
      float sweepAngle,
      Paint paint);

  private native void nativeDrawText(String text, float x, float y, Paint paint);
}
