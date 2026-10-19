// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Marks an observer of a {@link Lifecycle}; mirrors {@code androidx.lifecycle.LifecycleObserver}.
 * Implement {@link DefaultLifecycleObserver} for per-callback methods or {@link
 * LifecycleEventObserver} for one method per event (there is no annotation processor here, so a
 * bare {@code LifecycleObserver} hears nothing).
 */
public interface LifecycleObserver {}
