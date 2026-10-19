// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Hears every {@link Lifecycle.Event} of the owner it is added to; mirrors {@code
 * androidx.lifecycle.LifecycleEventObserver}.
 */
public interface LifecycleEventObserver extends LifecycleObserver {
  void onStateChanged(LifecycleOwner source, Lifecycle.Event event);
}
