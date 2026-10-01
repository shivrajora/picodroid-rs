// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.ArrayList;
import picodroid.concurrent.Executors;

/**
 * A value holder that tells its observers when the value changes, and only while they are on
 * screen; mirrors {@code androidx.lifecycle.LiveData}. An observer registered with {@link
 * #observe(LifecycleOwner, Observer)} is active while its owner is at least {@link
 * Lifecycle#STARTED}: it gets the current value when it becomes active (if it has not seen it),
 * every {@link #setValue} while it stays active, and is removed by itself when the owner is
 * destroyed.
 *
 * <pre>{@code
 * model.usage().observe(getViewLifecycleOwner(), usage -> render(usage));
 * }</pre>
 *
 * <p>{@code setValue} delivers synchronously, inside the call, to every active observer, as on
 * Android. On a slow board that is one main-thread tick doing every observer's work: an observer
 * with much to repaint should post the repaint to the next tick.
 *
 * <p>Everything but {@link #postValue} is main-thread only (not checked). Not provided: {@code
 * Transformations}, {@code MediatorLiveData}.
 */
public abstract class LiveData<T> {
  static final int START_VERSION = -1;

  /** The "no value yet" marker, so {@code null} can be a value. */
  private static final Object NOT_SET = new Object();

  private final ArrayList<ObserverWrapper> mObservers = new ArrayList<>();
  private int mActiveCount;
  private Object mData;
  private int mVersion;
  private boolean mDispatchingValue;
  private boolean mDispatchInvalidated;

  /** The value {@link #postValue} left for the main thread; guarded by {@code this}. */
  private Object mPendingData = NOT_SET;

  private Runnable mPostValueRunnable;

  /** A LiveData holding {@code value}. */
  public LiveData(T value) {
    mData = value;
    mVersion = START_VERSION + 1;
  }

  /** A LiveData with no value yet: {@link #getValue} is {@code null}, nothing is delivered. */
  public LiveData() {
    mData = NOT_SET;
    mVersion = START_VERSION;
  }

  /**
   * Adds {@code observer}, active while {@code owner} is started. Does nothing for an owner already
   * destroyed, or an observer already added with this owner; the same observer with another owner
   * throws.
   */
  public void observe(LifecycleOwner owner, Observer<? super T> observer) {
    Lifecycle lifecycle = owner.getLifecycle();
    if (lifecycle.getCurrentState() == Lifecycle.DESTROYED) {
      return;
    }
    ObserverWrapper existing = find(observer);
    if (existing != null) {
      if (existing.mLifecycle != lifecycle) {
        throw new IllegalArgumentException(
            "Cannot add the same observer with different lifecycles");
      }
      return;
    }
    ObserverWrapper wrapper = new ObserverWrapper(this, observer, lifecycle);
    mObservers.add(wrapper);
    lifecycle.addObserver(wrapper);
    wrapper.onStateChanged(lifecycle.getCurrentState());
  }

  /** Adds {@code observer}, always active, until {@link #removeObserver}. */
  public void observeForever(Observer<? super T> observer) {
    ObserverWrapper existing = find(observer);
    if (existing != null) {
      if (existing.mLifecycle != null) {
        throw new IllegalArgumentException(
            "Cannot add the same observer with different lifecycles");
      }
      return;
    }
    ObserverWrapper wrapper = new ObserverWrapper(this, observer, null);
    mObservers.add(wrapper);
    wrapper.activeStateChanged(true);
  }

  public void removeObserver(Observer<? super T> observer) {
    ObserverWrapper wrapper = find(observer);
    if (wrapper != null) {
      remove(wrapper);
    }
  }

  /** Removes every observer bound to {@code owner}. */
  public void removeObservers(LifecycleOwner owner) {
    Lifecycle lifecycle = owner.getLifecycle();
    for (int i = mObservers.size() - 1; i >= 0; i--) {
      ObserverWrapper wrapper = mObservers.get(i);
      if (wrapper.mLifecycle == lifecycle) {
        remove(wrapper);
      }
    }
  }

  /** The current value; {@code null} before the first one. */
  @SuppressWarnings("unchecked")
  public T getValue() {
    Object data = mData;
    return data != NOT_SET ? (T) data : null;
  }

  public boolean hasObservers() {
    return !mObservers.isEmpty();
  }

  public boolean hasActiveObservers() {
    return mActiveCount > 0;
  }

