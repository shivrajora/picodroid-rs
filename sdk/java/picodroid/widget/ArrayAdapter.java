// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.ArrayAdapter}. Holds a list of items and renders each as a {@link
 * TextView} showing its {@code toString()}, or — with Android's layout-resource constructors — as
 * an inflated {@code R.layout.*} row whose {@code TextView} (the whole row, or the child named by
 * {@code textViewResourceId}) shows the text. Construct from a {@code T[]}, a {@code List}, or fill
 * it with {@link #add}; pass to {@link AdapterView#setAdapter}.
 *
 * <p>{@link #getView} re-binds {@code convertView} when the list offers one back, so a {@code
 * notifyDataSetChanged()} updates rows in place instead of rebuilding them.
 */
public class ArrayAdapter<T> extends BaseAdapter {
  private final java.util.ArrayList<T> items = new java.util.ArrayList<T>();
  private final Context mContext;
  private final int mResource;
  private final int mFieldId;

  public ArrayAdapter(Context ctx, T[] items) {
    this(ctx, 0, 0);
    addAll(items);
  }

  public ArrayAdapter(T[] items) {
    this(null, 0, 0);
    addAll(items);
  }

  public ArrayAdapter(Context ctx) {
    this(ctx, 0, 0);
  }

  public ArrayAdapter() {
    this(null, 0, 0);
  }

  /**
   * Android's {@code (Context, int resource)}: rows inflate {@code resource}, a {@code TextView}.
   */
  public ArrayAdapter(Context ctx, int resource) {
    this(ctx, resource, 0);
  }

  /**
   * Android's {@code (Context, int resource, int textViewResourceId)}: rows inflate {@code
   * resource} and the text goes into its child {@code textViewResourceId}.
   */
  public ArrayAdapter(Context ctx, int resource, int textViewResourceId) {
    this.mContext = ctx;
    this.mResource = resource;
    this.mFieldId = textViewResourceId;
  }

  public ArrayAdapter(Context ctx, int resource, T[] objects) {
    this(ctx, resource, 0);
    addAll(objects);
  }

  public ArrayAdapter(Context ctx, int resource, int textViewResourceId, T[] objects) {
    this(ctx, resource, textViewResourceId);
    addAll(objects);
  }

  public ArrayAdapter(Context ctx, int resource, java.util.List<T> objects) {
    this(ctx, resource, 0);
    addAll(objects);
  }

  public ArrayAdapter(
      Context ctx, int resource, int textViewResourceId, java.util.List<T> objects) {
    this(ctx, resource, textViewResourceId);
    addAll(objects);
  }

  public Context getContext() {
    return mContext;
  }

  public void add(T item) {
    items.add(item);
  }

  public void addAll(T[] objects) {
    if (objects != null) {
      for (T item : objects) {
        items.add(item);
      }
    }
  }

  public void addAll(java.util.List<T> objects) {
    if (objects != null) {
      int n = objects.size();
      for (int i = 0; i < n; i++) {
        items.add(objects.get(i));
      }
    }
  }

  public void insert(T item, int index) {
    items.add(index, item);
  }

  public void remove(T item) {
    int i = getPosition(item);
    if (i >= 0) {
      items.remove(i);
    }
  }

  public void clear() {
    items.clear();
  }

  /** The index of {@code item} by {@code equals}, or -1. */
  public int getPosition(T item) {
    int n = items.size();
    for (int i = 0; i < n; i++) {
      T it = items.get(i);
      if (it == item || (it != null && it.equals(item))) {
        return i;
      }
    }
    return -1;
  }

  @Override
  public int getCount() {
    return items.size();
  }

  @Override
  public T getItem(int position) {
    return items.get(position);
  }

  @Override
  public View getView(int position, View convertView, ViewGroup parent) {
    View row = convertView;
    if (row == null) {
      row = mResource == 0 ? newTextView() : inflateRow(parent);
    }
    TextView text;
    if (mFieldId == 0) {
      text = (TextView) row;
    } else {
      text = row.findViewById(mFieldId);
      if (text == null) {
        throw new IllegalStateException(
            "ArrayAdapter: the row layout has no TextView with that id");
      }
    }
    T item = getItem(position);
    text.setText(item == null ? "" : item.toString());
    return row;
  }

  private TextView newTextView() {
    return mContext == null ? new TextView() : new TextView(mContext);
  }

  private View inflateRow(ViewGroup parent) {
    if (mContext == null) {
      throw new IllegalStateException("ArrayAdapter: a layout resource needs a Context");
    }
    return LayoutInflater.from(mContext).inflate(mResource, parent, false);
  }
}
