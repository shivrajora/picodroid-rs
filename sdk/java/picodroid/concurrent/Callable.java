// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

/** A task that returns a result and may throw, mirroring {@code java.util.concurrent.Callable}. */
public interface Callable<V> {
  V call() throws Exception;
}
