// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.AdapterView}. Common parent for adapter-backed widgets such as
 * {@link Spinner} and {@link ListView}. Holds a reference to the bound {@link Adapter} and exposes
 * {@link #refreshFromAdapter()} so subclasses re-render when {@code
 * BaseAdapter.notifyDataSetChanged()} fires.
 */
public abstract class AdapterView<T extends Adapter> extends ViewGroup {
  protected T adapter;

  /** Package-private so adapter-backed subclasses ({@link ListView}) can fire it. */
  OnItemClickListener onItemClickListener;

  protected AdapterView(int nativeHandle) {
    super(nativeHandle);
  }

  /**
   * Callback invoked when an item in this AdapterView is clicked. Mirrors {@code
   * android.widget.AdapterView.OnItemClickListener} exactly, including the full four-argument
   * signature. Note: {@code view} is the row View on Android; picodroid renders rows natively and
   * passes {@code null} for it — {@code parent}, {@code position}, and {@code id} are faithful.
   */
  public interface OnItemClickListener {
    void onItemClick(AdapterView<?> parent, View view, int position, long id);
  }

  /**
   * Register a listener for item clicks. Mirrors {@code
   * android.widget.AdapterView#setOnItemClickListener}. Subclasses that support D-pad/tap item
   * activation override {@link #registerNativeItemClick()} to wire the underlying widget.
   */
  public void setOnItemClickListener(OnItemClickListener listener) {
    this.onItemClickListener = listener;
    registerNativeItemClick();
  }

  /** Returns the registered item-click listener, or {@code null}. */
  public OnItemClickListener getOnItemClickListener() {
    return onItemClickListener;
  }

  /**
   * Wire the underlying native widget to deliver item-click events. Default no-op (e.g. {@link
   * Spinner} reports selection separately); {@link ListView} overrides it.
   */
  protected void registerNativeItemClick() {}

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
