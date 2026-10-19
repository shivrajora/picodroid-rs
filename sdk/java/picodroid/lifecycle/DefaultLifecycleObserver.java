// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * A {@link LifecycleObserver} with a no-op default per callback, so an observer overrides only what
 * it needs; mirrors {@code androidx.lifecycle.DefaultLifecycleObserver}. {@code owner} is the
 * {@link LifecycleOwner} the {@link Lifecycle} was made for (the Activity), or the Lifecycle
 * itself.
 */
public interface DefaultLifecycleObserver extends LifecycleObserver {
  default void onCreate(LifecycleOwner owner) {}

  default void onStart(LifecycleOwner owner) {}

  default void onResume(LifecycleOwner owner) {}

  default void onPause(LifecycleOwner owner) {}

  default void onStop(LifecycleOwner owner) {}

  default void onDestroy(LifecycleOwner owner) {}
}
