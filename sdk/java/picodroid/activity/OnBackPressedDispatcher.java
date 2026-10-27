// SPDX-License-Identifier: GPL-3.0-only
package picodroid.activity;

import picodroid.lifecycle.Lifecycle;
import picodroid.lifecycle.LifecycleEventObserver;
import picodroid.lifecycle.LifecycleOwner;

/**
 * Mirrors {@code androidx.activity.OnBackPressedDispatcher}: where an Activity's Back goes. Every
 * Activity has one ({@link picodroid.app.Activity#getOnBackPressedDispatcher}), and its {@link
 * picodroid.app.Activity#onBackPressed} hands each Back press to it. The most recently added
 * {@linkplain OnBackPressedCallback#isEnabled enabled} callback handles the press; with none, the
 * fallback runs — the Activity's default, which pops the fragment back stack or finishes.
 *
 * <p>Use it in place of overriding {@code onBackPressed} (deprecated on Android since API 33): a
 * screen that confirms before leaving adds a callback while it has unsaved edits, and a fragment
 * adds one with its view's {@link LifecycleOwner} so it only hears Back while it is on screen.
 *
 * <p>Divergence: the fragment back stack is popped after every app callback, where androidx puts
 * the fragment manager's own callback in the order it was added; the two agree for callbacks added
 * after {@code super.onCreate}. No predictive-back progress events.
 */
public final class OnBackPressedDispatcher {
  private final Runnable mFallback;

  /** The callbacks in the order they were added; the last enabled one handles a press. */
  private OnBackPressedCallback[] mCallbacks;

  private int mCount;

  /** Mirrors Android: a dispatcher whose Back does nothing when no callback takes it. */
  public OnBackPressedDispatcher() {
    this(null);
  }

  /**
   * Mirrors Android: a dispatcher that runs {@code fallbackOnBackPressed} when no enabled callback
   * takes a press.
   */
  public OnBackPressedDispatcher(Runnable fallbackOnBackPressed) {
    mFallback = fallbackOnBackPressed;
  }

  /**
   * Mirrors Android: adds {@code callback} above every callback already added; it hears Back until
   * it is {@linkplain OnBackPressedCallback#remove removed}. A callback already in a dispatcher is
   * moved here.
   */
  public void addCallback(OnBackPressedCallback callback) {
    if (callback.mDispatcher != null) {
      callback.mDispatcher.removeCallback(callback);
    }
    if (mCallbacks == null) {
      mCallbacks = new OnBackPressedCallback[2];
    } else if (mCount == mCallbacks.length) {
      OnBackPressedCallback[] grown = new OnBackPressedCallback[mCount * 2];
      System.arraycopy(mCallbacks, 0, grown, 0, mCount);
      mCallbacks = grown;
    }
    mCallbacks[mCount++] = callback;
    callback.mDispatcher = this;
  }

  /**
   * Mirrors Android: adds {@code callback} while {@code owner} is started — it is added on {@code
   * ON_START}, taken out on {@code ON_STOP} and dropped for good on {@code ON_DESTROY} — so a
   * fragment's callback hears Back only while its view is on screen. Nothing happens for an owner
   * already destroyed.
   */
  public void addCallback(LifecycleOwner owner, OnBackPressedCallback callback) {
    Lifecycle lifecycle = owner.getLifecycle();
    if (lifecycle.getCurrentState() == Lifecycle.State.DESTROYED) {
      return;
    }
    callback.remove();
    callback.mBinding = new LifecycleBinding(this, lifecycle, callback);
    lifecycle.addObserver(callback.mBinding);
  }

  /** Mirrors Android: whether any callback added here is enabled. */
  public boolean hasEnabledCallbacks() {
    for (int i = mCount - 1; i >= 0; i--) {
      if (mCallbacks[i].isEnabled()) {
        return true;
      }
    }
    return false;
  }

  /**
   * Mirrors Android: a Back press. The most recently added enabled callback handles it; with none,
   * the fallback runs.
   */
  public void onBackPressed() {
    for (int i = mCount - 1; i >= 0; i--) {
      OnBackPressedCallback callback = mCallbacks[i];
      if (callback.isEnabled()) {
        callback.handleOnBackPressed();
        return;
      }
    }
    if (mFallback != null) {
      mFallback.run();
    }
  }

  void removeCallback(OnBackPressedCallback callback) {
    for (int i = 0; i < mCount; i++) {
      if (mCallbacks[i] == callback) {
        System.arraycopy(mCallbacks, i + 1, mCallbacks, i, mCount - i - 1);
        mCallbacks[--mCount] = null;
        break;
      }
    }
    if (callback.mDispatcher == this) {
      callback.mDispatcher = null;
    }
  }

  /** Adds a callback on its owner's start and takes it out on its stop, until destroyed. */
  static final class LifecycleBinding implements LifecycleEventObserver {
    private final OnBackPressedDispatcher mDispatcher;
    private final Lifecycle mLifecycle;
    private final OnBackPressedCallback mCallback;

    LifecycleBinding(
        OnBackPressedDispatcher dispatcher, Lifecycle lifecycle, OnBackPressedCallback callback) {
      mDispatcher = dispatcher;
      mLifecycle = lifecycle;
      mCallback = callback;
    }

    @Override
    public void onStateChanged(LifecycleOwner source, Lifecycle.Event event) {
      if (event == Lifecycle.Event.ON_START) {
        mDispatcher.addCallback(mCallback);
      } else if (event == Lifecycle.Event.ON_STOP) {
        mDispatcher.removeCallback(mCallback);
      } else if (event == Lifecycle.Event.ON_DESTROY) {
        mCallback.remove();
      }
    }

    void cancel() {
      mLifecycle.removeObserver(this);
      mDispatcher.removeCallback(mCallback);
    }
  }
}
