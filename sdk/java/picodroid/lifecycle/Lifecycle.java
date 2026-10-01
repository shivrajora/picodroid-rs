// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.ArrayList;

/**
 * Where a {@link LifecycleOwner} is between creation and destruction, mirroring {@code
 * androidx.lifecycle.Lifecycle} and its {@code LifecycleRegistry} in one class. The framework moves
 * it with {@link #setCurrentState} as the owner's callbacks run: an owner is {@link #CREATED} after
 * its {@code onCreate}, {@link #STARTED} after {@code onStart}, {@link #RESUMED} after {@code
 * onResume}, and steps back down before {@code onPause}, {@code onStop} and {@code onDestroy}, as
 * on Android.
 *
 * <p>States are {@code int}s here rather than a {@code Lifecycle.State} enum, as {@link
 * picodroid.app.Fragment}'s are; compare with {@code >=} where Android says {@code isAtLeast}. Not
 * provided: {@code addObserver} with {@code LifecycleObserver} / {@code DefaultLifecycleObserver}
 * and the {@code Lifecycle.Event}s; {@link LiveData} is the one observer of a lifecycle.
 *
 * <p>A Lifecycle is also its own {@link LifecycleOwner}: that is what a fragment hands out for its
 * view, in place of a class per owner.
 */
public class Lifecycle implements LifecycleOwner {
  /** The owner is gone; a Lifecycle never leaves this state. */
  public static final int DESTROYED = 0;

  /** The owner exists and its {@code onCreate} has not returned yet. */
  public static final int INITIALIZED = 1;

  public static final int CREATED = 2;

  /** From here up the owner is on screen: {@link LiveData} delivers to its observers. */
  public static final int STARTED = 3;

  public static final int RESUMED = 4;

  private int mState = INITIALIZED;

  /** The {@link LiveData} observers bound to this lifecycle; null while there are none. */
  private ArrayList<LiveData.ObserverWrapper> mObservers;

  public Lifecycle() {}

  /** One of {@link #DESTROYED} … {@link #RESUMED}. */
  public int getCurrentState() {
    return mState;
  }

  /**
   * Moves to {@code state} and tells the bound observers; mirrors {@code
   * LifecycleRegistry.setCurrentState}. For the framework and for an app's own {@link
   * LifecycleOwner}. Main thread only.
   */
  public void setCurrentState(int state) {
    if (state == mState || mState == DESTROYED) {
      return;
    }
    mState = state;
    ArrayList<LiveData.ObserverWrapper> observers = mObservers;
    if (observers == null) {
      return;
    }
    // Newest first, by index: an observer told DESTROYED removes itself, and one that removes
    // another only makes this loop tell someone twice, which a wrapper ignores.
    for (int i = observers.size() - 1; i >= 0; i--) {
      if (i < observers.size()) {
        observers.get(i).onStateChanged(state);
      }
    }
    if (state == DESTROYED) {
      mObservers = null;
    }
  }

  @Override
  public Lifecycle getLifecycle() {
    return this;
  }

  void addObserver(LiveData.ObserverWrapper observer) {
    if (mObservers == null) {
      mObservers = new ArrayList<>();
    }
    mObservers.add(observer);
  }

  void removeObserver(LiveData.ObserverWrapper observer) {
    if (mObservers != null) {
      mObservers.remove(observer);
    }
  }
}
