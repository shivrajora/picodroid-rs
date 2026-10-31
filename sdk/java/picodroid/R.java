// SPDX-License-Identifier: GPL-3.0-only
package picodroid;

/**
 * Mirrors {@code android.R}: the framework's own resource ids, with Android's values, so an adapter
 * reads as it does on Android: {@code new ArrayAdapter<>(this,
 * picodroid.R.layout.simple_list_item_1, items)}. picodroid draws every one of these rows the same
 * way, a single-line label. {@link attr} carries the style attributes a widget constructor takes:
 * {@code new ProgressBar(this, null, picodroid.R.attr.progressBarStyleHorizontal)}.
 *
 * <p>Compile-only: javac copies each constant into the class that names it, so this class is never
 * loaded and never embedded ({@code COMPILE_ONLY_CLASSES} in {@code build_support/papk.rs}).
 */
public final class R {
  private R() {}

  /** Framework view and item ids. */
  public static final class id {
    private id() {}

    /**
     * The up button of an action bar, as the item {@code Activity.onOptionsItemSelected} receives
     * for it.
     */
    public static final int home = 0x0102002c;
  }

  /** Framework row layouts. */
  public static final class layout {
    private layout() {}

    /** A list row: one line of text. */
    public static final int simple_list_item_1 = 0x01090003;

    /** A {@code Spinner}'s closed face. */
    public static final int simple_spinner_item = 0x01090008;

    /** A {@code Spinner}'s drop-down row. */
    public static final int simple_spinner_dropdown_item = 0x01090009;
  }

  /**
   * Framework style attributes, for a widget's {@code (Context, AttributeSet, int)} constructor.
   */
  public static final class attr {
    private attr() {}

    /** {@code ProgressBar}'s default style: the indeterminate spinner. */
    public static final int progressBarStyle = 0x01010077;

    /** The determinate horizontal {@code ProgressBar}. */
    public static final int progressBarStyleHorizontal = 0x01010078;

    /** A small spinner: the spinner here, at the size its layout gives. */
    public static final int progressBarStyleSmall = 0x01010079;

    /** A large spinner: the spinner here, at the size its layout gives. */
    public static final int progressBarStyleLarge = 0x0101007a;
  }
}
