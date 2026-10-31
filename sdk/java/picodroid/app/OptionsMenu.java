// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.annotation.Hide;
import picodroid.content.Context;
import picodroid.content.DialogInterface;
import picodroid.content.res.Resources;
import picodroid.view.Menu;
import picodroid.view.MenuItem;

/**
 * The {@link Menu} an Activity's options menu and a {@link picodroid.widget.Toolbar}'s own menu are
 * built in (Android's hidden {@code MenuBuilder}, cut to what the list presentation needs), and
 * that list, {@link #show}. Items sit in display order: by {@link MenuItem#getOrder()}, then as
 * added. A small fixed-growth array rather than a collection: a menu has a handful of actions.
 * Public only so the Toolbar in {@code picodroid.widget} can make one: an app reaches a menu
 * through {@code onCreateOptionsMenu} or {@code Toolbar.getMenu()}.
 */
public final class OptionsMenu implements Menu {
  private Item[] items = new Item[4];
  private int count;

  /** Told when an item comes, goes, or is shown or hidden: the Toolbar's overflow button. */
  private Runnable mOnChange;

  @Hide("Activity.onCreateOptionsMenu(Menu) or Toolbar.getMenu()")
  public OptionsMenu() {}

  /** Who to tell when the visible items may have changed. */
  @Hide("Toolbar.getMenu()")
  public void setOnChange(Runnable onChange) {
    mOnChange = onChange;
  }

  void changed() {
    if (mOnChange != null) {
      mOnChange.run();
    }
  }

  /**
   * Shows the visible items as a list over {@code context}'s screen. A pick runs the item's own
   * {@link MenuItem.OnMenuItemClickListener}, else {@code fallback}; a disabled item is listed
   * dimmed and cannot be picked (the list stays open over it, as Android's menu does). {@code
   * onDismiss} runs once however the list goes away: a pick, {@code dismiss()}, or BACK. Returns
   * the showing list, or {@code null} when there is nothing to show.
   */
  @Hide("Activity.openOptionsMenu() or Toolbar.showOverflowMenu()")
  public AlertDialog show(
      Context context,
      MenuItem.OnMenuItemClickListener fallback,
      DialogInterface.OnDismissListener onDismiss) {
    final Item[] shown = visibleItems();
    if (shown.length == 0) {
      return null;
    }
    String[] titles = new String[shown.length];
    int disabled = 0;
    for (int i = 0; i < shown.length; i++) {
      titles[i] = String.valueOf(shown[i].getTitle());
      if (!shown[i].isEnabled()) {
        disabled |= 1 << i;
      }
    }
    return new AlertDialog.Builder(context)
        .setDisabledItems(disabled)
        .setItems(
            titles,
            (dialog, which) -> {
              Item item = shown[which];
              if (item.isEnabled() && !item.fireClick() && fallback != null) {
                fallback.onMenuItemClick(item);
              }
            })
        .setOnDismissListener(onDismiss)
        .show();
  }

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
    Item item = new Item(this, groupId, itemId, order, title);
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
    changed();
    return item;
  }

  @Override
  public void removeItem(int id) {
    for (int i = 0; i < count; i++) {
      if (items[i].itemId == id) {
        System.arraycopy(items, i + 1, items, i, count - i - 1);
        items[--count] = null;
        changed();
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
    changed();
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
    /** The menu this item is in; null for the action bar's home item, which is in none. */
    private final OptionsMenu menu;

    final int groupId;
    final int itemId;
    final int order;
    CharSequence title;
    boolean enabled = true;
    boolean visible = true;
    OnMenuItemClickListener listener;

    Item(OptionsMenu menu, int groupId, int itemId, int order, CharSequence title) {
      this.menu = menu;
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
      boolean was = this.visible;
      this.visible = visible;
      if (menu != null && was != visible) {
        menu.changed();
      }
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
