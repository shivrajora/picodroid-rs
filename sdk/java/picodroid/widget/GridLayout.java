// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * A grid of cells, the shape of {@code android.widget.GridLayout}: children are placed row by row
 * (or column by column under {@link #VERTICAL}) into the next free cell unless their {@link
 * LayoutParams} name one, a child may span several columns or rows, and a track with a weighted
 * child shares the free space in proportion to the weight. Backed by LVGL's grid layout.
 *
 * <p>A child's width and height are its cell's when it says {@code match_parent} on that axis (or
 * its alignment is {@link #FILL}); otherwise the child keeps its own size and sits in the cell
 * where its alignment says. Margins keep that much of the cell clear, as on Android.
 *
 * <p>Divergences from Android: a child's cell is fixed when it is added ({@code removeView} does
 * not free it and {@code addView(child, index)} does not reflow the rest); {@link #setColumnCount},
 * {@link #setRowCount} and {@link #setOrientation} throw once a child has been added; a weight
 * keeps one decimal and caps at 9.9; at most 16 columns, 16 rows and 6 grids alive at once; {@link
 * #BASELINE} reads as {@link #TOP}. Space between cells is the children's margins, as on Android.
 */
public class GridLayout extends ViewGroup {
  public static final int HORIZONTAL = 0;
  public static final int VERTICAL = 1;

  /** The value a {@link Spec}'s start takes when the grid chooses the cell. */
  public static final int UNDEFINED = Integer.MIN_VALUE;

  static final int ALIGN_START = 0;
  static final int ALIGN_CENTER = 1;
  static final int ALIGN_END = 2;
  static final int ALIGN_FILL = 3;
  static final int ALIGN_UNDEFINED = -1;

  public static final Alignment TOP = new Alignment(ALIGN_START);
  public static final Alignment LEFT = new Alignment(ALIGN_START);
  public static final Alignment START = LEFT;

  /** No baseline alignment on picodroid: reads as {@link #TOP}. */
  public static final Alignment BASELINE = TOP;

  public static final Alignment CENTER = new Alignment(ALIGN_CENTER);
  public static final Alignment BOTTOM = new Alignment(ALIGN_END);
  public static final Alignment RIGHT = new Alignment(ALIGN_END);
  public static final Alignment END = RIGHT;
  public static final Alignment FILL = new Alignment(ALIGN_FILL);
  static final Alignment UNDEFINED_ALIGNMENT = new Alignment(ALIGN_UNDEFINED);

  private int mOrientation = HORIZONTAL;
  private int mColumnCount = UNDEFINED;
  private int mRowCount = UNDEFINED;

  /** Android's placement cursor ({@code validateLayoutParams}), kept across adds. */
  private int mMajor;

  private int mMinor;

  /** Per minor track, the first major index past the cells taken so far; null until a child. */
  private int[] mMaxSizes;

  private int mColumnsUsed;
  private int mRowsUsed;

  public GridLayout(Context ctx) {
    super(ctx, nativeCreate());
  }

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected GridLayout(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  public int getOrientation() {
    return mOrientation;
  }

  /**
   * {@link #HORIZONTAL} (the default) fills a row before starting the next; {@link #VERTICAL} fills
   * a column. Must be called before the first child is added.
   */
  public void setOrientation(int orientation) {
    checkNoChildren("setOrientation");
    mOrientation = orientation == VERTICAL ? VERTICAL : HORIZONTAL;
  }

  /** The declared column count, or the columns in use when none was declared. */
  public int getColumnCount() {
    return mColumnCount != UNDEFINED ? mColumnCount : mColumnsUsed;
  }

  /** How many columns a row has before placement wraps. Must be called before the first child. */
  public void setColumnCount(int columnCount) {
    checkNoChildren("setColumnCount");
    if (columnCount < 1) {
      throw new IllegalArgumentException("GridLayout: columnCount must be at least 1");
    }
    mColumnCount = columnCount;
  }

  /** The declared row count, or the rows in use when none was declared. */
  public int getRowCount() {
    return mRowCount != UNDEFINED ? mRowCount : mRowsUsed;
  }

  /** How many rows a column has before placement wraps. Must be called before the first child. */
  public void setRowCount(int rowCount) {
    checkNoChildren("setRowCount");
    if (rowCount < 1) {
      throw new IllegalArgumentException("GridLayout: rowCount must be at least 1");
    }
    mRowCount = rowCount;
  }

  private void checkNoChildren(String what) {
    if (mMaxSizes != null) {
      throw new IllegalStateException(
          "GridLayout." + what + ": call it before the first child is added");
    }
  }

  @Override
  public void addView(View child) {
    ViewGroup.LayoutParams params = child == null ? null : child.getLayoutParams();
    addView(child, params != null ? params : new LayoutParams());
  }

  /**
   * Adds {@code child} in the cell its {@link LayoutParams} name, or the next free one. Params of
   * another type are converted (a {@code MarginLayoutParams} keeps its margins); {@code
   * match_parent} on an axis becomes {@link #FILL} on that axis.
   */
  @Override
  public void addView(View child, ViewGroup.LayoutParams params) {
    LayoutParams lp = params instanceof LayoutParams ? (LayoutParams) params : convert(params);
    place(lp);
    int width = lp.width;
    int height = lp.height;
    int colAlign = alignCode(lp.columnSpec, width);
    int rowAlign = alignCode(lp.rowSpec, height);
    // The cell does the stretching (LVGL's STRETCH): LVGL must never see a percentage width,
    // which it would take of the whole grid, not of the cell.
    if (width == LayoutParams.MATCH_PARENT) {
      lp.width = LayoutParams.WRAP_CONTENT;
    }
    if (height == LayoutParams.MATCH_PARENT) {
      lp.height = LayoutParams.WRAP_CONTENT;
    }
    super.addView(child, lp);
    lp.width = width;
    lp.height = height;
    Spec col = lp.columnSpec;
    Spec row = lp.rowSpec;
    if (!nativePlace(
        child,
        colAlign,
        col.start,
        col.size,
        fr(col.weight),
        rowAlign,
        row.start,
        row.size,
        fr(row.weight))) {
      throw new IllegalStateException(
          "GridLayout: a cell past column or row 16, or a seventh grid; split the layout");
    }
  }

  private native boolean nativePlace(
      View child,
      int colAlign,
      int col,
      int colSpan,
      int colFr,
      int rowAlign,
      int row,
      int rowSpan,
      int rowFr);

  private static LayoutParams convert(ViewGroup.LayoutParams params) {
    if (params instanceof MarginLayoutParams) {
      return new LayoutParams((MarginLayoutParams) params);
    }
    return params != null ? new LayoutParams(params) : new LayoutParams();
  }

  /** Alignment code for the native: the spec's, else FILL for match_parent, else START. */
  private static int alignCode(Spec spec, int size) {
    if (spec.alignment.code != ALIGN_UNDEFINED) {
      return spec.alignment.code;
    }
    return size == LayoutParams.MATCH_PARENT ? ALIGN_FILL : ALIGN_START;
  }

  /** A weight as tenths, 0 for none; the native clamps to 1..99. */
  private static int fr(float weight) {
    return weight <= 0f ? 0 : Math.max(1, Math.round(weight * 10f));
  }

  /**
   * Android's {@code validateLayoutParams}, one child at a time: the cursor and the per-track
   * high-water marks carry over, so the result is the same as a pass over every child.
   */
  private void place(LayoutParams lp) {
    boolean horizontal = mOrientation == HORIZONTAL;
    Spec majorSpec = horizontal ? lp.rowSpec : lp.columnSpec;
    Spec minorSpec = horizontal ? lp.columnSpec : lp.rowSpec;
    int count = horizontal ? mColumnCount : mRowCount;
    if (count == UNDEFINED) {
      count = 0;
    }
    if (mMaxSizes == null) {
      mMaxSizes = new int[count];
    }
    int majorSize = majorSpec.size;
    int minorSize = minorSpec.size;
    if (count != 0 && minorSize > count) {
      throw new IllegalArgumentException(
          "GridLayout: a span of " + minorSize + " does not fit " + count + " tracks");
    }
    if (majorSpec.startDefined) {
      mMajor = majorSpec.start;
    }
    if (minorSpec.startDefined) {
      mMinor = minorSpec.start;
    }
    if (count != 0) {
      if (!majorSpec.startDefined || !minorSpec.startDefined) {
        while (!fits(mMajor, mMinor, mMinor + minorSize)) {
          if (minorSpec.startDefined) {
            mMajor++;
          } else if (mMinor + minorSize <= count) {
            mMinor++;
          } else {
            mMinor = 0;
            mMajor++;
          }
        }
      }
      fill(mMinor, mMinor + minorSize, mMajor + majorSize);
    }
    int major = mMajor;
    int minor = mMinor;
    if (horizontal) {
      lp.rowSpec = majorSpec.at(major);
      lp.columnSpec = minorSpec.at(minor);
    } else {
      lp.columnSpec = majorSpec.at(major);
      lp.rowSpec = minorSpec.at(minor);
    }
    mColumnsUsed = Math.max(mColumnsUsed, lp.columnSpec.start + lp.columnSpec.size);
    mRowsUsed = Math.max(mRowsUsed, lp.rowSpec.start + lp.rowSpec.size);
  }

  private boolean fits(int major, int minorStart, int minorEnd) {
    if (minorEnd > mMaxSizes.length) {
      return false;
    }
    for (int i = minorStart; i < minorEnd; i++) {
      if (mMaxSizes[i] > major) {
        return false;
      }
    }
    return true;
  }

  private void fill(int minorStart, int minorEnd, int majorEnd) {
    int end = Math.min(minorEnd, mMaxSizes.length);
    for (int i = minorStart; i < end; i++) {
      mMaxSizes[i] = majorEnd;
    }
  }

  public static Spec spec(int start) {
    return spec(start, 1, UNDEFINED_ALIGNMENT, 0f);
  }

  public static Spec spec(int start, int size) {
    return spec(start, size, UNDEFINED_ALIGNMENT, 0f);
  }

  public static Spec spec(int start, Alignment alignment) {
    return spec(start, 1, alignment, 0f);
  }

  public static Spec spec(int start, int size, Alignment alignment) {
    return spec(start, size, alignment, 0f);
  }

  public static Spec spec(int start, float weight) {
    return spec(start, 1, UNDEFINED_ALIGNMENT, weight);
  }

  public static Spec spec(int start, int size, float weight) {
    return spec(start, size, UNDEFINED_ALIGNMENT, weight);
  }

  public static Spec spec(int start, Alignment alignment, float weight) {
    return spec(start, 1, alignment, weight);
  }

  /**
   * A row or column spec: the track it starts in ({@link #UNDEFINED} to take the next free one),
   * how many tracks it spans, how the child sits in the cell on that axis, and the share of free
   * space the track gets.
   */
  public static Spec spec(int start, int size, Alignment alignment, float weight) {
    if (size < 1) {
      throw new IllegalArgumentException("GridLayout.spec: size must be at least 1");
    }
    return new Spec(
        start != UNDEFINED,
        start != UNDEFINED ? start : 0,
        size,
        alignment != null ? alignment : UNDEFINED_ALIGNMENT,
        weight);
  }

  /**
   * Where a child sits in its cell on one axis: {@link #TOP}, {@link #BOTTOM}, {@link #LEFT},
   * {@link #RIGHT}, {@link #START}, {@link #END}, {@link #CENTER} or {@link #FILL}.
   */
  public static final class Alignment {
    final int code;

    Alignment(int code) {
      this.code = code;
    }
  }

  /** One axis of a child's placement; see {@link GridLayout#spec(int, int, Alignment, float)}. */
  public static class Spec {
    static final Spec UNDEFINED = new Spec(false, 0, 1, UNDEFINED_ALIGNMENT, 0f);

    final boolean startDefined;
    final int start;
    final int size;
    final Alignment alignment;
    final float weight;

    Spec(boolean startDefined, int start, int size, Alignment alignment, float weight) {
      this.startDefined = startDefined;
      this.start = start;
      this.size = size;
      this.alignment = alignment;
      this.weight = weight;
    }

    /** This spec with its start resolved to {@code start}. */
    Spec at(int start) {
      return startDefined && this.start == start
          ? this
          : new Spec(true, start, size, alignment, weight);
    }

    /** The first track of the span; {@link GridLayout#UNDEFINED} until the child is placed. */
    public int getStart() {
      return startDefined ? start : GridLayout.UNDEFINED;
    }

    public int getSize() {
      return size;
    }

    public float getWeight() {
      return weight;
    }
  }

  /**
   * Mirrors {@code android.widget.GridLayout.LayoutParams}: a {@link Spec} per axis and the
   * margins. The default is {@code wrap_content} both ways in the next free cell.
   */
  public static class LayoutParams extends ViewGroup.MarginLayoutParams {
    public Spec rowSpec;
    public Spec columnSpec;

    public LayoutParams() {
      this(Spec.UNDEFINED, Spec.UNDEFINED);
    }

    public LayoutParams(Spec rowSpec, Spec columnSpec) {
      super(WRAP_CONTENT, WRAP_CONTENT);
      this.rowSpec = rowSpec != null ? rowSpec : Spec.UNDEFINED;
      this.columnSpec = columnSpec != null ? columnSpec : Spec.UNDEFINED;
    }

    public LayoutParams(ViewGroup.LayoutParams params) {
      super(params);
      this.rowSpec = Spec.UNDEFINED;
      this.columnSpec = Spec.UNDEFINED;
    }

    public LayoutParams(MarginLayoutParams params) {
      super(params);
      this.rowSpec = Spec.UNDEFINED;
      this.columnSpec = Spec.UNDEFINED;
    }

    public LayoutParams(LayoutParams source) {
      super(source);
      this.rowSpec = source.rowSpec;
      this.columnSpec = source.columnSpec;
    }

    /**
     * Sets both specs' alignment from a {@link Gravity} bitmask, as {@code android:layout_gravity}
     * on a grid child does: {@code fill} stretches, {@code center} centres, an edge pulls to it. An
     * axis the gravity does not name keeps the spec's own alignment.
     */
    public void setGravity(int gravity) {
      Alignment column = alignment(gravity & Gravity.HORIZONTAL_GRAVITY_MASK);
      Alignment row = alignment((gravity & Gravity.VERTICAL_GRAVITY_MASK) >> 4);
      if (column != UNDEFINED_ALIGNMENT) {
        columnSpec =
            new Spec(
                columnSpec.startDefined,
                columnSpec.start,
                columnSpec.size,
                column,
                columnSpec.weight);
      }
      if (row != UNDEFINED_ALIGNMENT) {
        rowSpec = new Spec(rowSpec.startDefined, rowSpec.start, rowSpec.size, row, rowSpec.weight);
      }
    }

    /** One axis's three gravity bits (specified, pull-before, pull-after) as an alignment. */
    private static Alignment alignment(int bits) {
      switch (bits) {
        case 3: // AXIS_SPECIFIED | AXIS_PULL_BEFORE: LEFT / TOP
          return LEFT;
        case 5: // AXIS_SPECIFIED | AXIS_PULL_AFTER: RIGHT / BOTTOM
          return RIGHT;
        case 7: // both pulls: FILL
          return FILL;
        case 1: // AXIS_SPECIFIED alone: CENTER
          return CENTER;
        default:
          return UNDEFINED_ALIGNMENT;
      }
    }
  }
}
