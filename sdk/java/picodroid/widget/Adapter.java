// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

/**
 * Mirrors {@code android.widget.Adapter}. Backs {@link AdapterView} subclasses such as {@link
 * Spinner} and {@link ListView} so the data source is decoupled from the widget. Picodroid's v1
 * implementation only consumes {@link #getCount()} and {@link #getItem(int)} (rendering each item
 * via its {@code toString()}); convertView recycling is reserved for a follow-up milestone.
 */
public interface Adapter {
  int getCount();

  Object getItem(int position);

  long getItemId(int position);
}
