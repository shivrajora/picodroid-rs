// SPDX-License-Identifier: GPL-3.0-only
package picodroid.di;

/**
 * A lazily-computed, memoized {@code T} — the picodroid counterpart of {@code dagger.Lazy}.
 *
 * <p>Inject {@code Lazy<T>} instead of {@code T} to defer an expensive construction (a peripheral
 * open, a buffer allocation) until first use while still holding exactly one instance per injection
 * site. The {@code @Inject} processor generates a {@code T_Lazy} implementation that calls {@code
 * T_Factory.get()} once and caches the result; for a {@code @Singleton} the cached value is the
 * shared instance. See {@code docs/designs/inject-annotations-2026-08.md}.
 */
public interface Lazy<T> {
  T get();
}
