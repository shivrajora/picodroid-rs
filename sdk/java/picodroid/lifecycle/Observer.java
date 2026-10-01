// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/** Receives a {@link LiveData}'s values; mirrors {@code androidx.lifecycle.Observer}. */
public interface Observer<T> {
  /** The value was set, or the observer became active with a value it has not seen. */
  void onChanged(T value);
}
