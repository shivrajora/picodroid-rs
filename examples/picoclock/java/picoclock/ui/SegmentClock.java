// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picodroid.graphics.drawable.GradientDrawable;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/**
 * "HH:MM" as four seven-segment digits and a blinking colon, drawn out of plain rectangles.
 *
 * <p>The SDK has no text-size API — a {@code TextView} is whatever the bundled font is — so a clock
 * face big enough to read across a room has to be built rather than set. Thirty small views is also
 * the cheaper way to run one: a redraw is a handful of colour changes on the segments that actually
 * differed, where re-setting a label re-lays-out and re-rasterises the whole string every second.
 *
 * <p>The face is built whole, in the constructor, inside the screen's {@code onCreate}: the main
 * loop allows a screen build a slow first frame (docs/designs/ui-build-budget-2026-10.md).
 */
final class SegmentClock {
  /** Digit cell. Four of these plus a colon fill the panel's width with a margin to spare. */
  private static final int DIGIT_WIDTH = 62;

  private static final int DIGIT_HEIGHT = 104;

  /** Segment thickness. */
  private static final int STROKE = 11;

  /** Gap between the two digits of an hour or a minute. */
  private static final int DIGIT_GAP = 6;

  /** Width of the colon column, and the gap either side of it. */
  private static final int COLON_WIDTH = 14;

  private static final int COLON_GAP = 9;

  private static final int DOT = 11;

  /**
   * Which of the seven segments each digit lights, bit 0 = A (top) through bit 6 = G (middle), in
   * the order {@link #SEGMENT_BOXES} lays them out.
   */
  private static final int[] DIGIT_SEGMENTS = {
    0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F
  };

  /** Per-segment {x, y, w, h} within a digit cell: A, B, C, D, E, F, G. */
  private static final int[][] SEGMENT_BOXES = boxes();

  private static final int SEGMENTS_PER_DIGIT = 7;
  private static final int DIGITS = 4;

  /** The digit views, four digits of seven segments, then the colon's two dots. */
  private final FrameLayout[] segments = new FrameLayout[DIGITS * SEGMENTS_PER_DIGIT];

  private final FrameLayout[] colonDots = new FrameLayout[2];

  /** What each segment is currently painted, so a redraw only touches what changed. */
  private final boolean[] lit = new boolean[DIGITS * SEGMENTS_PER_DIGIT];

  private final FrameLayout face;

  /** Total width of the face, for centring it. */
  static int width() {
    return DIGITS * DIGIT_WIDTH + 2 * DIGIT_GAP + COLON_WIDTH + 2 * COLON_GAP;
  }

  static int height() {
    return DIGIT_HEIGHT;
  }

  /** Puts an unlit face into {@code parent} at {@code (x, y)}. */
  SegmentClock(ViewGroup parent, int x, int y) {
    face = Ui.group(x, y, width(), height());
    parent.addView(face);
    for (int i = 0; i < segments.length; i++) {
      addSegment(i);
    }
    for (int i = 0; i < colonDots.length; i++) {
      addColonDot(i);
    }
  }

  /** Paint {@code hour:minute}, with the colon shown or hidden. Cheap to call every second. */
  void show(int h, int m, boolean colon) {
    paintDigit(0, h / 10);
    paintDigit(1, h % 10);
    paintDigit(2, m / 10);
    paintDigit(3, m % 10);
    paintColon(colon);
  }

  // ── Construction ───────────────────────────────────────────────────────────

  private void addSegment(int i) {
    int digit = i / SEGMENTS_PER_DIGIT;
    int[] box = SEGMENT_BOXES[i % SEGMENTS_PER_DIGIT];
    FrameLayout seg = new FrameLayout();
    seg.setSize(box[2], box[3]);
    seg.setPosition(digitX(digit) + box[0], box[1]);
    seg.setPadding(0, 0, 0, 0);
    seg.setBackground(new GradientDrawable().setColor(Ui.SEGMENT_OFF).setCornerRadius(3));
    face.addView(seg);
    segments[i] = seg;
  }

  private void addColonDot(int i) {
    int x = 2 * DIGIT_WIDTH + DIGIT_GAP + COLON_GAP + (COLON_WIDTH - DOT) / 2;
    FrameLayout dot = new FrameLayout();
    dot.setSize(DOT, DOT);
    dot.setPosition(x, i == 0 ? DIGIT_HEIGHT / 3 - DOT / 2 : 2 * DIGIT_HEIGHT / 3 - DOT / 2);
    dot.setPadding(0, 0, 0, 0);
    dot.setBackground(new GradientDrawable().setColor(Ui.SEGMENT_OFF).setCornerRadius(DOT / 2));
    face.addView(dot);
    colonDots[i] = dot;
  }

  // ── Painting ───────────────────────────────────────────────────────────────

  private void paintDigit(int digit, int value) {
    int mask = DIGIT_SEGMENTS[value];
    int base = digit * SEGMENTS_PER_DIGIT;
    for (int s = 0; s < SEGMENTS_PER_DIGIT; s++) {
      paintSegment(base + s, (mask & (1 << s)) != 0);
    }
  }

  private void paintSegment(int i, boolean on) {
    if (segments[i] == null || lit[i] == on) {
      return;
    }
    lit[i] = on;
    segments[i].setBackground(
        new GradientDrawable().setColor(on ? Ui.SEGMENT_ON : Ui.SEGMENT_OFF).setCornerRadius(3));
  }

  private void paintColon(boolean on) {
    for (int i = 0; i < colonDots.length; i++) {
      if (colonDots[i] != null) {
        colonDots[i].setBackground(
            new GradientDrawable()
                .setColor(on ? Ui.SEGMENT_ON : Ui.SEGMENT_OFF)
                .setCornerRadius(DOT / 2));
      }
    }
  }

  /** The x of digit {@code i} within the face, the colon taking the place of the middle gap. */
  private static int digitX(int i) {
    int x = i * DIGIT_WIDTH;
    if (i >= 1) {
      x += DIGIT_GAP;
    }
    if (i >= 2) {
      x += COLON_WIDTH + 2 * COLON_GAP;
    }
    if (i >= 3) {
      x += DIGIT_GAP;
    }
    return x;
  }

  /**
   * The seven segment rectangles of a digit cell, in A-G order: A top, B upper right, C lower
   * right, D bottom, E lower left, F upper left, G middle. The two vertical runs each take half of
   * what the three horizontal strokes leave.
   */
  private static int[][] boxes() {
    int half = (DIGIT_HEIGHT - 3 * STROKE) / 2;
    int span = DIGIT_WIDTH - 2 * STROKE;
    return new int[][] {
      {STROKE, 0, span, STROKE}, // A
      {DIGIT_WIDTH - STROKE, STROKE, STROKE, half}, // B
      {DIGIT_WIDTH - STROKE, 2 * STROKE + half, STROKE, half}, // C
      {STROKE, 2 * STROKE + 2 * half, span, STROKE}, // D
      {0, 2 * STROKE + half, STROKE, half}, // E
      {0, STROKE, STROKE, half}, // F
      {STROKE, STROKE + half, span, STROKE}, // G
    };
  }

  /** Where the face should sit to be centred on the panel. */
  static int centredX() {
    return (Ui.WIDTH - width()) / 2;
  }
}
