// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.AdapterView}. Common parent for adapter-backed widgets such as
 * {@link Spinner} and {@link ListView}. Holds a reference to the bound {@link Adapter} and exposes
 * {@link #refreshFromAdapter()} so subclasses re-render when {@code
 * BaseAdapter.notifyDataSetChanged()} fires.
 */
public abstract class AdapterView<T extends Adapter> extends ViewGroup {
  protected T adapter;

  protected AdapterView(int nativeHandle) {
    super(nativeHandle);
  }

  public void setAdapter(T adapter) {
    this.adapter = adapter;
    if (adapter instanceof BaseAdapter) {
      ((BaseAdapter) adapter).bindView(this);
    }
    refreshFromAdapter();
  }

  public T getAdapter() {
    return adapter;
  }

  /**
   * Re-read the dataset from the bound adapter and push it to the underlying widget. Called by
   * {@link #setAdapter} and by {@link BaseAdapter#notifyDataSetChanged()}; subclasses override to
   * flush items into LVGL.
   */
  protected abstract void refreshFromAdapter();
}
