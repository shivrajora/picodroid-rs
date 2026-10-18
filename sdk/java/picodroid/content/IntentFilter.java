// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * The actions a {@link BroadcastReceiver} wants, mirroring {@code android.content.IntentFilter}
 * narrowed to actions: there is no data, type or category matching, since nothing here resolves an
 * implicit Intent against a manifest.
 *
 * <pre>{@code
 * IntentFilter filter = new IntentFilter(Intent.ACTION_SCREEN_ON);
 * filter.addAction(Intent.ACTION_SCREEN_OFF);
 * registerReceiver(receiver, filter);
 * }</pre>
 *
 * <p>A filter holds at most {@link #MAX_ACTIONS} actions; Android has no such limit.
 */
public final class IntentFilter {
  /** The most actions one filter names. */
  public static final int MAX_ACTIONS = 4;

  private final String[] actions = new String[MAX_ACTIONS];
  private int count;

  /** A filter with no actions yet; it matches nothing until {@link #addAction}. */
  public IntentFilter() {}

  /** A filter for one action. */
  public IntentFilter(String action) {
    addAction(action);
  }

  /** Mirrors Android: a copy of {@code o}. */
  public IntentFilter(IntentFilter o) {
    if (o == null) {
      throw new NullPointerException("IntentFilter");
    }
    for (int i = 0; i < o.count; i++) {
      actions[i] = o.actions[i];
    }
    count = o.count;
  }

  /**
   * Add an action to match. Adding one the filter already names changes nothing, as on Android.
   *
   * @throws IllegalStateException when the filter already names {@link #MAX_ACTIONS} actions
   */
  public void addAction(String action) {
    if (action == null) {
      throw new NullPointerException("action");
    }
    if (hasAction(action)) {
      return;
    }
    if (count == MAX_ACTIONS) {
      throw new IllegalStateException("an IntentFilter names at most " + MAX_ACTIONS + " actions");
    }
    actions[count++] = action;
  }

  /** How many actions the filter names. */
  public final int countActions() {
    return count;
  }

  /**
   * The action at {@code index}, in the order added.
   *
   * @throws ArrayIndexOutOfBoundsException past the last one, as Android does
   */
  public final String getAction(int index) {
    if (index < 0 || index >= count) {
      throw new ArrayIndexOutOfBoundsException(index);
    }
    return actions[index];
  }

  /** Whether the filter names {@code action}. */
  public final boolean hasAction(String action) {
    if (action == null) {
      return false;
    }
    for (int i = 0; i < count; i++) {
      if (action.equals(actions[i])) {
        return true;
      }
    }
    return false;
  }

  /** Whether an Intent with this action matches the filter: the same test as {@link #hasAction}. */
  public final boolean matchAction(String action) {
    return hasAction(action);
  }

  /**
   * The system-action bits this filter names (see {@code BroadcastReceiver}); 0 for app actions.
   */
  int systemBits() {
    int mask = 0;
    for (int i = 0; i < count; i++) {
      mask |= BroadcastReceiver.bitOf(actions[i]);
    }
    return mask;
  }
}
