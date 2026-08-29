// SPDX-License-Identifier: GPL-3.0-only
package javax.inject;

/**
 * Provides instances of {@code T} on demand (JSR-330 {@code javax.inject.Provider}).
 *
 * <p>Inject {@code Provider<T>} instead of {@code T} to defer construction to the call site, to get
 * a fresh instance per {@link #get()} for unscoped types (a {@code @Singleton} always returns the
 * one instance), or to break a dependency cycle. The {@code @Inject} processor generates a {@code
 * T_Provider} implementation that delegates to {@code T_Factory.get()}; a provider object holds no
 * state. See {@code docs/designs/inject-annotations-2026-08.md}.
 */
public interface Provider<T> {
  T get();
}
