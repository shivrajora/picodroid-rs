// SPDX-License-Identifier: GPL-3.0-only
package picodroid.util;

/**
 * The attributes of the layout element a view is being inflated from. Mirrors {@code
 * android.util.AttributeSet} in name, so a custom view keeps Android's {@code (Context,
 * AttributeSet)} constructor and a {@code LayoutInflater.Factory} its signature.
 *
 * <p>A layout is compiled to numbers before it reaches the device, and the framework applies the
 * attributes it knows ({@code android:id}, {@code layout_*}, padding, background, …) to the view
 * after the factory returns it. Nothing is left to read here: the set is always empty, and an
 * attribute of a view's own ({@code app:…}) is refused at build time.
 */
public interface AttributeSet {
  /** The number of attributes that can be read from this set: always 0. */
  int getAttributeCount();
}
