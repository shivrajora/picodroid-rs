// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.Adapter}. Backs {@link AdapterView} subclasses such as {@link
 * Spinner} and {@link ListView} so the data source is decoupled from the widget. {@link ListView}
 * asks {@link #getView} for every row and hands the row it already has back as {@code convertView}
 * on the next {@code notifyDataSetChanged()}, so an adapter that reuses it re-binds in place;
 * {@link Spinner} renders each item's {@code toString()} and never calls {@code getView}.
 */
public interface Adapter {
  int getCount();

  Object getItem(int position);

  long getItemId(int position);

  /**
   * The row for {@code position}. {@code convertView} is the view this adapter returned for the
   * same position last time, or {@code null} the first time and after the list was rebuilt; return
   * it re-bound to the current item, or a new view, as on Android. Returning a view other than
   * {@code convertView} frees {@code convertView}. {@code parent} is the {@link ListView}; do not
   * add the row to it — the list does.
   */
  View getView(int position, View convertView, ViewGroup parent);
}
