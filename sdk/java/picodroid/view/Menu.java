// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.Menu}: the actions an Activity offers from its options menu, added in
 * {@link picodroid.app.Activity#onCreateOptionsMenu}. The framework presents the menu as a list —
 * opened by a MENU key where a board has one, by holding SELECT on a four-key board, by the menu
 * control the framework draws on a touch board — and reports the pick to {@link
 * picodroid.app.Activity#onOptionsItemSelected}, so one set of actions reaches the user on every
 * board without the app mapping buttons to them.
 *
 * <p>Items keep the order they were added in, grouped by {@link MenuItem#getOrder()}. There are no
 * icons, sub-menus, shortcuts or checkable items.
 */
public interface Menu {
  /** Mirrors Android: no group, no id, no order. */
  int NONE = 0;

  /** Mirrors Android: the first item id an app may hand out. */
  int FIRST = 1;

  /** Mirrors Android: adds an item with {@code title} and no id. */
  MenuItem add(CharSequence title);

  /** Mirrors Android: adds an item titled by the string resource {@code titleRes}. */
  MenuItem add(int titleRes);

  /** Mirrors Android: adds an item; {@code itemId} is what {@link MenuItem#getItemId} returns. */
  MenuItem add(int groupId, int itemId, int order, CharSequence title);

  /** Mirrors Android: {@link #add(int, int, int, CharSequence)} with a string resource. */
  MenuItem add(int groupId, int itemId, int order, int titleRes);

  /** Mirrors Android: removes the item with {@code id}, if any. */
  void removeItem(int id);

  /** Mirrors Android: removes every item. */
  void clear();

  /** Mirrors Android: the item with {@code id}, or null. */
  MenuItem findItem(int id);

  /** Mirrors Android: how many items the menu has, visible or not. */
  int size();

  /** Mirrors Android: the item at {@code index}, in display order. */
  MenuItem getItem(int index);

  /** Mirrors Android: whether any item would show. */
  boolean hasVisibleItems();
}
