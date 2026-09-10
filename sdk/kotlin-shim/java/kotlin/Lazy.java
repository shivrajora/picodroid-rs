// SPDX-License-Identifier: GPL-3.0-only
package kotlin;

/**
 * A value computed on first access; {@code val x by lazy { ... }} reads it through {@link
 * #getValue}.
 */
public interface Lazy<T> {
  T getValue();

  boolean isInitialized();
}
