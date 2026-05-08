// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

/**
 * Common base for {@link Adapter} implementations. Mirrors {@code android.widget.BaseAdapter} —
 * subclasses override {@link #getCount} and {@link #getItem(int)}; {@link #getItemId(int)} defaults
 * to the position. {@link #notifyDataSetChanged()} re-pushes the dataset to the bound {@link
 * AdapterView}.
 */
public abstract class BaseAdapter implements Adapter {
  private AdapterView<? extends Adapter> boundView;

  @Override
  public long getItemId(int position) {
    return position;
  }

  /** Called by AdapterView when this adapter is set, so notifyDataSetChanged can flush updates. */
  void bindView(AdapterView<? extends Adapter> view) {
    this.boundView = view;
  }

  /** Re-push the dataset to the bound widget. */
  public void notifyDataSetChanged() {
    if (boundView != null) {
      boundView.refreshFromAdapter();
    }
  }
}
