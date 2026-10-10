// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.ListView}. Rows come from the bound {@link Adapter}'s {@link
 * Adapter#getView getView}: every position gets a row view, kept as a direct child in position
 * order, and {@code notifyDataSetChanged()} offers each existing row back as {@code convertView} so
 * the adapter re-binds it in place. The recycle pool is the row set itself — rows never scroll out
 * of existence here, so there is no off-screen scrap; a position past the new count frees its row,
 * and the list frees every row with itself.
 *
 * <p>Each row is made clickable and keypad-focusable and joins the Activity's focus group, so a tap
 * or ENTER on it fires {@link AdapterView.OnItemClickListener#onItemClick onItemClick} with the row
 * as {@code view}. Rows are stretched to the list's width, as Android's are.
 */
public class ListView extends AdapterView<Adapter> {
  /** The row views in position order; {@code null} entries past {@link #mRowCount}. */
  private View[] mRows;

  private int mRowCount;

  public ListView() {
    super(nativeCreate());
  }

  public ListView(Context ctx) {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  /**
   * Append a single native text row. Convenience kept for parity with the pre-adapter API; a list
   * with an adapter replaces these rows at its next refresh.
   */
  public native void addItem(String text);

  @Override
  protected void registerNativeItemClick() {
    nativeRegisterItemClickListener();
  }

  private native void nativeRegisterItemClickListener();

  /**
   * Invoked by the framework event loop when a row is activated (ENTER on the focused row, or a
   * touch tap). Resolves the row's stable {@code id} from the bound {@link Adapter} and delivers
   * the full Android {@code onItemClick(parent, view, position, id)} callback; {@code view} is the
   * row the adapter built, or {@code null} for an {@link #addItem} row.
   */
  void fireItemClick(int position) {
    if (onItemClickListener != null) {
      long id = adapter != null ? adapter.getItemId(position) : position;
      View row = position >= 0 && position < mRowCount ? mRows[position] : null;
      onItemClickListener.onItemClick(this, row, position, id);
    }
  }

  @Override
  public void removeAllViews() {
    super.removeAllViews();
    mRowCount = 0;
  }

  /**
   * Binds {@code adapter} and builds its rows from scratch: as on Android, a new adapter never sees
   * the old adapter's rows as {@code convertView} (its recycler is cleared), so every position gets
   * {@code getView(position, null, this)} and the previous rows are freed.
   */
  @Override
  public void setAdapter(Adapter adapter) {
    removeAllViews();
    super.setAdapter(adapter);
  }

  @Override
  protected void refreshFromAdapter() {
    if (getChildCount() != mRowCount) {
      // addItem rows, or children added behind the adapter's back: start from an empty list so
      // positions and child indices agree again (a click is resolved by child index).
      removeAllViews();
    }
    int count = adapter == null ? 0 : adapter.getCount();
    if (count > 0 && (mRows == null || mRows.length < count)) {
      View[] bigger = new View[Math.max(count, mRows == null ? 4 : mRows.length * 2)];
      if (mRows != null) {
        System.arraycopy(mRows, 0, bigger, 0, mRowCount);
      }
      mRows = bigger;
    }
    for (int i = 0; i < count; i++) {
      View convert = i < mRowCount ? mRows[i] : null;
      View row = adapter.getView(i, convert, this);
      if (row == null) {
        throw new IllegalStateException("getView returned null for position " + i);
      }
      if (row != convert) {
        if (convert != null) {
          // The adapter built a replacement: the old row is freed, as removeView frees.
          removeView(convert);
        }
        addView(row, i);
        nativeStyleRow(row);
      }
      mRows[i] = row;
    }
    for (int i = count; i < mRowCount; i++) {
      removeView(mRows[i]);
      mRows[i] = null;
    }
    mRowCount = count;
  }

  /**
   * Makes a freshly added row behave as a list row: clickable and keypad-focusable, a member of the
   * Activity's focus group, stretched to the list's width, padded, and highlighted when focused.
   */
  private native void nativeStyleRow(View row);
}
