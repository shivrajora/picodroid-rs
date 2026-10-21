// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.Adapter}. Backs {@link AdapterView} subclasses such as {@link
 * Spinner} and {@link ListView} so the data source is decoupled from the widget. {@link ListView}
 * asks {@link #getView} for the rows in view and, as the list scrolls, hands a row that left the
 * screen back as {@code convertView} for the position coming in, so an adapter that re-binds it
 * builds a dozen views for a thousand items; {@link Spinner} renders each item's {@code toString()}
 * and never calls {@code getView}.
 */
public interface Adapter {
  int getCount();

  Object getItem(int position);

  long getItemId(int position);

  /**
   * The row for {@code position}. {@code convertView} is a row this adapter built earlier, for
   * another position that scrolled away or for this one on a {@code notifyDataSetChanged()}, or
   * {@code null} when there is none to reuse; return it re-bound to the current item, or a new
   * view, as on Android. Returning a view other than {@code convertView} frees {@code convertView}.
   * {@code parent} is the {@link ListView}; do not add the row to it — the list does.
   */
  View getView(int position, View convertView, ViewGroup parent);
}
