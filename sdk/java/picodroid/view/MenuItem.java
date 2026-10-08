// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.MenuItem}: one action of an Activity's {@link Menu}. Picked either
 * through its own {@link OnMenuItemClickListener} or, when that is absent or returns false, through
 * {@link picodroid.app.Activity#onOptionsItemSelected}. No icon, shortcut, checkable state or
 * action view.
 */
public interface MenuItem {
  /** Mirrors Android: notified when the item is picked. */
  interface OnMenuItemClickListener {
    /** Return true to consume the pick, false to let {@code onOptionsItemSelected} see it. */
    boolean onMenuItemClick(MenuItem item);
  }

  /** Mirrors Android: the id given at {@link Menu#add(int, int, int, CharSequence)}. */
  int getItemId();

  /** Mirrors Android: the group given at {@link Menu#add(int, int, int, CharSequence)}. */
  int getGroupId();

  /** Mirrors Android: the order given at {@link Menu#add(int, int, int, CharSequence)}. */
  int getOrder();

  /** Mirrors Android: the text the menu shows for this item. */
  CharSequence getTitle();

  MenuItem setTitle(CharSequence title);

  MenuItem setTitle(int titleRes);

  /** Mirrors Android: a disabled item still shows but a pick does nothing. */
  MenuItem setEnabled(boolean enabled);

  boolean isEnabled();

  /** Mirrors Android: a hidden item is left out of the menu. */
  MenuItem setVisible(boolean visible);

  boolean isVisible();

  MenuItem setOnMenuItemClickListener(OnMenuItemClickListener listener);
}
