// SPDX-License-Identifier: GPL-3.0-only
package picodroid.activity;

/**
 * Mirrors {@code androidx.activity.OnBackPressedCallback}: one handler of the system Back, added to
 * an Activity's {@link OnBackPressedDispatcher}. While it is {@linkplain #isEnabled enabled} and
 * the most recently added enabled callback, a Back press runs {@link #handleOnBackPressed} instead
 * of the Activity's default (pop the fragment back stack, else finish). Disable it rather than
 * removing and re-adding it when the screen's state says Back should do the usual thing again.
 *
 * <p>Divergence: no predictive-back progress ({@code handleOnBackStarted} and the rest): a Back is
 * a key or a tap here, never a gesture in flight.
 */
public abstract class OnBackPressedCallback {
  private boolean mEnabled;

  /** The dispatcher this callback is in, for {@link #remove}; null while it is in none. */
  OnBackPressedDispatcher mDispatcher;

  /** The lifecycle binding that adds and removes this callback, if it was added with an owner. */
  OnBackPressedDispatcher.LifecycleBinding mBinding;

  /** Mirrors Android: a callback that starts enabled or not. */
  public OnBackPressedCallback(boolean enabled) {
    mEnabled = enabled;
  }

  /** Mirrors Android: whether a Back press may reach {@link #handleOnBackPressed}. */
  public final void setEnabled(boolean enabled) {
    mEnabled = enabled;
  }

  /** Mirrors Android: whether this callback is enabled. */
  public final boolean isEnabled() {
    return mEnabled;
  }

  /**
   * Mirrors Android: takes this callback out of the dispatcher it was added to, and out of the
   * lifecycle it was added with; it can be added again.
   */
  public final void remove() {
    if (mBinding != null) {
      mBinding.cancel();
      mBinding = null;
    }
    if (mDispatcher != null) {
      mDispatcher.removeCallback(this);
    }
  }

  /** Mirrors Android: Back was pressed and this callback is the one to handle it. */
  public abstract void handleOnBackPressed();
}
