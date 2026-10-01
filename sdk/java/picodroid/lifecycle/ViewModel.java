// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Holds the data a screen shows, apart from the views that show it; mirrors {@code
 * androidx.lifecycle.ViewModel}. An Activity and its fragments get the same instance from a {@link
 * ViewModelProvider} over the Activity, which is how fragments share data without knowing their
 * host's class.
 *
 * <p>A ViewModel lives as long as the Activity instance that owns it and gets {@link #onCleared}
 * when that Activity is destroyed. Android keeps it across a configuration change; there are none
 * here, and after {@code recreate()} or a reclaim the new Activity instance starts with a new
 * ViewModel, as an Android app does after process death. Never hold a view or an Activity in one.
 */
public abstract class ViewModel {
  public ViewModel() {}

  /** The owner is gone for good: let go of whatever this ViewModel holds. */
  protected void onCleared() {}

  final void clear() {
    onCleared();
  }
}