  /** Sets the value and delivers it to the active observers before returning. Main thread. */
  protected void setValue(T value) {
    mVersion++;
    mData = value;
    dispatchingValue(null);
  }

  /**
   * Sets the value from any thread: it is handed to the main thread, which then runs {@link
   * #setValue}. Of several values posted before the main thread gets there, the last one wins.
   */
  protected void postValue(T value) {
    synchronized (this) {
      mPendingData = value;
      if (mPostValueRunnable == null) {
        mPostValueRunnable = this::deliverPosted;
      }
    }
    // One post per call and no "posted" flag: the main queue drops a post when full, and a flag
    // would then swallow every later value.
    Executors.mainExecutor().execute(mPostValueRunnable);
  }

  @SuppressWarnings("unchecked")
  private void deliverPosted() {
    Object value;
    synchronized (this) {
      value = mPendingData;
      mPendingData = NOT_SET;
    }
    if (value != NOT_SET) {
      setValue((T) value);
    }
  }

  /** The number of active observers went from 0 to 1. */
  protected void onActive() {}

  /** The number of active observers went from 1 to 0. */
  protected void onInactive() {}

  private ObserverWrapper find(Observer<? super T> observer) {
    for (int i = 0; i < mObservers.size(); i++) {
      ObserverWrapper wrapper = mObservers.get(i);
      if (wrapper.mObserver == observer) {
        return wrapper;
      }
    }
    return null;
  }

  private void remove(ObserverWrapper wrapper) {
    mObservers.remove(wrapper);
    if (wrapper.mLifecycle != null) {
      wrapper.mLifecycle.removeObserver(wrapper);
    }
    wrapper.activeStateChanged(false);
    if (mDispatchingValue) {
      mDispatchInvalidated = true; // the list moved under the dispatch loop: go round again
    }
  }

  private void activeCountChanged(int by) {
    int before = mActiveCount;
    mActiveCount = before + by;
    if (before == 0 && mActiveCount > 0) {
      onActive();
    } else if (before > 0 && mActiveCount == 0) {
      onInactive();
    }
  }

  /**
   * Delivers to {@code initiator}, or to every observer when it is null. A value set from inside an
   * observer restarts the round, as on Android; versions keep anyone from seeing a value twice.
   */
  private void dispatchingValue(ObserverWrapper initiator) {
    if (mDispatchingValue) {
      mDispatchInvalidated = true;
      return;
    }
    mDispatchingValue = true;
    try {
      do {
        mDispatchInvalidated = false;
        if (initiator != null) {
          considerNotify(initiator);
          initiator = null;
        } else {
          for (int i = 0; i < mObservers.size(); i++) {
            considerNotify(mObservers.get(i));
            if (mDispatchInvalidated) {
              break;
            }
          }
        }
      } while (mDispatchInvalidated);
    } finally {
      mDispatchingValue = false; // an observer that threw must not silence every later value
    }
  }

  private void considerNotify(ObserverWrapper wrapper) {
    if (!wrapper.mActive) {
      return;
    }
    Lifecycle lifecycle = wrapper.mLifecycle;
    if (lifecycle != null && lifecycle.getCurrentState() < Lifecycle.STARTED) {
      wrapper.activeStateChanged(false);
      return;
    }
    if (wrapper.mLastVersion >= mVersion) {
      return;
    }
    wrapper.mLastVersion = mVersion;
    wrapper.mObserver.onChanged(mData);
  }

  /** One observer of one LiveData, with the lifecycle it is bound to (null: forever). */
  static final class ObserverWrapper {
    private final LiveData<?> mLiveData;
    final Observer<Object> mObserver;
    final Lifecycle mLifecycle;
    boolean mActive;
    int mLastVersion = START_VERSION;

    @SuppressWarnings("unchecked")
    ObserverWrapper(LiveData<?> liveData, Observer<?> observer, Lifecycle lifecycle) {
      mLiveData = liveData;
      mObserver = (Observer<Object>) observer;
      mLifecycle = lifecycle;
    }

    /** From {@link Lifecycle#setCurrentState}: the owner moved. */
    void onStateChanged(int state) {
      if (state == Lifecycle.DESTROYED) {
        mLiveData.remove(this);
        return;
      }
      activeStateChanged(state >= Lifecycle.STARTED);
    }

    void activeStateChanged(boolean active) {
      if (active == mActive) {
        return;
      }
      mActive = active;
      mLiveData.activeCountChanged(active ? 1 : -1);
      if (active) {
        mLiveData.dispatchingValue(this);
      }
    }
  }
}
