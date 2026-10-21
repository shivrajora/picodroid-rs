// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.view.ViewParent;

/**
 * Mirrors {@code android.widget.AdapterView}. Common parent for adapter-backed widgets such as
 * {@link Spinner} and {@link ListView}. Holds a reference to the bound {@link Adapter} and exposes
 * {@link #refreshFromAdapter()} so subclasses re-render when {@code
 * BaseAdapter.notifyDataSetChanged()} fires.
 */
public abstract class AdapterView<T extends Adapter> extends ViewGroup {
  /** Mirrors {@code android.widget.AdapterView#INVALID_POSITION}: no row. */
  public static final int INVALID_POSITION = -1;

  protected T adapter;

  /** Package-private so adapter-backed subclasses ({@link ListView}) can fire it. */
  OnItemClickListener onItemClickListener;

  /** Package-private so adapter-backed subclasses ({@link Spinner}) can fire it. */
  OnItemSelectedListener onItemSelectedListener;

  protected AdapterView(int nativeHandle) {
    super(nativeHandle);
  }

  protected AdapterView(Context ctx, int nativeHandle) {
    super(ctx, nativeHandle);
  }

  /**
   * Callback invoked when an item in this AdapterView is clicked. Mirrors {@code
   * android.widget.AdapterView.OnItemClickListener} exactly, including the full four-argument
   * signature. {@code view} is the row the adapter's {@code getView} built ({@link ListView}), or
   * {@code null} for a widget that renders rows natively ({@link Spinner}, {@code ListView.addItem}
   * rows); {@code parent}, {@code position}, and {@code id} are faithful.
   */
  public interface OnItemClickListener {
    void onItemClick(AdapterView<?> parent, View view, int position, long id);
  }

  /**
   * Callback invoked when an item's selection state changes. Mirrors {@code
   * android.widget.AdapterView.OnItemSelectedListener} exactly, including the full four-argument
   * signature and {@link #onNothingSelected}. Note: {@code view} is the row View on Android;
   * picodroid renders rows natively and passes {@code null} for it — {@code parent}, {@code
   * position}, and {@code id} are faithful. {@code onNothingSelected} is declared for source
   * compatibility but never fires in picodroid: the underlying roller always has a selection.
   */
  public interface OnItemSelectedListener {
    void onItemSelected(AdapterView<?> parent, View view, int position, long id);

    void onNothingSelected(AdapterView<?> parent);
  }

  /**
   * Register a listener for item clicks. Mirrors {@code
   * android.widget.AdapterView#setOnItemClickListener}. Subclasses that support D-pad/tap item
   * activation override {@link #registerNativeItemClick()} to wire the underlying widget. The
   * lifetime rule is {@link View#setOnClickListener}'s: the listener lives with this view.
   */
  public void setOnItemClickListener(OnItemClickListener listener) {
    this.onItemClickListener = listener;
    registerNativeItemClick();
  }

  /**
   * Register a listener for selection changes. Mirrors {@code
   * android.widget.AdapterView#setOnItemSelectedListener}. Subclasses that report selection
   * override {@link #registerNativeItemSelected()} to wire the underlying widget.
   */
  public void setOnItemSelectedListener(OnItemSelectedListener listener) {
    this.onItemSelectedListener = listener;
    registerNativeItemSelected();
  }

  /** Returns the registered item-selected listener, or {@code null}. */
  public OnItemSelectedListener getOnItemSelectedListener() {
    return onItemSelectedListener;
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

  /**
   * Wire the underlying native widget to deliver selection-change events. Default no-op; {@link
   * Spinner} overrides it.
   */
  protected void registerNativeItemSelected() {}

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

  /** Mirrors {@code android.widget.AdapterView#getCount()}: the bound adapter's count, or 0. */
  public int getCount() {
    return adapter == null ? 0 : adapter.getCount();
  }

  /**
   * Mirrors {@code android.widget.AdapterView#getFirstVisiblePosition()}: the adapter position of
   * the first row on screen. A widget that shows every item at once reports 0; {@link ListView}
   * reads its scroll offset.
   */
  public int getFirstVisiblePosition() {
    return 0;
  }

  /**
   * Mirrors {@code android.widget.AdapterView#getLastVisiblePosition()}: the adapter position of
   * the last row on screen, or {@link #INVALID_POSITION} when the adapter is empty.
   */
  public int getLastVisiblePosition() {
    return getCount() - 1;
  }

  /**
   * Mirrors {@code android.widget.AdapterView#getPositionForView}: the adapter position of the row
   * that is {@code view} or contains it, or {@link #INVALID_POSITION}. Here a child index is a
   * position; {@link ListView} adds the position of its first live row.
   */
  public int getPositionForView(View view) {
    View row = view;
    while (row != null && row.getParent() != this) {
      ViewParent up = row.getParent();
      row = up instanceof View ? (View) up : null;
    }
    if (row == null) {
      return INVALID_POSITION;
    }
    int index = indexOfChild(row);
    return index < 0 ? INVALID_POSITION : index;
  }

  /**
   * Re-read the dataset from the bound adapter and push it to the underlying widget. Called by
   * {@link #setAdapter} and by {@link BaseAdapter#notifyDataSetChanged()}; subclasses override to
   * flush items into LVGL.
   */
  protected abstract void refreshFromAdapter();
}
