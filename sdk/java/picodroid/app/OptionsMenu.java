// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.res.Resources;
import picodroid.view.Menu;
import picodroid.view.MenuItem;

/**
 * The {@link Menu} an Activity's options menu is built in (Android's {@code MenuBuilder}, cut to
 * what the list presentation needs). Items sit in display order: by {@link MenuItem#getOrder()},
 * then as added. A small fixed-growth array rather than a collection: a menu has a handful of
 * actions.
 */
final class OptionsMenu implements Menu {
  private Item[] items = new Item[4];
  private int count;

  @Override
  public MenuItem add(CharSequence title) {
    return add(NONE, NONE, NONE, title);
  }

  @Override
  public MenuItem add(int titleRes) {
    return add(NONE, NONE, NONE, Resources.getInstance().getString(titleRes));
  }

  @Override
  public MenuItem add(int groupId, int itemId, int order, int titleRes) {
    return add(groupId, itemId, order, Resources.getInstance().getString(titleRes));
  }

  @Override
  public MenuItem add(int groupId, int itemId, int order, CharSequence title) {
    Item item = new Item(groupId, itemId, order, title);
    if (count == items.length) {
      Item[] grown = new Item[items.length * 2];
      System.arraycopy(items, 0, grown, 0, count);
      items = grown;
    }
    // After every item of a lower or equal order: stable by insertion, as Android lists them.
    int at = count;
    while (at > 0 && items[at - 1].order > order) {
      at--;
    }
    System.arraycopy(items, at, items, at + 1, count - at);
    items[at] = item;
    count++;
    return item;
  }

  @Override
  public void removeItem(int id) {
    for (int i = 0; i < count; i++) {
      if (items[i].itemId == id) {
        System.arraycopy(items, i + 1, items, i, count - i - 1);
        items[--count] = null;
        return;
      }
    }
  }

  @Override
  public void clear() {
    for (int i = 0; i < count; i++) {
      items[i] = null;
    }
    count = 0;
  }

  @Override
  public MenuItem findItem(int id) {
    for (int i = 0; i < count; i++) {
      if (items[i].itemId == id) {
        return items[i];
      }
    }
    return null;
  }

  @Override
  public int size() {
    return count;
  }

  @Override
  public MenuItem getItem(int index) {
    if (index < 0 || index >= count) {
      throw new IndexOutOfBoundsException("menu item " + index + " of " + count);
    }
    return items[index];
  }

  @Override
  public boolean hasVisibleItems() {
    for (int i = 0; i < count; i++) {
      if (items[i].visible) {
        return true;
      }
    }
    return false;
  }

  /** The items that show, in display order: what the list presents. */
  Item[] visibleItems() {
    int n = 0;
    for (int i = 0; i < count; i++) {
      if (items[i].visible) {
        n++;
      }
    }
    Item[] out = new Item[n];
    int j = 0;
    for (int i = 0; i < count; i++) {
      if (items[i].visible) {
        out[j++] = items[i];
      }
    }
    return out;
  }

  /** One item; the mutators return {@code this}, as Android's do. */
  static final class Item implements MenuItem {
    final int groupId;
    final int itemId;
    final int order;
    CharSequence title;
    boolean enabled = true;
    boolean visible = true;
    OnMenuItemClickListener listener;

    Item(int groupId, int itemId, int order, CharSequence title) {
      this.groupId = groupId;
      this.itemId = itemId;
      this.order = order;
      this.title = title;
    }

    /** The item's own listener took the pick. */
    boolean fireClick() {
      return listener != null && listener.onMenuItemClick(this);
    }

    @Override
    public int getItemId() {
      return itemId;
    }

    @Override
    public int getGroupId() {
      return groupId;
    }

    @Override
    public int getOrder() {
      return order;
    }

    @Override
    public CharSequence getTitle() {
      return title;
    }

    @Override
    public MenuItem setTitle(CharSequence title) {
      this.title = title;
      return this;
    }

    @Override
    public MenuItem setTitle(int titleRes) {
      this.title = Resources.getInstance().getString(titleRes);
      return this;
    }

    @Override
    public MenuItem setEnabled(boolean enabled) {
      this.enabled = enabled;
      return this;
    }

    @Override
    public boolean isEnabled() {
      return enabled;
    }

    @Override
    public MenuItem setVisible(boolean visible) {
      this.visible = visible;
      return this;
    }

    @Override
    public boolean isVisible() {
      return visible;
    }

    @Override
    public MenuItem setOnMenuItemClickListener(OnMenuItemClickListener listener) {
      this.listener = listener;
      return this;
    }
  }
}
