// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;

/**
 * Mirrors {@code android.widget.ArrayAdapter}. Holds an array of items and renders each via {@code
 * toString()}. Construct from a {@code String[]}, a varargs list, or a builder-style {@link
 * #add(Object)} flow; pass to {@link AdapterView#setAdapter}.
 */
public class ArrayAdapter<T> extends BaseAdapter {
  private final java.util.ArrayList<T> items;

  public ArrayAdapter(Context ctx, T[] items) {
    this.items = new java.util.ArrayList<T>();
    if (items != null) {
      for (T item : items) {
        this.items.add(item);
      }
    }
  }

  public ArrayAdapter(T[] items) {
    this(null, items);
  }

  public ArrayAdapter(Context ctx) {
    this(ctx, null);
  }

  public ArrayAdapter() {
    this(null, null);
  }

  public void add(T item) {
    items.add(item);
  }

  public void clear() {
    items.clear();
  }

  @Override
  public int getCount() {
    return items.size();
  }

  @Override
  public T getItem(int position) {
    return items.get(position);
  }
}
